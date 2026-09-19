use core::any::{Any, TypeId};
use core::task::Waker;
use std::sync::{Arc, Weak};

use zlim_core::world::World;
use zlim_task::Task;
use zlim_utils::ext::TypeMap;
use zlim_utils::hash::{HashMap, HashSet};
use zlim_utils::sync::SegQueue;

use crate::asset::Asset;
use crate::error::{AssetLoadError, MissingHandleProvider};
use crate::handle::{AssetHandleProvider, ErasedHandle, Handle, StrongHandle};
use crate::ident::{AssetIndex, ErasedAssetId, TypedAssetIndex};
use crate::loaded::ErasedLoadedAsset;
use crate::meta::AssetHash;
use crate::path::AssetPath;
use crate::server::AssetServerEvent;
use crate::server::state::{DependencyLoadState, LoadState, RecursiveDependencyLoadState};

// -----------------------------------------------------------------------------
// HandleLoadingMode

/// Determines how a handle should be initialized
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum HandleLoadingMode {
    /// The handle is for an asset that isn't loading/loaded yet.
    NotLoading,
    /// The handle is for an asset that is being _requested_ to load (if it isn't already loading)
    Request,
    /// The handle is for an asset that is being forced to load (even if it has already loaded)
    Force,
}

// -----------------------------------------------------------------------------
// AssetInfo

/// Internal tracking record for a single managed asset.
#[derive(Debug)]
pub(crate) struct AssetInfo {
    pub(crate) weak_handle: Weak<StrongHandle>,
    pub(crate) path: Option<AssetPath<'static>>,

    pub(crate) load_state: LoadState,
    pub(crate) dep_load_state: DependencyLoadState,
    pub(crate) rec_dep_load_state: RecursiveDependencyLoadState,

    /// Forward index: direct dependencies of this asset that are still loading.
    pub(crate) loading_dependencies: HashSet<TypedAssetIndex>,
    /// Forward index: direct dependencies of this asset that have failed.
    pub(crate) failed_dependencies: HashSet<TypedAssetIndex>,

    /// Forward index: all transitive dependencies of this asset that are still loading.
    pub(crate) loading_rec_dependencies: HashSet<TypedAssetIndex>,
    /// Forward index: all transitive dependencies of this asset that have failed.
    pub(crate) failed_rec_dependencies: HashSet<TypedAssetIndex>,

    /// Reverse index: assets that are directly waiting on this asset to finish loading.
    pub(crate) dependents_waiting_on_load: HashSet<TypedAssetIndex>,
    /// Reverse index: assets that are waiting on this asset's full recursive dependency load.
    pub(crate) dependents_waiting_on_recursive_dep_load: HashSet<TypedAssetIndex>,

    /// The asset paths required to *load this asset*.
    pub(crate) loader_dependencies: HashMap<AssetPath<'static>, AssetHash>,

    /// The number of handle drops to skip for this asset.
    pub(crate) handle_drops_to_skip: usize,

    /// List of tasks waiting for this asset to complete loading
    pub(crate) waiting_tasks: Vec<Waker>,
}

impl AssetInfo {
    #[inline]
    fn new(weak_handle: Weak<StrongHandle>, path: Option<AssetPath<'static>>) -> Self {
        Self {
            weak_handle,
            path,
            load_state: LoadState::NotLoaded,
            dep_load_state: DependencyLoadState::NotLoaded,
            rec_dep_load_state: RecursiveDependencyLoadState::NotLoaded,
            loading_dependencies: HashSet::new(),
            failed_dependencies: HashSet::new(),
            loading_rec_dependencies: HashSet::new(),
            failed_rec_dependencies: HashSet::new(),
            loader_dependencies: HashMap::new(),
            dependents_waiting_on_load: HashSet::new(),
            dependents_waiting_on_recursive_dep_load: HashSet::new(),
            handle_drops_to_skip: 0,
            waiting_tasks: Vec::new(),
        }
    }
}

// -----------------------------------------------------------------------------
// AssetInfos

