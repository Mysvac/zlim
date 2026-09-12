//! Typed asset storage: [`Assets<A>`].

use core::any::TypeId;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::Ordering;
use std::sync::Arc;

use uuid::Uuid;
use zlim_core::borrow::{Res, ResMut};
use zlim_core::derive::{Error, Resource};
use zlim_core::job_fn;
use zlim_core::message::MessageWriter;
use zlim_core::system::SystemTick;
use zlim_path::TypePath;
use zlim_utils::hash::HashMap;
use zlim_utils::hash::map::Entry as MapEntry;

use crate::asset::Asset;
use crate::change::AssetChanges;
use crate::event::AssetEvent;
use crate::handle::{AssetHandleProvider, ErasedHandle, Handle};
use crate::ident::{AssetId, AssetIndex, AssetIndexAllocator};

// -----------------------------------------------------------------------------
// AssetTable

struct Entry<A: Asset> {
    value: Option<A>,
    generation: u32,
}

impl<A: Asset> Entry<A> {
    const DEFAULT: Entry<A> = Entry {
        value: None,
        generation: 0,
    };

    #[inline(always)]
    const fn none(generation: u32) -> Self {
        Self {
            value: None,
            generation,
        }
    }
}

struct AssetTable<A: Asset> {
    storage: Vec<Option<Entry<A>>>,
    len: u32,
    allocator: Arc<AssetIndexAllocator>,
}

impl<A: Asset> Default for AssetTable<A> {
    #[inline]
    fn default() -> Self {
        Self {
            storage: Vec::new(),
            len: 0,
            allocator: Arc::new(AssetIndexAllocator::new()),
        }
    }
}

impl<A: Asset> AssetTable<A> {
    fn flush(&mut self) {
        let new_len = self.allocator.next_index.load(Ordering::Relaxed);
        let len = new_len as usize;
        self.storage.resize_with(len, || Some(Entry::<A>::DEFAULT));

        while let Some(recycled) = self.allocator.recycled.pop() {
            let index = recycled.index as usize;
            self.storage[index] = Some(Entry::<A>::none(recycled.generation));
        }
    }

    fn insert(&mut self, index: AssetIndex, asset: A) -> Result<bool, InvalidGenerationError> {
        self.flush();

        let entry = &mut self.storage[index.index as usize];

        let Some(entry) = entry.as_mut() else {
            return Err(InvalidGenerationError::Removed { index });
        };

        let Entry { value, generation } = entry;

        if *generation != index.generation {
            return Err(InvalidGenerationError::Occupied {
                index,
                current_generation: *generation,
            });
        }

        let replaced = value.is_none();
        if replaced {
            self.len += 1;
        }

        *value = Some(asset);
        Ok(!replaced)
    }

    fn remove_and_recycle(&mut self, index: AssetIndex) -> Option<A> {
        self.flush();

        let entry = self.storage[index.index as usize].as_mut()?;
        let Entry { value, generation } = entry;

        if *generation != index.generation {
            return None;
        }
        let value = value.take().inspect(|_| self.len -= 1);

        self.storage[index.index as usize] = None;
        self.allocator.recycle(index);

        value
    }

    fn remove_still_alive(&mut self, index: AssetIndex) -> Option<A> {
        self.flush();

        let entry = self.storage[index.index as usize].as_mut()?;
        let Entry { value, generation } = entry;

        if *generation != index.generation {
            return None;
        }

        value.take().inspect(|_| self.len -= 1)
    }

    fn get(&self, index: AssetIndex) -> Option<&A> {
        let entry = self.storage.get(index.index as usize)?;
        let Entry { value, generation } = entry.as_ref()?;
        (*generation == index.generation).then_some(value.as_ref())?
    }

    fn get_mut(&mut self, index: AssetIndex) -> Option<&mut A> {
        let entry = self.storage.get_mut(index.index as usize)?;
        let Entry { value, generation } = entry.as_mut()?;
        (*generation == index.generation).then_some(value.as_mut())?
    }

    #[inline]
    fn get_or_insert(
        &mut self,
        index: AssetIndex,
        f: impl FnOnce() -> A,
    ) -> Result<(&mut A, bool), InvalidGenerationError> {
        self.flush();

        let entry = &mut self.storage[index.index as usize];

        let Some(entry) = entry.as_mut() else {
            return Err(InvalidGenerationError::Removed { index });
        };

        let Entry { value, generation } = entry;

        if *generation != index.generation {
            return Err(InvalidGenerationError::Occupied {
                index,
                current_generation: *generation,
            });
        }

        if value.is_some() {
            Ok((value.as_mut().unwrap(), false))
        } else {
            self.len += 1;
            Ok((value.insert(f()), true))
        }
    }
}

