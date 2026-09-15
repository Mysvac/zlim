//! Typed asset storage: [`Assets<A>`] and the machinery around it.
//!
//! The module owns the storage of one asset type — a dense slot table plus a UUID map — the
//! [`AssetMut`] guard that queues [`AssetEvent::Modified`] only when the asset is actually
//! accessed mutably, the [`InvalidGenerationError`] a stale id produces, the [`AssetsIterator`],
//! [`AssetsMutIterator`] and [`AssetIdIterator`] over it, and the two per-type jobs: `asset_events`
//! drains the queued events into messages and the change table, `track_assets` processes dropped
//! handles.

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
use zlim_utils::sync::SpinLock;

use crate::asset::Asset;
use crate::change::AssetChanges;
use crate::event::AssetEvent;
use crate::handle::{AssetHandleProvider, Handle};
use crate::ident::{AssetId, AssetIndex, AssetIndexAllocator};

// -----------------------------------------------------------------------------
// AssetTable

struct Entry<A: Asset> {
    value: Option<A>,
    generation: u32,
}

impl<A: Asset> Entry<A> {
    const DEFAULT: Entry<A> = Entry::none(0);

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
    /// Grows `storage` to the allocator's high-water mark and installs a fresh empty entry for
    /// every recycled slot, at the generation the allocator bumped it to.
    fn flush(&mut self) {
        let new_len = self.allocator.next_index.load(Ordering::Relaxed);
        let len = new_len as usize;

        if len > self.storage.len() {
            ::core::hint::cold_path();
            self.storage.resize_with(len, || Some(Entry::<A>::DEFAULT));
        }

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

        let was_empty = value.is_none();
        if was_empty {
            self.len += 1;
        }

        *value = Some(asset);
        Ok(!was_empty)
    }

    /// Removes the value at `index` and hands the slot back to the allocator, so that a later
    /// `reserve` hands it out again under a bumped generation.
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

    /// Removes the value at `index` but keeps the slot at its current generation, so that strong
    /// handles still pointing at it can store a value there again.
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
/// [`get_mut_untracked`]: Self::get_mut_untracked
#[derive(TypePath, Resource)]
pub struct Assets<A: Asset> {
    table: AssetTable<A>,
    hash_map: HashMap<Uuid, A>,
    handle_provider: AssetHandleProvider,
    queued_events: Vec<AssetEvent<A>>,
    /// Extra strong handles that were upgraded from an [`AssetId`], per slot. They keep the
    /// slot alive until they are dropped, so recycling has to wait for the count to reach zero.
    duplicate_handles: HashMap<AssetIndex, u32>,
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
    /// Called by `track_assets` for every drop event that passes the server's bookkeeping check.
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
    #[doc(alias = "get_or_insert_with")]
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
            lock: None,
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
            lock: None,
        };

        Some(AssetMut { asset, notifier })
    }

    /// Upgrade an `AssetId` into a `Handle` that will prevent asset drop.
    ///
    /// Returns `None` if the provided `id` is not part of this `Assets` collection.
    ///
    /// StrongHandle created using this function cannot be serialized (skipped).
    ///
    /// # Panics
    ///
    /// Panics when the number of strong handles already upgraded for the same slot reaches the
    /// per-slot counter limit.
    #[doc(alias = "get_strong_handle")]
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

        if *counter >= const { u32::MAX >> 2 } {
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

    /// Iterates over all stored `(AssetId<A>, AssetMut<'_, A>)` pairs.
    ///
    /// Each yielded guard queues [`AssetEvent::Modified`] only if it is used to access the asset
    /// mutably (through [`DerefMut`], [`AssetMut::into_inner`] or [`AssetMut::set_changed`]);
    /// iterating alone queues nothing.
    #[inline]
    pub fn iter_mut(&mut self) -> AssetsMutIterator<'_, A> {
        AssetsMutIterator {
            locker: Arc::new(SpinLock::new(())),
            queued_events: &mut self.queued_events,
            dense: self.table.storage.iter_mut().enumerate(),
            uuid: self.hash_map.iter_mut(),
        }
    }

    /// Iterates over the ids of all stored assets.
    #[inline]
    #[doc(alias = "ids")]
    #[doc(alias = "iter_ids")]
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
    // AssetsMutIterator may produce multiple AssetMut values simultaneously, all of
    // which hold a mutable borrow of queued_events. If an AssetMut is sent to
    // multiple threads at this point, a data race would occur. Therefore, when we
    // use AssetsMutIterator to produce an AssetMut, a built-in lock must be provided.
    lock: Option<Arc<SpinLock<()>>>,
}