type DepLoadedEventSender = fn(&mut World, AssetIndex);
type DepFailedEventSender = fn(&mut World, AssetIndex, AssetPath<'static>, AssetLoadError);

/// Reads the value of an asset of one type out of the [`World`].
///
/// The values live in `Assets<A>`, which only the world has, so anything that works from a
/// [`Handle`] alone — saving one, for instance — goes through one of these,
/// registered per asset type by [`register_asset`].
///
/// [`Handle`]: crate::handle::Handle
/// [`register_asset`]: super::AssetServer::register_asset
pub(crate) type GetAssetFn = fn(&World, ErasedAssetId) -> Option<&(dyn Any + Send + Sync)>;

/// Central registry of all assets known to the [`AssetServer`].
///
/// Stores [`AssetInfo`] records keyed by [`TypedAssetIndex`], maintains path-to-index
/// look-ups, and drives dependency-load-state propagation.
///
/// [`AssetServer`]: super::AssetServer
#[derive(Default)]
pub(crate) struct AssetInfos {
    /// One record per managed asset, keyed by its typed index.
    pub infos: HashMap<TypedAssetIndex, AssetInfo>,

    /// The assets that have a path, looked up by that path and then by asset type.
    pub path_to_index: HashMap<AssetPath<'static>, TypeMap<AssetIndex>>,

    /// The handle allocator of each registered asset type. A type without one cannot be handled
    /// at all, which is what [`unwrap_with_context`] turns into a panic.
    pub handle_providers: TypeMap<AssetHandleProvider>,

    /// Reads an asset value of one type out of the world, registered with the handle provider.
    pub asset_getters: TypeMap<GetAssetFn>,

    /// Whether the server follows its sources for changes, which is what the reverse indexes
    /// below are maintained for.
    pub watching_for_changes: bool,

    /// Reverse index: maps each asset path to the set of paths that depend on it for hot-reloading.
    pub loader_dependents: HashMap<AssetPath<'static>, HashSet<AssetPath<'static>>>,

    /// Reverse index: the labels still alive under each label-stripped base path, so that a
    /// sub-asset can be reloaded after the handle of its base asset is gone.
    pub living_labeled_assets: HashMap<AssetPath<'static>, HashSet<Arc<str>>>,

    /// Sends the typed "this asset and its dependencies have loaded" event, per asset type.
    pub dependency_loaded_event_sender: TypeMap<DepLoadedEventSender>,
    /// Sends the typed "this asset failed to load" event, per asset type.
    pub dependency_failed_event_sender: TypeMap<DepFailedEventSender>,

    /// The task currently loading each asset, keyed by that asset's index. It is kept so that a
    /// dropped task can cancel the load it drives — the entry goes away with the asset's last
    /// handle — and so that the `ClearFinishedAssetTask` job can forget the finished ones.
    pub pending_tasks: HashMap<TypedAssetIndex, Task<()>>,
}

impl core::fmt::Debug for AssetInfos {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AssetInfos")
            .field("path_to_index", &self.path_to_index)
            .field("infos", &self.infos)
            .finish()
    }
}

// -----------------------------------------------------------------------------
// unwrap_with_context

/// Unwraps the result of a handle allocation, turning a missing handle provider into a panic.
///
/// `debug_name` is the name of the asset type to put in that message, for the callers that know
/// it; a caller that only has the type id passes [`None`] and gets the id in the message instead.
///
/// # Panics
///
/// Panics when `result` is a [`MissingHandleProvider`] — the asset type was never initialized —
/// with a message naming the `app.init_asset::<T>()` call the caller has to add.
#[inline(always)]
pub(crate) fn unwrap_with_context<T>(
    result: Result<T, MissingHandleProvider>,
    type_id: TypeId,
    debug_name: Option<&'static str>,
) -> T {
    #[cold]
    #[inline(never)]
    fn handle_error(type_id: TypeId, debug_name: Option<&'static str>) -> ! {
        let hint = debug_name.unwrap_or("_unknown_");
        let type_name = debug_name.unwrap_or("(actual asset type)");

        panic!(
            "Cannot allocate an AssetHandle of type '{hint}({type_id:?})' because the asset type \
            has not been initialized. Make sure you have called `app.init_asset::<{type_name}>()`"
        );
    }

    match result {
        Ok(value) => value,
        Err(_) => handle_error(type_id, debug_name),
    }
}

// -----------------------------------------------------------------------------
// get_or_alloc_handle_internal

impl AssetInfos {
    /// Allocates a new handle for an asset identified by `type_id`.
    ///
    /// - Allocates a fresh [`AssetInfo`] and inserts it into the `infos` map.
    /// - Does **not** insert into `path_to_index`; callers are responsible for that.
    /// - If `loading` is `true`, all three load-state fields are set to `Loading`.
    /// - If `watching_for_changes` is `true` and the path contains a label, the label
    ///   is recorded in `living_labeled_assets`.
    #[inline(never)]
    fn alloc_internal(
        infos: &mut HashMap<TypedAssetIndex, AssetInfo>,
        handle_providers: &TypeMap<AssetHandleProvider>,
        living_labeled_assets: &mut HashMap<AssetPath<'static>, HashSet<Arc<str>>>,
        watching_for_changes: bool,
        loading: bool,
        type_id: TypeId,
        path: Option<AssetPath<'static>>,
    ) -> Result<ErasedHandle, MissingHandleProvider> {
        let Some(provider) = handle_providers.get(type_id) else {
            ::core::hint::cold_path();
            return Err(MissingHandleProvider(type_id));
        };

        if watching_for_changes && let Some(path) = &path {
            let mut without_label = path.clone();
            if let Some(label) = without_label.remove_label() {
                let label: Arc<str> = match label {
                    atomicow::CowArc::Borrowed(x) => Arc::from(x),
                    atomicow::CowArc::Static(x) => Arc::from(x),
                    atomicow::CowArc::Owned(x) => x,
                };
                let entry = living_labeled_assets.entry(without_label);
                entry.or_default().insert(label);
            }
        }

        let handle = provider.alloc_handle(path.clone(), true);
        let mut info = AssetInfo::new(Arc::downgrade(&handle), path);

        if loading {
            info.load_state = LoadState::Loading;
            info.dep_load_state = DependencyLoadState::Loading;
            info.rec_dep_load_state = RecursiveDependencyLoadState::Loading;
        }

        infos.insert(TypedAssetIndex::new(handle.index, handle.type_id), info);

        Ok(ErasedHandle::Strong(handle))
    }