/// Returned when an [`AssetIndex`] does not match the state of its slot.
///
/// This happens when a stale id (kept across a removal or past its generation) is used to
/// insert a value: the slot either belongs to a newer asset or was recycled.
#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InvalidGenerationError {
    /// The slot is occupied by a value of a different generation.
    #[error(
        "asset index {index} has an invalid generation; \
        the slot is at generation {current_generation}"
    )]
    Occupied {
        /// The index whose generation did not match.
        index: AssetIndex,
        /// The generation the slot currently has.
        current_generation: u32,
    },
    /// The slot was recycled and no longer holds an asset.
    #[error("asset index {index} has been removed")]
    Removed {
        /// The index that no longer exists.
        index: AssetIndex,
    },
}

// -----------------------------------------------------------------------------
// Assets
// -----------------------------------------------------------------------------

/// Typed storage for every loaded asset of type `A`.
///
/// One `Assets<A>` exists per asset type; it is registered as a `Resource` by
/// `App::init_asset::<A>()` (see the asset plugin). It owns both halves of the storage:
///
/// Assets identified by [`AssetId::Index`] will be stored in a "dense" vec-like storage.
/// This is more efficient, but it means that the assets can only be identified at runtime.
/// This is the default behavior.
///
/// Assets identified by [`AssetId::Uuid`] will be stored in a hashmap. This is less efficient,
/// but it means that the assets can be referenced at compile time.
///
/// Mutations queue [`AssetEvent`]s instead of writing messages directly, so that a system
/// changing many assets produces one batch per frame (the `asset_events` job in this module
/// drains the queue and also updates the per-type `AssetChanges` resource).
///
/// # Change tracking
///
/// [`get_mut`] returns an [`AssetMut`] guard which only queues [`AssetEvent::Modified`]
/// if the value was actually mutably accessed; [`get_mut_untracked`] skips the event entirely.
///
/// [`get_mut`]: Self::get_mut
/// [`uuid_handle!`]: crate::uuid_handle
/// [`get_mut_untracked`]: Self::get_mut_untracked
#[derive(TypePath, Resource)]
pub struct Assets<A: Asset> {
    table: AssetTable<A>,
    hash_map: HashMap<Uuid, A>,
    handle_provider: AssetHandleProvider,
    queued_events: Vec<AssetEvent<A>>,
    /// Extra strong handles that were upgraded from an [`AssetId`], per slot. They keep the
    /// slot alive until they are dropped, so recycling has to wait for the count to reach zero.
    duplicate_handles: HashMap<AssetIndex, u16>,
}

impl<A: Asset> Default for Assets<A> {
    fn default() -> Self {
        let table = AssetTable::default();
        let allocator = table.allocator.clone();
        let handle_provider = AssetHandleProvider::new(TypeId::of::<A>(), allocator);
        Self {
            table,
            hash_map: HashMap::new(),
            handle_provider,
            queued_events: Vec::new(),
            duplicate_handles: HashMap::new(),
        }
    }
}

impl<A: Asset> Assets<A> {
    /// Returns a clone of the handle provider backing this storage.
    ///
    /// The asset server adopts it through `AssetServer::register_asset`,
    /// so the handles it hands out address slots of this very storage.
    #[inline]
    pub fn handle_provider(&self) -> AssetHandleProvider {
        self.handle_provider.clone()
    }

    /// Reserves a handle for an asset that will be stored later.
    ///
    /// The slot stays empty (`contains` is `false`) until a value is inserted for the returned
    /// id, which is what `AssetServer::load` relies on while a load is in flight.
    #[inline]
    pub fn reserve_handle(&self) -> Handle<A> {
        self.handle_provider
            .reserve_handle()
            .with_type_debug_checked::<A>()
    }

    /// Returns `true` when nothing is stored.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.table.len == 0 && self.hash_map.is_empty()
    }

    /// Returns the number of stored assets, counting both index- and UUID-keyed ones.
    #[inline]
    pub fn len(&self) -> usize {
        (self.table.len as usize) + self.hash_map.len()
    }
}