impl<A: Asset> Drop for AssetChangeNotifier<'_, A> {
    fn drop(&mut self) {
        if self.changed {
            let _guard = self.lock.as_ref().map(|l| l.lock());
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
    #[doc(alias = "into_inner_untracked")]
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

/// Iterator over the `(AssetId<A>, AssetMut<'a, A>)` pairs of an [`Assets<A>`].
pub struct AssetsMutIterator<'a, A: Asset> {
    locker: Arc<SpinLock<()>>,
    queued_events: &'a mut Vec<AssetEvent<A>>,
    dense: core::iter::Enumerate<core::slice::IterMut<'a, Option<Entry<A>>>>,
    uuid: zlim_utils::hash::map::IterMut<'a, Uuid, A>,
}

impl<'a, A: Asset> Iterator for AssetsMutIterator<'a, A> {
    type Item = (AssetId<A>, AssetMut<'a, A>);

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

            let ptr = self.queued_events as *mut Vec<_>;

            let notifier = AssetChangeNotifier {
                changed: false,
                id,
                #[expect(unsafe_code, reason = "ensured by locker")]
                queued_events: unsafe { &mut *ptr },
                lock: Some(self.locker.clone()),
            };

            let asset = AssetMut {
                asset: value,
                notifier,
            };
            return Some((id, asset));
        }

        let (uuid, value) = self.uuid.next()?;
        let id = AssetId::Uuid { uuid: *uuid };

        let ptr = self.queued_events as *mut Vec<_>;

        let notifier = AssetChangeNotifier {
            changed: false,
            id,
            #[expect(unsafe_code, reason = "ensured by locker")]
            queued_events: unsafe { &mut *ptr },
            lock: Some(self.locker.clone()),
        };
        let asset = AssetMut {
            asset: value,
            notifier,
        };

        Some((id, asset))
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

#[job_fn(type = HandleAssetEventsJob<A: Asset>, run_if = contains_asset_event::<A>)]
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

use crate::server::AssetServer;

#[job_fn(type = HandleAssetDropEventsJob<A: Asset>, run_if = contains_drop_event::<A>)]
fn track_assets<A: Asset>(mut assets: ResMut<Assets<A>>, asset_server: ResMut<AssetServer>) {
    let mut infos = asset_server.0.write_infos();
    while let Some(drop_event) = assets.handle_provider.try_recv() {
        if drop_event.asset_server_managed {
            // the `process_handle_drop` call checks whether new handles have been
            // created since the drop event was fired, before removing the asset
            if !infos.process_handle_drop(drop_event.index, drop_event.type_id) {
                // a new handle has been created, or the asset doesn't exist
                continue;
            }
        }

        assets.remove_dropped(drop_event.index);
    }
}

fn contains_drop_event<A: Asset>(assets: Res<Assets<A>>) -> bool {
    assets.handle_provider.has_drop_event()
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use zlim_app::{App, Last, Plugin, PluginExt};
    use zlim_core::component::Component;
    use zlim_core::message::MessageQueue;
    use zlim_core::world::World;
    use zlim_path::TypePath;

    use super::*;
    use crate::asset::{AssetComponent, VisitAssetDependencies};
    use crate::change::AssetChanged;
    use crate::ident::ErasedAssetId;
    use crate::plugin::{AppAssetExt, AssetPlugin};
    use crate::server::AssetServer;

    /// The asset the filter tracks.
    #[derive(TypePath)]
    struct Tracked;

    impl VisitAssetDependencies for Tracked {
        fn visit_dependencies(&self, _visit: &mut dyn FnMut(ErasedAssetId)) {}
    }

    impl Asset for Tracked {}

    /// A component that points at a `Tracked` asset, which is what the filter matches on.
    #[derive(TypePath, Component, Clone)]
    struct TrackedRef(Handle<Tracked>);

    impl AssetComponent for TrackedRef {
        type Asset = Tracked;

        fn asset_id(&self) -> AssetId<Tracked> {
            self.0.id()
        }
    }

    /// How many entities the filter matched in the frame that just ran.
    #[derive(TypePath, Resource, Default)]
    struct Matches(usize);

    #[job_fn(type = CountChangedRefs)]
    fn count_changed_refs(world: &mut World) {
        let matches = world.query::<(), AssetChanged<TrackedRef>>().iter().count();
        world.resource_mut::<Matches>().0 = matches;
    }

    /// Registers the asset type and the counting job.
    struct TrackedPlugin;

    impl Plugin for TrackedPlugin {
        fn build(&mut self, app: &mut App) {
            AssetPlugin::apply_before::<Self>(app);
        }

        fn apply(&mut self, app: &mut App) {
            app.init_asset::<Tracked>();

            let world = app.main_world_mut();
            world.insert_resource(Matches::default());
            // `Last` runs after `PostUpdate`, where the per-type event job drains `Assets<A>`.
            world.schedule_entry(Last).insert::<CountChangedRefs>(());
        }
    }

    /// The filter is wired to `Assets<A>`: adding or mutably borrowing an asset queues an event, the
    /// per-type event job turns it into a change tick, and the filter sees it for exactly one frame.
    #[test]
    fn the_changed_filter_sees_added_and_modified_assets() {
        let mut app = App::new();
        app.add_plugins((
            AssetPlugin {
                watch_for_changes_override: Some(false),
                ..AssetPlugin::default()
            },
            TrackedPlugin,
        ));
        app.build();

        let handle = app
            .main_world_mut()
            .resource_mut::<Assets<Tracked>>()
            .add(Tracked);
        app.main_world_mut().spawn(TrackedRef(handle.clone()), None);

        let matches = |app: &App| app.main_world().resource::<Matches>().0;

        // The `Added` event of the `add` above is drained during this frame.
        app.update();
        assert_eq!(matches(&app), 1, "a newly added asset matches");

        app.update();
        assert_eq!(matches(&app), 0, "the match does not last a second frame");

        // A guard that is never borrowed mutably is not a change: `get_mut` alone says nothing.
        {
            let world = app.main_world_mut();
            let mut assets = world.resource_mut::<Assets<Tracked>>();
            let _untouched = assets.get_mut(&handle).expect("the asset was just added");
        }

        app.update();
        assert_eq!(matches(&app), 0, "an untouched guard is not a change");

        // Borrowing the asset mutably is what queues `Modified`, which the next frame drains.
        {
            let world = app.main_world_mut();
            let mut assets = world.resource_mut::<Assets<Tracked>>();
            let mut guard = assets.get_mut(&handle).expect("the asset was just added");
            *guard = Tracked;
        }

        app.update();
        assert_eq!(matches(&app), 1, "a mutably borrowed asset matches again");

        app.update();
        assert_eq!(matches(&app), 0, "and only for that frame");
    }

    /// The ids of the `Unused` events currently queued.
    fn unused_ids(app: &App) -> Vec<AssetId<Tracked>> {
        let Some(queue) = app
            .main_world()
            .get_resource::<MessageQueue<AssetEvent<Tracked>>>()
        else {
            return Vec::new();
        };

        let mut ids = Vec::new();
        for index in queue.oldest_message_index()..queue.counter() {
            if let Some((_, AssetEvent::Unused { id })) = queue.get(index) {
                ids.push(*id);
            }
        }
        ids
    }

    /// Dropping the last strong handle releases the value and queues `Unused` — both for a handle
    /// this collection alone knows about and for one the server manages.
    #[test]
    fn dropping_the_last_handle_releases_the_asset() {
        let mut app = App::new();
        app.add_plugins((
            AssetPlugin {
                watch_for_changes_override: Some(false),
                ..AssetPlugin::default()
            },
            TrackedPlugin,
        ));
        app.build();

        let handle = app
            .main_world_mut()
            .resource_mut::<Assets<Tracked>>()
            .add(Tracked);
        let id = handle.id();

        app.update();
        assert!(
            app.main_world()
                .resource::<Assets<Tracked>>()
                .get(id)
                .is_some(),
            "the added value is stored",
        );

        ::core::mem::drop(handle);
        app.update();

        assert!(
            app.main_world()
                .resource::<Assets<Tracked>>()
                .get(id)
                .is_none(),
            "the value is released once its last handle is gone",
        );
        assert!(
            unused_ids(&app).contains(&id),
            "an `Unused` event is queued for the released asset, got {:?}",
            unused_ids(&app),
        );

        // A server-managed handle goes through the server's own bookkeeping as well.
        let server = app.main_world().resource::<AssetServer>().clone();
        let handle = server.add(Tracked);
        let id = handle.id();

        app.update();
        assert!(server.is_managed(id), "the server tracks the added asset");
        assert!(
            app.main_world()
                .resource::<Assets<Tracked>>()
                .get(id)
                .is_some(),
            "the added value is stored",
        );

        ::core::mem::drop(handle);
        app.update();

        assert!(
            !server.is_managed(id),
            "the server forgets the asset once its last handle is gone",
        );
        assert!(
            app.main_world()
                .resource::<Assets<Tracked>>()
                .get(id)
                .is_none(),
            "and the value is released",
        );
    }
}

// -----------------------------------------------------------------------------