    /// Returns an existing handle for `(path, type_id)`, or creates a new one.
    ///
    /// - If no entry exists, delegates to [`Self::alloc_internal`] and inserts
    ///   the resulting index into `path_to_index`.
    /// - If an entry exists but all strong handles have been dropped (the weak ref is
    ///   dead), a new strong handle is allocated for the same `AssetIndex` and
    ///   `handle_drops_to_skip` is incremented so the pending drop event is ignored.
    #[inline(never)]
    fn get_or_alloc_internal(
        &mut self,
        path: AssetPath<'static>,
        type_id: TypeId,
        loading_mode: HandleLoadingMode,
    ) -> Result<(ErasedHandle, bool), MissingHandleProvider> {
        use zlim_utils::ext::type_map::TypeMapEntry;

        let key = path.clone();
        let handles = self.path_to_index.entry(key).or_default();

        match handles.entry(type_id) {
            TypeMapEntry::Occupied(entry) => {
                let index = *entry.get();

                // if there is a path_to_index entry, info always exists
                let typedindex = TypedAssetIndex::new(index, type_id);
                let info = self.infos.get_mut(&typedindex).unwrap();

                let handle = if let Some(strong_handle) = info.weak_handle.upgrade() {
                    // If we can upgrade the handle, there is at least one live handle right now,
                    // The asset load has already kicked off (and maybe completed), so we can just
                    // return a strong handle
                    ErasedHandle::Strong(strong_handle)
                } else {
                    // Asset meta exists, but all live handles were dropped. This means the
                    // `track_assets` system hasn't been run yet to remove the current asset
                    // (note that this is guaranteed to be transactional with the `track_assets`
                    // system because it locks the AssetInfos collection)

                    let provider = self
                        .handle_providers
                        .get(type_id)
                        .ok_or(MissingHandleProvider(type_id))?;

                    // We created a new strong handle, need to skip one drop info request.
                    info.handle_drops_to_skip += 1;

                    // We must create a new strong handle for the existing id and ensure that the drop of the old
                    // strong handle doesn't remove the asset from the Assets collection
                    let handle = provider.build_handle(index, Some(path), true);
                    info.weak_handle = Arc::downgrade(&handle);
                    ErasedHandle::Strong(handle)
                };

                let should_load = match loading_mode {
                    HandleLoadingMode::Force => true,
                    HandleLoadingMode::Request => {
                        matches!(info.load_state, LoadState::NotLoaded | LoadState::Failed(_))
                    }
                    _ => false,
                };

                if should_load {
                    info.load_state = LoadState::Loading;
                    info.dep_load_state = DependencyLoadState::Loading;
                    info.rec_dep_load_state = RecursiveDependencyLoadState::Loading;
                }

                Ok((handle, should_load))
            }
            TypeMapEntry::Vacant(entry) => {
                let should_load = match loading_mode {
                    HandleLoadingMode::NotLoading => false,
                    HandleLoadingMode::Request | HandleLoadingMode::Force => true,
                };
                let handle = Self::alloc_internal(
                    &mut self.infos,
                    &self.handle_providers,
                    &mut self.living_labeled_assets,
                    self.watching_for_changes,
                    should_load,
                    type_id,
                    Some(path),
                )?;
                let index = match &handle {
                    ErasedHandle::Strong(handle) => handle.index,
                    // `alloc_internal` always returns the Strong variant.
                    ErasedHandle::Uuid { .. } => unreachable!(),
                };
                entry.insert(index);
                Ok((handle, should_load))
            }
        }
    }
}

// -----------------------------------------------------------------------------
// alloc handle

impl AssetInfos {
    /// Allocates a path-less handle for an asset of type `type_id`, with all three load states set
    /// to `Loading`.
    ///
    /// This is how an asset that is added directly — already loaded, so nothing has to be read —
    /// gets its handle: the states start at `Loading`, and the `Loaded` event that follows settles
    /// them.
    ///
    /// # Panics
    ///
    /// Panics when `type_id` has no handle provider, i.e. the asset type was never initialized;
    /// see [`unwrap_with_context`], which also documents the use of `debug_name`.
    pub fn alloc_loading_handle_erased(
        &mut self,
        type_id: TypeId,
        debug_name: Option<&'static str>,
    ) -> ErasedHandle {
        let result = Self::alloc_internal(
            &mut self.infos,
            &self.handle_providers,
            &mut self.living_labeled_assets,
            self.watching_for_changes,
            true,
            type_id,
            None,
        );

        unwrap_with_context(result, type_id, debug_name)
    }