impl<A: Asset> Assets<A> {
    /// Inserts the given `asset`, identified by the given `id`.
    ///
    /// If an asset already exists for `id`, it will be replaced.
    ///
    /// Note: This will never return an error for UUID asset IDs.
    #[inline]
    pub fn insert(
        &mut self,
        id: impl Into<AssetId<A>>,
        asset: A,
    ) -> Result<(), InvalidGenerationError> {
        self.insert_internal(id.into(), asset)
    }

    fn insert_internal(&mut self, id: AssetId<A>, asset: A) -> Result<(), InvalidGenerationError> {
        let replaced = match id {
            AssetId::Index { index, .. } => self.table.insert(index, asset)?,
            AssetId::Uuid { uuid } => self.hash_map.insert(uuid, asset).is_some(),
        };
        let event = if replaced {
            AssetEvent::Modified { id }
        } else {
            AssetEvent::Added { id }
        };
        self.queued_events.push(event);
        Ok(())
    }

    /// Adds the given `asset` and allocates a new strong [`Handle`] for it.
    ///
    /// Assets created using this function cannot be serialized (skipped).
    #[inline]
    pub fn add(&mut self, asset: impl Into<A>) -> Handle<A> {
        let index = self.table.allocator.reserve();
        self.add_internal(index, asset.into());
        Handle::Strong(self.handle_provider.build_handle(index, None, false))
    }

    fn add_internal(&mut self, index: AssetIndex, asset: A) {
        const EXP: &str = "a freshly reserved slot is always insertable";
        let replaced = self.table.insert(index, asset).expect(EXP);
        debug_assert!(!replaced, "a freshly reserved slot is always empty");
        let id = AssetId::<A>::from(index);
        self.queued_events.push(AssetEvent::Added { id });
    }

    /// Returns whether a value is stored under `id`.
    #[inline]
    pub fn contains(&self, id: impl Into<AssetId<A>>) -> bool {
        match id.into() {
            AssetId::Index { index, .. } => self.table.get(index).is_some(),
            AssetId::Uuid { uuid } => self.hash_map.contains_key(&uuid),
        }
    }
}

impl<A: Asset> Assets<A> {
    /// Removes the asset at `id` and queues [`AssetEvent::Removed`].
    ///
    /// An index-keyed slot is kept allocated: live strong handles (including duplicates)
    /// keep it alive, and it is only recycled once they are all dropped.
    ///
    /// See [`remove_untracked`](Self::remove_untracked) for the silent variant.
    #[inline]
    pub fn remove(&mut self, id: impl Into<AssetId<A>>) -> Option<A> {
        let id: AssetId<A> = id.into();
        let removed = match id {
            AssetId::Index { index, .. } => self.table.remove_still_alive(index),
            AssetId::Uuid { uuid } => self.hash_map.remove(&uuid),
        };
        if removed.is_some() {
            self.queued_events.push(AssetEvent::Removed { id });
        }
        removed
    }

    /// Removes the asset at `id` without queueing an event.
    #[inline]
    pub fn remove_untracked(&mut self, id: impl Into<AssetId<A>>) -> Option<A> {
        match id.into() {
            AssetId::Index { index, .. } => self.table.remove_still_alive(index),
            AssetId::Uuid { uuid } => self.hash_map.remove(&uuid),
        }
    }

    /// Recycles the slot of a dropped handle.
    ///
    /// Always queues [`AssetEvent::Unused`] — the last strong reference is gone — and
    /// additionally [`AssetEvent::Removed`] when a value was still stored at that point.
    ///
    /// Extra strong handles upgraded through [`resolve_handle`] defer the recycle:
    /// the counter is decremented first and the call returns early while duplicates are still alive.
    ///
    /// See [`asset_drops`] for details.
    ///
    /// [`resolve_handle`]: Self::resolve_handle
    fn remove_dropped(&mut self, index: AssetIndex) {
        match self.duplicate_handles.get_mut(&index) {
            None => {}
            Some(0) => {
                self.duplicate_handles.remove(&index);
            }
            Some(duplicates) => {
                *duplicates -= 1;
                return;
            }
        }

        let id = AssetId::<A>::from(index);
        self.queued_events.push(AssetEvent::Unused { id });

        if self.table.remove_and_recycle(index).is_some() {
            self.queued_events.push(AssetEvent::Removed { id });
        }
    }
}