    /// Returns the handle registered for `(path, type_id)`, allocating an entry for it when there
    /// is none yet.
    ///
    /// A new entry is recorded in `path_to_index`; an existing one whose strong handles have all
    /// been dropped gets a new strong handle for the same index, and the drop event that is still
    /// pending is skipped so that the asset stays registered.
    ///
    /// The returned `bool` answers whether the caller has to start a load for the handle — which is
    /// exactly when the three load states were just set to `Loading` by `mode`
    /// ([`HandleLoadingMode::Force`], or [`HandleLoadingMode::Request`] on an asset that has not
    /// loaded yet). On `false` there is nothing to start: the asset is already loading or already
    /// settled, or `mode` was [`HandleLoadingMode::NotLoading`].
    ///
    /// # Panics
    ///
    /// Panics when `type_id` has no handle provider, i.e. the asset type was never initialized;
    /// see [`unwrap_with_context`], which also documents the use of `debug_name`.
    pub fn get_or_alloc_handle_erased(
        &mut self,
        path: AssetPath<'static>,
        mode: HandleLoadingMode,
        type_id: TypeId,
        debug_name: Option<&'static str>,
    ) -> (ErasedHandle, bool) {
        let result = self.get_or_alloc_internal(path, type_id, mode);

        // it is ok to unwrap because TypeId was specified above
        unwrap_with_context(result, type_id, debug_name)
    }

    /// Typed counterpart of [`get_or_alloc_handle_erased`](Self::get_or_alloc_handle_erased) that
    /// returns the handle alone.
    ///
    /// The "a load has to be started" answer is dropped here, so a caller that passes
    /// [`HandleLoadingMode::Request`] or [`HandleLoadingMode::Force`] has to start that load
    /// itself; every current caller only looks a handle up without loading it, i.e. uses
    /// [`HandleLoadingMode::NotLoading`].
    ///
    /// # Panics
    ///
    /// Panics when `A` was never initialized with `init_asset`, because there is no handle provider
    /// to allocate from. The message names that missing call.
    pub fn get_or_alloc_handle<A: Asset>(
        &mut self,
        path: AssetPath<'static>,
        mode: HandleLoadingMode,
    ) -> Handle<A> /*, bool */ {
        let type_id = TypeId::of::<A>();
        let debug_name = ::core::any::type_name::<A>();
        let result = self.get_or_alloc_internal(path, type_id, mode);

        // it is ok to unwrap because TypeId was specified above
        unwrap_with_context(result, type_id, Some(debug_name))
            .0
            .with_type_debug_checked()
    }

    /// Returns the handle of the sub-asset registered at `path`, when that path resolves to exactly
    /// one asset type.
    ///
    /// A labeled path (`base#label`) is an entry of its own: the loader that produced the sub-asset
    /// registered it under this very path, so its handle is already there and the base asset does
    /// not have to be read again. Returns [`None`] when nothing is registered for `path`, or when
    /// several asset types share it — the caller then has to name the type it wants.
    pub fn try_get_sub_asset_handle(
        &mut self,
        path: AssetPath<'static>,
        mode: HandleLoadingMode,
    ) -> Option<(ErasedHandle, bool)> {
        let handles = self.path_to_index.get(&path)?;

        if handles.len() != 1 {
            return None;
        }

        let type_id = handles.types().next()?;

        Some(self.get_or_alloc_handle_erased(path, mode, type_id, None))
    }
}

// -----------------------------------------------------------------------------
// Lookups

impl AssetInfos {
    pub fn contains_key(&self, index: TypedAssetIndex) -> bool {
        self.infos.contains_key(&index)
    }

    pub fn get(&self, index: TypedAssetIndex) -> Option<&AssetInfo> {
        self.infos.get(&index)
    }

    pub fn get_mut(&mut self, index: TypedAssetIndex) -> Option<&mut AssetInfo> {
        self.infos.get_mut(&index)
    }

    pub fn iter_indices_by_path<'a>(
        &'a self,
        path: &'a AssetPath<'_>,
    ) -> impl ExactSizeIterator<Item = TypedAssetIndex> + 'a {
        /// Concrete type to allow returning an `impl Iterator` even if
        /// `self.path_to_index.get(path)` is [`None`]
        enum TypedAssetIndexIter<T> {
            None,
            Some(T),
        }

        impl<T> Iterator for TypedAssetIndexIter<T>
        where
            T: Iterator<Item = TypedAssetIndex>,
        {
            type Item = TypedAssetIndex;

            #[inline]
            fn next(&mut self) -> Option<Self::Item> {
                match self {
                    TypedAssetIndexIter::None => None,
                    TypedAssetIndexIter::Some(iter) => iter.next(),
                }
            }

            #[inline]
            fn size_hint(&self) -> (usize, Option<usize>) {
                match self {
                    TypedAssetIndexIter::None => (0, Some(0)),
                    TypedAssetIndexIter::Some(iter) => iter.size_hint(),
                }
            }
        }

        impl<T> ExactSizeIterator for TypedAssetIndexIter<T>
        where
            T: ExactSizeIterator<Item = TypedAssetIndex>,
        {
            #[inline]
            fn len(&self) -> usize {
                match self {
                    TypedAssetIndexIter::None => 0,
                    TypedAssetIndexIter::Some(iter) => iter.len(),
                }
            }
        }

        if let Some(mapper) = self.path_to_index.get(path) {
            let iter = mapper
                .iter()
                .map(|(type_id, index)| TypedAssetIndex::new(*index, type_id));
            TypedAssetIndexIter::Some(iter)
        } else {
            TypedAssetIndexIter::None
        }
    }

    pub fn iter_handles_by_path<'a>(
        &'a self,
        path: &'a AssetPath<'_>,
    ) -> impl Iterator<Item = ErasedHandle> + 'a {
        self.iter_indices_by_path(path)
            .filter_map(|id| self.get_handle_by_index(id))
    }

    pub fn get_handles_by_path<'a>(&'a self, path: &'a AssetPath<'_>) -> Vec<ErasedHandle> {
        if let Some(mapper) = self.path_to_index.get(path) {
            let mut buffer = Vec::with_capacity(mapper.len());
            for (type_id, index) in mapper.iter() {
                let index = TypedAssetIndex::new(*index, type_id);
                if let Some(handle) = self.get_handle_by_index(index) {
                    buffer.push(handle);
                }
            }
            return buffer;
        }
        Vec::new()
    }

    pub fn get_indices_by_path<'a>(&'a self, path: &'a AssetPath<'_>) -> Vec<ErasedAssetId> {
        if let Some(mapper) = self.path_to_index.get(path) {
            let mut buffer: Vec<ErasedAssetId> = Vec::with_capacity(mapper.len());
            for (type_id, index) in mapper.iter() {
                let tyindex = TypedAssetIndex::new(*index, type_id);
                if self.get_handle_by_index(tyindex).is_some() {
                    let index = *index;
                    buffer.push(ErasedAssetId::Index { type_id, index });
                }
            }
            return buffer;
        }
        Vec::new()
    }

    pub fn contains_by_path<'a>(&'a self, path: &'a AssetPath<'_>) -> bool {
        // The same predicate as `iter_indices_by_path(path).any(contains_by_index)`, written out
        // as a loop to match the look-ups above.
        if let Some(mapper) = self.path_to_index.get(path) {
            for (type_id, index) in mapper.iter() {
                let index = TypedAssetIndex::new(*index, type_id);
                if self.contains_by_index(index) {
                    return true;
                }
            }
        }
        false
    }

    pub fn get_handle_by_index(&self, index: TypedAssetIndex) -> Option<ErasedHandle> {
        let info = self.infos.get(&index)?;
        let strong_handle = info.weak_handle.upgrade()?;
        Some(ErasedHandle::Strong(strong_handle))
    }

    pub fn contains_by_index(&self, index: TypedAssetIndex) -> bool {
        if let Some(info) = self.infos.get(&index) {
            info.weak_handle.strong_count() != 0
        } else {
            false
        }
    }

    pub fn get_handle_by_path_and_type_id(
        &self,
        path: &AssetPath<'_>,
        type_id: TypeId,
    ) -> Option<ErasedHandle> {
        let index = *self.path_to_index.get(path)?.get(type_id)?;
        self.get_handle_by_index(TypedAssetIndex::new(index, type_id))
    }

    /// Returns `true` if some live handle is registered under `path`.
    ///
    /// The path is looked up as it is given, so a labeled path only answers for the sub-asset
    /// registered under that very label; see [`Self::should_reload`] for the label-aware question.
    pub fn is_path_alive<'a>(&self, path: impl Into<AssetPath<'a>>) -> bool {
        let path = path.into();
        self.contains_by_path(&path)
    }

    /// Returns `true` if `path` still has something the server would reload.
    ///
    /// Two checks, in order: a live handle registered under exactly `path`, then a label still
    /// alive under the *label-stripped* base path — the key [`Self::alloc_internal`] records in
    /// `living_labeled_assets`, and only while the server watches for changes. Hot reload calls
    /// this with the base path of the changed file, so both checks apply to it; for a labeled path
    /// only the first one can answer.
    pub fn should_reload(&self, path: &AssetPath) -> bool {
        if self.is_path_alive(path) {
            return true;
        }

        if let Some(living) = self.living_labeled_assets.get(path) {
            !living.is_empty()
        } else {
            false
        }
    }
}

// -----------------------------------------------------------------------------
// handle drop

impl AssetInfos {
    fn remove_dependents_and_labels(
        info: &AssetInfo,
        loader_dependents: &mut HashMap<AssetPath<'static>, HashSet<AssetPath<'static>>>,
        path: &AssetPath<'static>,
        living_labeled_assets: &mut HashMap<AssetPath<'static>, HashSet<Arc<str>>>,
    ) {
        use zlim_utils::hash::map::Entry;

        for loader_dependency in info.loader_dependencies.keys() {
            if let Some(dependents) = loader_dependents.get_mut(loader_dependency) {
                dependents.remove(path);
            }
        }

        let Some(label) = path.label() else {
            return;
        };

        let mut without_label = path.clone();
        without_label.remove_label();

        let Entry::Occupied(mut entry) = living_labeled_assets.entry(without_label) else {
            return;
        };

        entry.get_mut().remove(label);

        if entry.get().is_empty() {
            entry.remove();
        }
    }