impl<A: Asset> Assets<A> {
    /// Retrieves an [`Asset`] stored for the given `id` if it exists.
    ///
    /// If it does not exist, it will be inserted using `builder`, and the matching
    /// [`AssetEvent::Added`] is queued exactly like [`Assets::insert`] does — a created
    /// asset must be visible to change tracking and to the event pump.
    ///
    /// Note: This will never return an error for UUID asset IDs.
    #[inline]
    pub fn get_or_insert(
        &mut self,
        id: impl Into<AssetId<A>>,
        builder: impl FnOnce() -> A,
    ) -> Result<AssetMut<'_, A>, InvalidGenerationError> {
        let id: AssetId<A> = id.into();

        let (asset, created) = match id {
            AssetId::Index { index, .. } => self.table.get_or_insert(index, builder)?,
            AssetId::Uuid { uuid } => match self.hash_map.entry(uuid) {
                MapEntry::Occupied(entry) => (entry.into_mut(), false),
                MapEntry::Vacant(entry) => (entry.insert(builder()), true),
            },
        };

        if created {
            self.queued_events.push(AssetEvent::Added { id });
        }

        let notifier = AssetChangeNotifier {
            changed: false,
            id,
            queued_events: &mut self.queued_events,
        };

        Ok(AssetMut { asset, notifier })
    }

    /// Returns the asset at `id`.
    #[inline]
    pub fn get(&self, id: impl Into<AssetId<A>>) -> Option<&A> {
        match id.into() {
            AssetId::Index { index, .. } => self.table.get(index),
            AssetId::Uuid { uuid } => self.hash_map.get(&uuid),
        }
    }

    /// Returns a mutable reference to the asset at `id` without queueing an event.
    #[inline]
    #[doc(alias = "get_bypass")]
    pub fn get_mut_untracked(&mut self, id: impl Into<AssetId<A>>) -> Option<&mut A> {
        match id.into() {
            AssetId::Index { index, .. } => self.table.get_mut(index),
            AssetId::Uuid { uuid } => self.hash_map.get_mut(&uuid),
        }
    }

    /// Returns a change-tracking mutable reference to the asset at `id`.
    ///
    /// [`AssetEvent::Modified`] is queued on drop, and only if the guard was used to get a
    /// `&mut A` (through [`DerefMut`] or [`AssetMut::into_inner`]).
    #[inline]
    pub fn get_mut(&mut self, id: impl Into<AssetId<A>>) -> Option<AssetMut<'_, A>> {
        let id: AssetId<A> = id.into();

        let asset = match id {
            AssetId::Index { index, .. } => self.table.get_mut(index),
            AssetId::Uuid { uuid } => self.hash_map.get_mut(&uuid),
        }?;

        let notifier = AssetChangeNotifier {
            changed: false,
            id,
            queued_events: &mut self.queued_events,
        };

        Some(AssetMut { asset, notifier })
    }

    /// Upgrade an `AssetId` into a `Handle` that will prevent asset drop.
    ///
    /// Returns `None` if the provided `id` is not part of this `Assets` collection.
    ///
    /// StrongHandle created using this function cannot be serialized (skipped).
    #[doc(alias = "get_handle")]
    pub fn resolve_handle(&mut self, id: AssetId<A>) -> Option<Handle<A>> {
        if !self.contains(id) {
            return None;
        }

        let index = match id {
            AssetId::Index { index, .. } => index,
            AssetId::Uuid { uuid } => return Some(Handle::Uuid(uuid, PhantomData)),
        };

        #[cold]
        #[inline(never)]
        fn overflow(name: &str) -> ! {
            panic!("the number of strong handles for {name} reached the limit")
        }

        let counter = self.duplicate_handles.entry(index).or_insert(0);

        if *counter == u16::MAX {
            overflow(core::any::type_name::<A>());
        }

        *counter += 1;

        let p = &self.handle_provider;
        Some(Handle::Strong(p.build_handle(index, None, false)))
    }
}