    fn process_handle_drop_internal(
        infos: &mut HashMap<TypedAssetIndex, AssetInfo>,
        path_to_index: &mut HashMap<AssetPath<'static>, TypeMap<AssetIndex>>,
        loader_dependents: &mut HashMap<AssetPath<'static>, HashSet<AssetPath<'static>>>,
        living_labeled_assets: &mut HashMap<AssetPath<'static>, HashSet<Arc<str>>>,
        pending_tasks: &mut HashMap<TypedAssetIndex, Task<()>>,
        watching_for_changes: bool,
        index: TypedAssetIndex,
    ) -> bool {
        use zlim_utils::hash::map::Entry;

        let Entry::Occupied(mut entry) = infos.entry(index) else {
            // Either the asset was already dropped, it doesn't exist, or it isn't managed by the asset server
            // None of these cases should result in a removal from the Assets collection
            return false;
        };

        if entry.get_mut().handle_drops_to_skip > 0 {
            entry.get_mut().handle_drops_to_skip -= 1;
            return false;
        }

        pending_tasks.remove(&index);

        let type_id = index.type_id;
        let info = entry.remove();

        let Some(path) = &info.path else {
            return true;
        };

        if watching_for_changes {
            Self::remove_dependents_and_labels(
                &info,
                loader_dependents,
                path,
                living_labeled_assets,
            );
        }

        if let Some(map) = path_to_index.get_mut(path) {
            map.remove(type_id);

            if map.is_empty() {
                path_to_index.remove(path);
            }
        };

        true
    }

    /// Returns `true` if the asset should be removed from the collection.
    #[inline]
    pub fn process_handle_drop(&mut self, index: AssetIndex, type_id: TypeId) -> bool {
        Self::process_handle_drop_internal(
            &mut self.infos,
            &mut self.path_to_index,
            &mut self.loader_dependents,
            &mut self.living_labeled_assets,
            &mut self.pending_tasks,
            self.watching_for_changes,
            TypedAssetIndex::new(index, type_id),
        )
    }

    /// Drains the handle-drop queue of *every* registered provider.
    ///
    /// The typed job (assets.rs's HandleAssetDropEvents<A>) is what removes a dropped asset
    /// from Assets<A>, and it pops from these very queues: draining them here instead would take
    /// the event away from that job and leave the value in Assets<A> forever. So this is only
    /// usable where no typed job runs — the standalone importer, which owns a server of its own.
    pub fn process_handle_drop_events(&mut self) {
        for provider in self.handle_providers.values() {
            while let Some(drop_event) = provider.try_recv() {
                let index = TypedAssetIndex::new(drop_event.index, drop_event.type_id);
                if drop_event.asset_server_managed {
                    Self::process_handle_drop_internal(
                        &mut self.infos,
                        &mut self.path_to_index,
                        &mut self.loader_dependents,
                        &mut self.living_labeled_assets,
                        &mut self.pending_tasks,
                        self.watching_for_changes,
                        index,
                    );
                }
            }
        }
    }
}

// -----------------------------------------------------------------------------
// propagate

/// Wakes every task that waits for this asset's load state to settle.
///
/// [`AssetServer::wait_for_asset_id`] only resolves
/// once *both* `load_state` and `rec_dep_load_state` are terminal, and a task whose waker is
/// registered here is parked until then. Successful loads report through the `FullyLoaded` event,
/// so every place that makes the *recursive* state terminal with a failure has to call this:
/// otherwise a failure deeper in the tree would leave those tasks parked forever.
///
/// [`AssetServer::wait_for_asset_id`]: crate::server::AssetServer::wait_for_asset_id
#[inline]
fn wake_waiting_tasks(info: &mut AssetInfo) {
    for waker in core::mem::take(&mut info.waiting_tasks) {
        waker.wake();
    }
}

impl AssetInfos {
    /// Recursively propagates loaded state up the dependency tree.
    fn propagate_loaded_state(
        &mut self,
        loaded_id: TypedAssetIndex,
        waiting_id: TypedAssetIndex,
        sender: &SegQueue<AssetServerEvent>,
    ) {
        let Some(info) = self.infos.get_mut(&waiting_id) else {
            return;
        };

        info.loading_rec_dependencies.remove(&loaded_id);

        if info.loading_rec_dependencies.is_empty() && info.failed_rec_dependencies.is_empty() {
            info.rec_dep_load_state = RecursiveDependencyLoadState::Loaded;
            info.loading_rec_dependencies = HashSet::new(); // dealloc memory

            if info.load_state.is_loaded() {
                let event = AssetServerEvent::FullyLoaded { index: waiting_id };
                sender.push(event);
            }

            for dep_id in core::mem::take(&mut info.dependents_waiting_on_recursive_dep_load) {
                self.propagate_loaded_state(waiting_id, dep_id, sender);
            }
        }
    }

    /// Recursively propagates failed state up the dependency tree
    fn propagate_failed_state(
        &mut self,
        failed_id: TypedAssetIndex,
        waiting_id: TypedAssetIndex,
        error: &AssetLoadError,
    ) {
        if let Some(info) = self.infos.get_mut(&waiting_id) {
            info.loading_rec_dependencies.remove(&failed_id);
            if info.loading_rec_dependencies.is_empty() {
                info.loading_rec_dependencies = HashSet::new(); // dealloc memory
            }

            info.failed_rec_dependencies.insert(failed_id);
            info.rec_dep_load_state = RecursiveDependencyLoadState::Failed(error.clone());

            // The waiters of this asset only have to be woken once, but the recursive state may
            // settle as `Failed` exactly once, so this is that moment.
            wake_waiting_tasks(info);

            for dep_id in core::mem::take(&mut info.dependents_waiting_on_recursive_dep_load) {
                self.propagate_failed_state(waiting_id, dep_id, error);
            }
        };
    }
}

// -----------------------------------------------------------------------------
// process events

impl AssetInfos {
    /// Applies a [`Failed`](AssetServerEvent::Failed) event.
    ///
    /// The error is recorded on the asset itself, on the assets that were loading it, and — up the
    /// reverse index — on every asset whose recursive dependency state had this one outstanding.
    /// The tasks parked on any of them through `AssetServer::wait_for_asset_id` are woken too: a
    /// failed recursive state never reaches `Loaded`, so the `FullyLoaded` event that would
    /// normally release them never comes.
    ///
    /// Does nothing when the asset has already been removed.
    pub fn process_asset_fail(&mut self, failed_index: TypedAssetIndex, error: AssetLoadError) {
        let Some(info) = self.infos.get_mut(&failed_index) else {
            // already be removed
            return;
        };

        // The waiters of this asset are parked instead of being woken one by one, so that they only
        // run after the states of the assets that were loading it have been written.
        struct WakingGuard(Vec<Waker>);

        impl Drop for WakingGuard {
            fn drop(&mut self) {
                for waker in core::mem::take(&mut self.0) {
                    waker.wake();
                }
            }
        }

        let mut waiting_tasks = WakingGuard(Vec::new());

        let (waiting_on_load, waiting_on_rec_load) = {
            info.load_state = LoadState::Failed(Clone::clone(&error));
            info.dep_load_state = DependencyLoadState::Failed(Clone::clone(&error));
            info.rec_dep_load_state = RecursiveDependencyLoadState::Failed(Clone::clone(&error));

            waiting_tasks.0 = core::mem::take(&mut info.waiting_tasks);

            (
                core::mem::take(&mut info.dependents_waiting_on_load),
                core::mem::take(&mut info.dependents_waiting_on_recursive_dep_load),
            )
        };

        for waiting_id in waiting_on_load {
            if let Some(info) = self.infos.get_mut(&waiting_id) {
                info.loading_dependencies.remove(&failed_index);
                if info.loading_dependencies.is_empty() {
                    info.loading_dependencies = HashSet::new(); // dealloc memory
                }

                info.failed_dependencies.insert(failed_index);
                // don't overwrite DependencyLoadState if already failed to preserve first error
                if !info.dep_load_state.is_failed() {
                    info.dep_load_state = DependencyLoadState::Failed(Clone::clone(&error));
                }
            }
        }

        for waiting_id in waiting_on_rec_load {
            self.propagate_failed_state(failed_index, waiting_id, &error);
        }
    }