impl<A: Asset> Assets<A> {
    /// Iterates over all stored `(id, &A)` pairs.
    #[inline]
    pub fn iter(&self) -> AssetsIterator<'_, A> {
        AssetsIterator {
            dense: self.table.storage.iter().enumerate(),
            uuid: self.hash_map.iter(),
        }
    }

    /// Iterates over all stored `(id, &mut A)` pairs,
    /// queueing [`AssetEvent::Modified`] for every yielded asset.
    #[inline]
    pub fn iter_mut(&mut self) -> AssetsMutIterator<'_, A> {
        AssetsMutIterator {
            queued_events: &mut self.queued_events,
            dense: self.table.storage.iter_mut().enumerate(),
            uuid: self.hash_map.iter_mut(),
        }
    }

    /// Iterates over the ids of all stored assets.
    #[inline]
    pub fn iter_id(&self) -> AssetIdIterator<'_, A> {
        AssetIdIterator {
            dense: self.table.storage.iter().enumerate(),
            uuid: self.hash_map.keys(),
        }
    }
}

// -----------------------------------------------------------------------------
// AssetMut
// -----------------------------------------------------------------------------

struct AssetChangeNotifier<'a, A: Asset> {
    changed: bool,
    id: AssetId<A>,
    queued_events: &'a mut Vec<AssetEvent<A>>,
}

impl<A: Asset> Drop for AssetChangeNotifier<'_, A> {
    fn drop(&mut self) {
        if self.changed {
            let event = AssetEvent::Modified { id: self.id };
            self.queued_events.push(event);
        }
    }
}

/// Unique mutable borrow of an asset.
///
/// [`AssetEvent::Modified`] events will be only
/// triggered if an asset itself is mutably borrowed.
pub struct AssetMut<'a, A: Asset> {
    asset: &'a mut A,
    notifier: AssetChangeNotifier<'a, A>,
}

impl<'a, A: Asset> AssetMut<'a, A> {
    /// Consumes the guard, always queueing [`AssetEvent::Modified`].
    #[inline]
    pub fn into_inner(mut self) -> &'a mut A {
        self.notifier.changed = true;
        self.asset
    }

    /// Consumes the guard and returns a mutable reference
    /// without triggering change detection.
    #[inline]
    pub fn bypass_inner(self) -> &'a mut A {
        self.asset
    }

    /// Returns a mutable reference to the inner value without
    /// triggering change detection.
    #[inline]
    pub fn bypass(&mut self) -> &mut A {
        self.asset
    }

    /// Manually marks this asset as having been changed.
    #[inline]
    pub fn set_changed(&mut self) {
        self.notifier.changed = true;
    }
}

impl<A: Asset> Deref for AssetMut<'_, A> {
    type Target = A;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.asset
    }
}

impl<A: Asset> DerefMut for AssetMut<'_, A> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.notifier.changed = true;
        self.asset
    }
}

// -----------------------------------------------------------------------------
// AssetsIterator

/// Iterator over the `(AssetId<A>, &A)` pairs of an [`Assets<A>`].
///
/// Index-keyed assets are yielded first (in slot order), then the UUID-keyed ones.
pub struct AssetsIterator<'a, A: Asset> {
    dense: core::iter::Enumerate<core::slice::Iter<'a, Option<Entry<A>>>>,
    uuid: zlim_utils::hash::map::Iter<'a, Uuid, A>,
}

impl<'a, A: Asset> Iterator for AssetsIterator<'a, A> {
    type Item = (AssetId<A>, &'a A);

    fn next(&mut self) -> Option<Self::Item> {
        for (index, entry) in self.dense.by_ref() {
            let Some(entry) = entry else {
                continue;
            };
            let Entry { value, generation } = entry;

            let Some(value) = value else {
                continue;
            };

            let index = index as u32;
            let generation = *generation;

            let id: AssetId<A> = AssetId::Index {
                index: AssetIndex { index, generation },
                marker: PhantomData,
            };

            return Some((id, value));
        }

        let (uuid, value) = self.uuid.next()?;
        Some((AssetId::Uuid { uuid: *uuid }, value))
    }
}

// -----------------------------------------------------------------------------
// AssetsMutIterator

/// Iterator over the `(AssetId<A>, &mut A)` pairs of an [`Assets<A>`].
///
/// Every yielded asset is marked as modified: an [`AssetEvent::Modified`] is queued for it as
/// soon as it is returned.
pub struct AssetsMutIterator<'a, A: Asset> {
    queued_events: &'a mut Vec<AssetEvent<A>>,
    dense: core::iter::Enumerate<core::slice::IterMut<'a, Option<Entry<A>>>>,
    uuid: zlim_utils::hash::map::IterMut<'a, Uuid, A>,
}