    /// Applies a [`Loaded`](AssetServerEvent::Loaded) event.
    ///
    /// Every labeled sub-asset is applied first, as a load of its own, then the value is written
    /// into the world and the asset's load states and dependency sets are recomputed from the
    /// dependency lists the load reported.
    ///
    /// The assets that were waiting on this one are released from it: a direct dependent whose
    /// dependencies are now all in gets [`DependencyLoadState::Loaded`], and a recursive one is
    /// walked up through [`Self::propagate_loaded_state`]. The asset itself reports
    /// `FullyLoaded` as soon as its recursive state is `Loaded`, which is what wakes the tasks
    /// parked on it through `AssetServer::wait_for_asset_id`; when that state is `Failed` instead,
    /// the event can never come, so those tasks are woken here.
    ///
    /// Does nothing when the handle was dropped while the asset was loading.
    pub fn process_asset_load(
        &mut self,
        loaded_index: TypedAssetIndex,
        loaded_asset: ErasedLoadedAsset,
        world: &mut World,
        sender: &SegQueue<AssetServerEvent>,
    ) {
        for asset in loaded_asset.labeled_assets {
            let ErasedHandle::Strong(handle) = &asset.handle else {
                unreachable!("Labeled assets are always strong handles");
            };
            let label_index = TypedAssetIndex {
                index: handle.index,
                type_id: handle.type_id,
            };
            self.process_asset_load(label_index, asset.asset, world, sender);
        }

        // Check whether the handle has been dropped since the asset was loaded.
        if !self.infos.contains_key(&loaded_index) {
            return;
        }

        loaded_asset.value.apply_asset(loaded_index.index, world);

        let mut loading_deps: HashSet<TypedAssetIndex> = loaded_asset.dependencies;
        let mut failed_deps: HashSet<TypedAssetIndex> = HashSet::new();
        let mut dep_error: Option<AssetLoadError> = None;

        let mut loading_rec_deps: HashSet<TypedAssetIndex> = loading_deps.clone();
        let mut failed_rec_deps: HashSet<TypedAssetIndex> = HashSet::new();
        let mut rec_dep_error: Option<AssetLoadError> = None;

        loading_deps.retain(|dep_id| {
            let Some(dep_info) = self.infos.get_mut(dep_id) else {
                zlim_log::warn!(
                    "Dependency {dep_id} from asset {loaded_index} is unknown. This asset's dependency \
                    load status will not switch to 'Loaded' until the unknown dependency is loaded.",
                );
                return true;
            };
            use RecursiveDependencyLoadState as RState;
            match &dep_info.rec_dep_load_state {
                RState::Loading | RState::NotLoaded => {
                    // If dependency is loading, wait for it.
                    dep_info.dependents_waiting_on_recursive_dep_load.insert(loaded_index);
                }
                RState::Loaded => {
                    // If dependency is loaded, reduce our count by one
                    loading_rec_deps.remove(dep_id);
                }
                RState::Failed(error) => {
                    if rec_dep_error.is_none() {
                        rec_dep_error = Some(error.clone());
                    }
                    failed_rec_deps.insert(*dep_id);
                    loading_rec_deps.remove(dep_id);
                }
            }
            match &dep_info.load_state {
                LoadState::NotLoaded | LoadState::Loading => {
                    // If dependency is loading, wait for it.
                    dep_info.dependents_waiting_on_load.insert(loaded_index);
                    true
                }
                LoadState::Loaded => {
                    // If dependency is loaded, reduce our count by one
                    false
                }
                LoadState::Failed(error) => {
                    if dep_error.is_none() {
                        dep_error = Some(error.clone());
                    }
                    failed_deps.insert(*dep_id);
                    false
                }
            }
        });

        if loading_deps.is_empty() {
            loading_deps = HashSet::new();
        }

        if loading_rec_deps.is_empty() {
            loading_rec_deps = HashSet::new();
        }

        let dep_load_state = match (loading_deps.len(), failed_deps.len()) {
            (0, 0) => DependencyLoadState::Loaded,
            (_loading, 0) => DependencyLoadState::Loading,
            (_loading, _failed) => DependencyLoadState::Failed(dep_error.unwrap()),
        };

        let rec_dep_load_state = match (loading_rec_deps.len(), failed_rec_deps.len()) {
            (0, 0) => {
                sender.push(AssetServerEvent::FullyLoaded {
                    index: loaded_index,
                });
                RecursiveDependencyLoadState::Loaded
            }
            (_loading, 0) => RecursiveDependencyLoadState::Loading,
            (_loading, _failed) => RecursiveDependencyLoadState::Failed(rec_dep_error.unwrap()),
        };

        let (waiting_on_load, waiting_on_rec_load) = {
            // Asset info should always exist at this point
            let info = self.infos.get_mut(&loaded_index).unwrap();

            // if watching for changes, track reverse loader dependencies for hot reloading
            if self.watching_for_changes {
                if let Some(asset_path) = &info.path {
                    for loader_dependency in loaded_asset.loader_dependencies.keys() {
                        self.loader_dependents
                            .entry(loader_dependency.clone())
                            .or_default()
                            .insert(asset_path.clone());
                    }
                }

                info.loader_dependencies = loaded_asset.loader_dependencies;
            }

            info.loading_dependencies = loading_deps;
            info.failed_dependencies = failed_deps;
            info.loading_rec_dependencies = loading_rec_deps;
            info.failed_rec_dependencies = failed_rec_deps;
            info.load_state = LoadState::Loaded;
            info.dep_load_state = dep_load_state;
            info.rec_dep_load_state = rec_dep_load_state.clone();

            if rec_dep_load_state.is_failed() {
                // Nothing else will ever advance this asset's recursive state, so its waiters
                // have to be released now (on success the `FullyLoaded` event does it).
                wake_waiting_tasks(info);
            }

            let recf = rec_dep_load_state.is_failed() || rec_dep_load_state.is_loaded();

            (
                core::mem::take(&mut info.dependents_waiting_on_load),
                recf.then(|| core::mem::take(&mut info.dependents_waiting_on_recursive_dep_load)),
            )
        };

        for id in waiting_on_load {
            if let Some(info) = self.infos.get_mut(&id) {
                info.loading_dependencies.remove(&loaded_index);
                if info.loading_dependencies.is_empty() && !info.dep_load_state.is_failed() {
                    // send dependencies loaded event
                    info.dep_load_state = DependencyLoadState::Loaded;
                    info.loading_dependencies = HashSet::new(); // dealloc memory
                }
            }
        }

        if let Some(waiting_on_rec_load) = waiting_on_rec_load {
            match &rec_dep_load_state {
                RecursiveDependencyLoadState::Loaded => {
                    for dep_id in waiting_on_rec_load {
                        Self::propagate_loaded_state(self, loaded_index, dep_id, sender);
                    }
                }
                RecursiveDependencyLoadState::Failed(error) => {
                    for dep_id in waiting_on_rec_load {
                        Self::propagate_failed_state(self, loaded_index, dep_id, error);
                    }
                }
                RecursiveDependencyLoadState::Loading | RecursiveDependencyLoadState::NotLoaded => {
                    unreachable!("Should not be `Loading` or `NotLoaded`, checked above.")
                }
            }
        }
    }
}