impl<'a, A: Asset> Iterator for AssetsMutIterator<'a, A> {
    type Item = (AssetId<A>, &'a mut A);

    fn next(&mut self) -> Option<Self::Item> {
        for (index, entry) in self.dense.by_ref() {
            let Some(entry) = entry else {
                continue;
            };
            let Entry { value, generation } = entry;

            let Some(value) = value else {
                continue;
            };

            let index = index as u32;
            let generation = *generation;

            let id: AssetId<A> = AssetId::Index {
                index: AssetIndex { index, generation },
                marker: PhantomData,
            };

            self.queued_events.push(AssetEvent::Modified { id });
            return Some((id, value));
        }

        let (uuid, value) = self.uuid.next()?;
        let id = AssetId::Uuid { uuid: *uuid };
        self.queued_events.push(AssetEvent::Modified { id });
        Some((id, value))
    }
}

// -----------------------------------------------------------------------------
// AssetIdIterator

/// Iterator over the [`AssetId`]s of an [`Assets<A>`].
pub struct AssetIdIterator<'a, A: Asset> {
    dense: core::iter::Enumerate<core::slice::Iter<'a, Option<Entry<A>>>>,
    uuid: zlim_utils::hash::map::Keys<'a, Uuid, A>,
}

impl<A: Asset> Iterator for AssetIdIterator<'_, A> {
    type Item = AssetId<A>;

    fn next(&mut self) -> Option<Self::Item> {
        for (index, entry) in self.dense.by_ref() {
            let Some(entry) = entry else {
                continue;
            };
            let Entry { value, generation } = entry;
            if value.is_none() {
                continue;
            }

            let index = index as u32;
            let generation = *generation;

            return Some(AssetId::Index {
                index: AssetIndex { index, generation },
                marker: PhantomData,
            });
        }

        let uuid = self.uuid.next()?;

        Some(AssetId::Uuid { uuid: *uuid })
    }
}

// -----------------------------------------------------------------------------
// AssetEvents

#[job_fn(type = HandleAssetEvents<A: Asset>, run_if = contains_asset_event::<A>)]
fn asset_events<A: Asset>(
    mut assets: ResMut<Assets<A>>,
    mut messages: MessageWriter<AssetEvent<A>>,
    asset_changes: Option<ResMut<AssetChanges<A>>>,
    ticks: SystemTick,
) {
    use AssetEvent::{Added, FullyLoaded, Modified, Removed, Unused};

    if let Some(mut asset_changes) = asset_changes {
        for new_event in &assets.queued_events {
            match new_event {
                Removed { id } | Unused { id } => asset_changes.remove(id),
                Added { id } | Modified { id } | FullyLoaded { id } => {
                    asset_changes.insert(*id, ticks.this_run);
                }
            };
        }
    }

    messages.write_batch(assets.queued_events.drain(..));
}

fn contains_asset_event<A: Asset>(assets: Res<Assets<A>>) -> bool {
    !assets.queued_events.is_empty()
}

// -----------------------------------------------------------------------------
// AssetServer
// -----------------------------------------------------------------------------

#[job_fn(type = HandleAssetDropEvents<A: Asset>)]
fn asset_drops<A: Asset>(mut assets: ResMut<Assets<A>>) {
    while let Some(drop_event) = assets.handle_provider.try_recv() {
        if drop_event.asset_server_managed {
            // TODO(server): a server-managed handle should also consult `AssetInfos`
            // (`asset_server_managed` distinguishes the two) so that an unloaded asset can be
            // re-loaded on demand. Until M2 drops it like any other handle.
        }

        assets.remove_dropped(drop_event.index);
    }
}

// -----------------------------------------------------------------------------

/// A "loaded folder" containing handles for all assets stored in a given [`AssetPath`].
///
/// [`AssetPath`]: crate::path::AssetPath
#[derive(Asset, TypePath)]
#[type_path = "zlim_asset::assets::LoadedFolder"]
pub struct LoadedFolder {
    /// The handles of all assets stored in the folder.
    #[asset(dependency)]
    pub handles: Vec<ErasedHandle>,
}

/// A "loaded asset" containing the handle of the asset that was loaded without knowing its type.
///
/// [`AssetPath`]: crate::path::AssetPath
#[derive(Asset, TypePath)]
#[type_path = "zlim_asset::assets::LoadedUntypedAsset"]
pub struct LoadedUntypedAsset {
    /// The handle of the loaded asset, typed only at runtime.
    #[asset(dependency)]
    pub handle: ErasedHandle,
}

// -----------------------------------------------------------------------------
