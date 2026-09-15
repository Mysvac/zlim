//! The server internals: everything [`AssetServer`] does behind its public API.
//!
//! What lives here, in the order the sections of this file appear:
//!
//! - the shared state: [`AssetServerData`] (the sources, the loaders and savers, the handle
//!   providers) with its [`Stats`], and the lock and event plumbing every handle of a server goes
//!   through;
//! - construction and registration: building a server, initializing an asset type, and reaching the
//!   registries it was built with;
//! - resolving a path: which `.meta` applies to it, which loader reads it, and which reader hands out
//!   its bytes;
//! - the load path: the typed and type-erased entry points, their untyped and folder variants, the
//!   tasks a load is spawned into, and reloading;
//! - the two things that do not load: adding an asset by value, and saving one through a saver.
//!
//! [`AssetServer`]: crate::server::AssetServer

use core::any::Any;
use core::any::TypeId;
use core::panic::AssertUnwindSafe;
use core::sync::atomic::AtomicUsize;
use std::borrow::Cow;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use futures_lite::{AsyncWriteExt, FutureExt, StreamExt};
use zlim_core::world::World;
use zlim_task::IoTaskPool;
use zlim_utils::ext::CachePadded;
use zlim_utils::hash::HashSet;
use zlim_utils::str::SmolStr;
use zlim_utils::sync::SegQueue;

use super::builder::Guard;
use super::server::AssetServer;
use crate::asset::Asset;
use crate::assets::Assets;
use crate::error::*;
use crate::event::{AssetEvent, AssetLoadFailedEvent};
use crate::handle::{AssetHandleProvider, ErasedHandle, Handle};
use crate::ident::{AssetId, AssetIndex, AssetSourceId, ErasedAssetId, TypedAssetIndex};
use crate::io::{AssetReaderError, AssetWriterError, ErasedAssetReader, Reader};
use crate::loaded::{ErasedLoadedAsset, LoadedAsset, LoadedFolder, LoadedUntypedAsset};
use crate::loader::{AssetLoader, AssetLoaders, ErasedAssetLoader, LoadContext};
use crate::meta::{AssetActionMinimal, AssetConfigMinimal, ErasedAssetMeta, Settings};
use crate::path::AssetPath;
use crate::saver::{AssetSaver, AssetSavers, ErasedSavedAsset};
use crate::server::AssetServerEvent;
use crate::server::UNTYPED_SOURCE_SUFFIX;
use crate::server::config::*;
use crate::server::event::SaveCommand;
use crate::server::info::{AssetInfos, HandleLoadingMode};
use crate::source::{AssetSource, AssetSources};

// -----------------------------------------------------------------------------
// Stats

/// Tracks statistics of the asset server.
pub(crate) struct Stats {
    /// The number of load tasks that have been started.
    pub started_load_tasks: AtomicUsize,
}

// -----------------------------------------------------------------------------
// AssetServerData

/// The shared state behind every [`AssetServer`] handle.
pub(crate) struct AssetServerData {
    /// Which side of a source the server reads: the raw source, or the processed one.
    pub(crate) server_mode: AssetServerMode,
    /// When a `.meta` sidecar is consulted while loading.
    pub(crate) meta_mode: AssetMetaCheckMode,
    /// How a path that escapes its source root is treated.
    pub(crate) path_mode: UnapprovedPathMode,

    /// Whether the server tracks file dependencies for hot reloading.
    pub(crate) watching_for_changes: bool,

    /// The sources the server reads from and writes to, by id.
    pub(crate) sources: Arc<AssetSources>,

    /// The loaders registered with this server.
    pub(crate) loaders: Arc<RwLock<AssetLoaders>>,
    /// The savers registered with this server.
    pub(crate) savers: Arc<RwLock<AssetSavers>>,

    /// The per-asset tracking state: handles, load state and file dependencies.
    pub(crate) infos: CachePadded<RwLock<AssetInfos>>,
    /// The saves that were queued and not run yet.
    pub(crate) saves: SegQueue<SaveCommand>,
    /// The load results that are waiting to be applied to the world.
    pub(crate) queue: SegQueue<AssetServerEvent>,
    /// The load counters the diagnostic reads.
    pub(crate) stats: CachePadded<Stats>,
}

// -----------------------------------------------------------------------------
// Stats Methods

// Simple accessors; no doc comments needed.
impl AssetServerData {
    #[inline]
    pub(crate) fn add_started_load_tasks(&self, num: usize) {
        use core::sync::atomic::Ordering::Relaxed;
        self.stats.started_load_tasks.fetch_add(num, Relaxed);
    }

    #[inline]
    pub(crate) fn get_started_load_tasks(&self) -> usize {
        use core::sync::atomic::Ordering::Relaxed;
        self.stats.started_load_tasks.load(Relaxed)
    }
}

// -----------------------------------------------------------------------------
// lock & event

impl AssetServerData {
    #[inline]
    pub(crate) fn read_infos(&self) -> RwLockReadGuard<'_, AssetInfos> {
        self.infos.read().unwrap_or_else(PoisonError::into_inner)
    }

    #[inline]
    pub(crate) fn write_infos(&self) -> RwLockWriteGuard<'_, AssetInfos> {
        self.infos.write().unwrap_or_else(PoisonError::into_inner)
    }

    #[inline]
    pub(crate) fn read_loaders(&self) -> RwLockReadGuard<'_, AssetLoaders> {
        self.loaders.read().unwrap_or_else(PoisonError::into_inner)
    }

    #[inline]
    pub(crate) fn write_loaders(&self) -> RwLockWriteGuard<'_, AssetLoaders> {
        self.loaders.write().unwrap_or_else(PoisonError::into_inner)
    }

    #[inline]
    pub(crate) fn read_savers(&self) -> RwLockReadGuard<'_, AssetSavers> {
        self.savers.read().unwrap_or_else(PoisonError::into_inner)
    }

    #[inline]
    pub(crate) fn write_savers(&self) -> RwLockWriteGuard<'_, AssetSavers> {
        self.savers.write().unwrap_or_else(PoisonError::into_inner)
    }

    #[inline]
    pub(crate) fn reader_for<'a>(
        &self,
        source: &'a AssetSource,
    ) -> Result<&'a dyn ErasedAssetReader, MissingProcessedAssetReader> {
        match self.server_mode {
            AssetServerMode::Unprocessed => Ok(source.reader()),
            AssetServerMode::Processed => source.processed_reader(),
        }
    }

    #[inline]
    fn send_asset_event(&self, event: AssetServerEvent) {
        self.queue.push(event);
    }
}

// -----------------------------------------------------------------------------
// AssetServer: construction & registration

impl AssetServer {
    #[inline]
    pub(crate) fn new_impl(
        sources: Arc<AssetSources>,
        server_mode: AssetServerMode,
        meta_mode: AssetMetaCheckMode,
        path_mode: UnapprovedPathMode,
        watching_for_changes: bool,
    ) -> Self {
        Self::new_with_loaders(
            sources,
            Arc::new(RwLock::new(AssetLoaders::default())),
            server_mode,
            meta_mode,
            path_mode,
            watching_for_changes,
        )
    }

    /// Creates a server that shares the loader registry of another server.
    ///
    /// This is what the importer and the app are built with in
    /// [`AssetServerMode::Processed`]: a loader registered on either server has to be the same one
    /// the other one loads with, and a registry belongs to one server. Loaders are the only thing
    /// shared — the processors live on the importer, and the savers a processor uses are the ones
    /// its pipeline holds.
    pub(crate) fn new_with_loaders(
        sources: Arc<AssetSources>,
        loaders: Arc<RwLock<AssetLoaders>>,
        server_mode: AssetServerMode,
        meta_mode: AssetMetaCheckMode,
        path_mode: UnapprovedPathMode,
        watching_for_changes: bool,
    ) -> Self {
        let infos = AssetInfos {
            watching_for_changes,
            ..AssetInfos::default()
        };

        let stats = Stats {
            started_load_tasks: AtomicUsize::new(0),
        };

        Self(Arc::new(AssetServerData {
            server_mode,
            meta_mode,
            path_mode,
            watching_for_changes,
            sources,
            loaders,
            savers: Arc::new(RwLock::new(AssetSavers::default())),

            infos: CachePadded::new(RwLock::new(infos)),
            saves: SegQueue::new(),
            queue: SegQueue::new(),
            stats: CachePadded::new(stats),
        }))
    }

    /// Adopts the handle provider of `A` and registers the type-specific writers
    /// used to report this asset type's load result.
    pub(crate) fn register_asset_impl<A: Asset>(&self, assets: &Assets<A>) {
        // ---------------------------------------------------------------------
        // register provider

        fn getasset<A: Asset>(
            world: &World,
            id: ErasedAssetId,
        ) -> Option<&(dyn Any + Send + Sync)> {
            let id = id.try_with_type::<A>().ok()?;
            let value = world.get_resource::<Assets<A>>()?.get(id)?;
            Some(value as &(dyn Any + Send + Sync))
        }

        // ---------------------------------------------------------------------

        let provider = assets.handle_provider();
        debug_assert_eq!(provider.type_id(), TypeId::of::<A>());

        let mut infos = self.0.write_infos();

        infos.handle_providers.insert(TypeId::of::<A>(), provider);

        // Reading the value back out of the world is what lets a `Handle<A>` be saved.
        infos.asset_getters.insert(TypeId::of::<A>(), getasset::<A>);

        // ---------------------------------------------------------------------
        // register asset event

        fn loaded_sender<A: Asset>(world: &mut World, index: AssetIndex) {
            world.write_message(AssetEvent::<A>::FullyLoaded { id: index.into() });
        }

        fn failed_sender<A: Asset>(
            world: &mut World,
            index: AssetIndex,
            path: AssetPath<'static>,
            error: AssetLoadError,
        ) {
            let id: AssetId<A> = index.into();
            world.write_message(AssetLoadFailedEvent::<A> { id, path, error });
        }

        // ---------------------------------------------------------------------

        infos
            .dependency_loaded_event_sender
            .insert(TypeId::of::<A>(), loaded_sender::<A>);

        infos
            .dependency_failed_event_sender
            .insert(TypeId::of::<A>(), failed_sender::<A>);

        // ---------------------------------------------------------------------
    }

    /// Registers `provider` as the handle provider of the asset type it allocates for.
    ///
    /// A server hands out a handle for every asset type it loads, and the provider is where those
    /// indexes come from: without one, allocating a handle of that type panics. `Assets<A>` brings
    /// its own provider along when it is registered with
    /// [`register_asset`](AssetServer::register_asset), and a server that is *not* backed by a
    /// world's `Assets` — the importer's — gets one of its own, with an id space of its own.
    pub(crate) fn register_handle_provider(&self, provider: AssetHandleProvider) {
        self.0
            .write_infos()
            .handle_providers
            .insert(provider.type_id(), provider);
    }

    /// Pre-registers `L`, so that assets resolved to it wait instead of failing until it is
    /// registered for real.
    pub(crate) fn preregister_loader_impl<L: AssetLoader>(&self) {
        self.0.write_loaders().reserve::<L>();
    }

    /// Registers `loader` with this server.
    pub(crate) fn register_loader_impl<L: AssetLoader>(&self, loader: L) {
        self.0.write_loaders().push(loader);
    }

    /// Registers `saver` with this server.
    pub(crate) fn register_saver_impl<S: AssetSaver>(&self, saver: S) {
        self.0.write_savers().push(saver);
    }

    /// Returns `true` when the loader is registered and ready.
    pub(crate) fn contains_loader_impl<L: AssetLoader>(&self) -> bool {
        self.0.read_loaders().contains(L::type_path())
    }

    /// Returns `true` when the saver is registered and ready.
    pub(crate) fn contains_saver_impl<S: AssetSaver>(&self) -> bool {
        self.0.read_savers().contains(S::type_path())
    }
}

// -----------------------------------------------------------------------------
// meta + loader + reader

/// The meta, the loader and the reader an asset is loaded with.
type MetaLoaderReader<'a> = (
    Box<dyn ErasedAssetMeta>,
    Arc<dyn ErasedAssetLoader>,
    Box<dyn Reader + 'a>,
);

impl AssetServer {
    /// Resolves the meta, the loader and the reader for `asset_path`.
    ///
    /// The loader is chosen by the first of these that applies:
    ///
    /// - `loader_name`, the loader the caller forced (a load builder option). It is taken as a loader
    ///   name and looked up as one — a full type path and a short name both resolve — so the `.meta`'s
    ///   own name is not consulted at all. The `.meta` is still read, though: the forced loader
    ///   deserializes its settings as its own. An empty name is equivalent to passing [`None`] — the
    ///   `.meta` decides again — though there is normally no reason to pass one.
    /// - the `.meta`'s `Load { loader }` name, when none was forced.
    /// - `asset_type_id` and the path's extension, when there is no `.meta` (or it names none). The id
    ///   is the type of the handle the caller already holds, if any, and only narrows the look-up for
    ///   paths *without* a label.
    ///
    /// `asset_type_name` is the same type as [`core::any::type_name`] spells it, and only ever ends up
    /// in the error a missing loader produces — the look-up itself goes by the id.
    pub(crate) async fn get_meta_loader_and_reader<'a>(
        &'a self,
        loader_name: Option<&'a str>,
        asset_path: &'a AssetPath<'_>,
        asset_type_id: Option<TypeId>,
        asset_type_name: Option<&'static str>,
    ) -> Result<MetaLoaderReader<'a>, AssetLoadError> {
        let data = self.0.as_ref();

        let source_id = AssetSourceId::new(asset_path.source_raw());
        let source = data.sources.get(source_id).map_err(AssetError::from)?;

        let asset_reader = data.reader_for(source)?;

        let read_meta = data.meta_mode.should_check(asset_path);

        let default_logic = async {
            let error = || {
                ::core::hint::cold_path();
                AssetLoadError::from(MissingAssetLoader::from(
                    MissingBuilder::new()
                        .with_asset_path(asset_path)
                        .may_with_type_name(loader_name)
                        .may_with_asset_type(asset_type_name)
                        .may_with_asset_type_id(asset_type_id),
                ))
            };

            let entry = {
                data.read_loaders()
                    .find(None, loader_name, asset_type_id, Some(asset_path))
            };

            // No type name is passed, so no name can be ambiguous: `find` can only report a missing
            // loader here.
            let Ok(entry) = entry else {
                ::core::hint::cold_path();
                return Err(error());
            };

            let loader = match entry {
                Ok(loader) => loader,
                Err(pending) => {
                    ::core::hint::cold_path();
                    pending.get().await.ok_or_else(error)?
                }
            };

            let meta = loader.default_meta();

            let reader = asset_reader.read(asset_path.path()).await?;

            Ok((meta, loader, reader))
        };

        // ---------------------------------------------------------------------
        // No meta: pick a loader by asset type / extension and use the
        // loader's default meta (which is a `Load` config with default settings).
        // ---------------------------------------------------------------------

        if !read_meta {
            return default_logic.await;
        }

        // ---------------------------------------------------------------------
        // Meta: read the sidecar and let it pick the loader and the settings.
        // ---------------------------------------------------------------------

        let mut meta_reader = match asset_reader.read_meta(asset_path.path()).await {
            Ok(reader) => reader,
            Err(error) if error.is_not_found() => return default_logic.await,
            Err(error) => return Err(error.into()),
        };

        let mut meta_bytes = Vec::new();

        meta_reader
            .read_all_bytes(&mut meta_bytes)
            .await
            .map_err(|error| {
                ::core::hint::cold_path();
                let err = AssetReaderError::from(error);
                AssetError::AssetReaderError(err)
            })?;

        let mut loader_name: Cow<'a, str> = Cow::Borrowed(loader_name.unwrap_or_default());

        if loader_name.is_empty() {
            let minimal = AssetConfigMinimal::from_bytes(&meta_bytes).map_err(|error| {
                ::core::hint::cold_path();
                let path = asset_path.to_string().into_boxed_str();
                AssetMetaParseError { path, error }
            })?;

            loader_name = match minimal.asset_config {
                AssetActionMinimal::Load { loader } if loader.is_empty() => {
                    return default_logic.await;
                }
                AssetActionMinimal::Load { loader } => Cow::Owned(loader),
                AssetActionMinimal::Process { .. } => {
                    ::core::hint::cold_path();
                    return Err(CannotLoadProcessedAsset(asset_path.clone_owned()).into());
                }
                AssetActionMinimal::Ignore => {
                    ::core::hint::cold_path();
                    return Err(CannotLoadIgnoredAsset(asset_path.clone_owned()).into());
                }
                AssetActionMinimal::None => return default_logic.await,
            };
        }

        let error = || {
            ::core::hint::cold_path();
            AssetLoadError::from(MissingAssetLoader::from(
                MissingBuilder::new()
                    .with_type_name(loader_name.as_ref())
                    .with_asset_path(asset_path)
                    .may_with_asset_type(asset_type_name)
                    .may_with_asset_type_id(asset_type_id),
            ))
        };

        // A `.meta` names its loader by *type name*: the lenient form, which also accepts a
        // fully-qualified type path, so the string can be used without being classified first.
        // The registry lock is released inside the expression, before waiting for the loader.
        let entry = {
            data.read_loaders()
                .find(None, Some(&loader_name), asset_type_id, Some(asset_path))
        };

        let entry = match entry {
            Ok(entry) => entry,
            Err(None) => {
                ::core::hint::cold_path();
                return Err(error());
            }
            Err(Some(ambiguous)) => {
                ::core::hint::cold_path();
                return Err(AssetLoadError::from(AssetError::from(ambiguous)));
            }
        };

        let loader = match entry {
            Ok(loader) => loader,
            Err(pending) => {
                ::core::hint::cold_path();
                pending.get().await.ok_or_else(error)?
            }
        };

        let meta = loader.deserialize_meta(&meta_bytes).map_err(|e| {
            ::core::hint::cold_path();
            let path = asset_path.to_string().into_boxed_str();
            AssetMetaParseError { path, error: e }
        })?;

        let reader = asset_reader.read(asset_path.path()).await?;

        Ok((meta, loader, reader))
    }
}

// -----------------------------------------------------------------------------
// loader

impl AssetServer {
    /// Returns the loader that reads `asset_path`, chosen the way a load would choose it.
    ///
    /// A `.meta` that names a loader wins (it is authoritative for that asset); without one the path's
    /// extension decides. This is not the public API it looks like: the importer is the only caller,
    /// and it needs the loader of an asset it is about to copy to the processed side.
    ///
    /// # Errors
    ///
    /// Fails when no loader can read the asset, and when the pre-registered loader that can is never
    /// registered.
    pub(crate) async fn get_asset_loader_by_asset_path(
        &self,
        path: &AssetPath<'_>,
    ) -> Result<Arc<dyn ErasedAssetLoader>, MissingAssetLoader> {
        let error = || {
            ::core::hint::cold_path();
            MissingAssetLoader::from(MissingBuilder::new().with_asset_path(path))
        };

        let loader = { self.0.read_loaders().find(None, None, None, Some(path)) };

        // No type name is passed, so no name can be ambiguous: `find` can only report a missing
        // loader here.
        let Ok(entry) = loader else {
            ::core::hint::cold_path();
            return Err(error());
        };

        match entry {
            Ok(loader) => Ok(loader),
            Err(pending) => {
                ::core::hint::cold_path();
                pending.get().await.ok_or_else(error)
            }
        }
    }
}

// -----------------------------------------------------------------------------
// load

impl AssetServer {
    /// Loads the asset at `path` and sends the result to the event queue.
    ///
    /// `input_handle` is the handle the load was requested for, if any; when it is
    /// given, the load can be cancelled by dropping that handle.
    ///
    /// `loader_name` is the loader the caller forced, if any — an empty one is equivalent to [`None`],
    /// though there is normally no reason to pass one — and `asset_type_name` is the name of the type
    /// that will be loaded, when it is known. Both only feed the loader look-up and the
    /// `MissingAssetLoader` error a failed one builds; nothing else in the load uses them.
    /// `force` loads the asset again even when a load is already in flight or has finished.
    ///
    /// Returns [`None`] only when the caller passed its own handle for a path without a label: the
    /// load then drives that handle, and there is none to hand back.
    pub(crate) async fn load_internal<'a>(
        &self,
        loader_name: Option<&'a str>,
        input_handle: Option<ErasedHandle>,
        path: AssetPath<'a>,
        asset_type_name: Option<&'static str>,
        force: bool,
    ) -> Result<Option<ErasedHandle>, AssetLoadError> {
        let asset_type_id = input_handle.as_ref().map(ErasedHandle::type_id);

        let asset_path: AssetPath<'static> = path.into_owned();

        // ---------------------------------------------------------------------
        // meta + loader + reader
        // ---------------------------------------------------------------------

        let (meta, loader, mut reader) = match self
            .get_meta_loader_and_reader(loader_name, &asset_path, asset_type_id, asset_type_name)
            .await
        {
            Ok(ret) => ret,
            Err(load_error) => {
                if let Some(handle) = &input_handle {
                    self.0.send_asset_event(AssetServerEvent::Failed {
                        path: asset_path.clone(),
                        error: load_error.clone(),
                        // The input handle is always a strong handle.
                        index: handle_index(handle),
                    });
                }
                return Err(load_error);
            }
        };

        // ---------------------------------------------------------------------
        // handle & whether the asset has to be loaded at all
        // ---------------------------------------------------------------------

        let label = asset_path.label();
        let asset_id: Option<TypedAssetIndex>; // The asset ID of the asset we are trying to load.
        let fetched_handle: Option<ErasedHandle>; // The handle if one was looked up/created.
        let should_load: bool; // Whether we need to load the asset.

        if let Some(handle) = input_handle {
            // This must have been created with `get_or_alloc_internal` at some point,
            //  which only produces Strong variant handles, so this is safe.
            asset_id = Some(handle_index(&handle));
            // Intentionally drop the input handle here: the load is cancelled
            // when the caller drops their handle.
            fetched_handle = None;
            should_load = true;
        } else if label.is_none() {
            const M: HandleLoadingMode = HandleLoadingMode::Request;
            let path = asset_path.clone();
            let type_id = loader.asset_type_id();
            let debug_name = Some(loader.asset_type_path());

            let (handle, should) = self
                .0
                .write_infos()
                .get_or_alloc_handle_erased(path, M, type_id, debug_name);

            asset_id = Some(handle_index(&handle));
            fetched_handle = Some(handle);
            should_load = should;
        } else {
            // No input handle and a labeled path: the handle is the sub-asset's own, which the
            // loader registers while the base is read — the branch below hands it back directly
            // when the base needs no load, and resolves it from the load otherwise.
            asset_id = None;
            fetched_handle = None;
            should_load = true;
        }

        // ---------------------------------------------------------------------
        // the handle's type has to match the loader's asset type
        // ---------------------------------------------------------------------

        if label.is_none()
            && let Some(asset_id) = asset_id
            && asset_id.type_id != loader.asset_type_id()
        {
            core::hint::cold_path();
            let error: AssetLoadError = RequestedHandleTypeMismatch {
                path: asset_path.clone(),
                requested: asset_id.type_id,
                loader_name: loader.type_path(),
                actual_asset_name: loader.asset_type_path(),
            }
            .into();

            self.0.send_asset_event(AssetServerEvent::Failed {
                path: asset_path.clone(),
                error: error.clone(),
                index: asset_id,
            });

            return Err(error);
        }

        if !should_load && !force {
            // `should_load == false` only comes from the `label.is_none()`
            // branch above, which always sets `fetched_handle = Some(handle)`
            // , so the handle is always `Some` here.
            debug_assert!(fetched_handle.is_some());
            return Ok(fetched_handle);
        }

        // ---------------------------------------------------------------------
        // a labeled path loads the whole asset and picks the sub-asset out of it
        // ---------------------------------------------------------------------

        let mut _base_handle_guard: Option<ErasedHandle> = None;
        let base_asset_id: TypedAssetIndex;
        let base_path: AssetPath<'static>;

        if label.is_some() {
            let mut pure_path = asset_path.clone();
            pure_path.remove_label();

            // The base asset is only read when it has to be: a sub-asset that is already registered
            // (because its base was loaded before) is handed straight back below. An explicit
            // reload (`force`) always reads the base again.
            let mode = if force {
                HandleLoadingMode::Force
            } else {
                HandleLoadingMode::Request
            };
            let path = pure_path.clone();
            let type_id = loader.asset_type_id();
            let debug_name = Some(loader.asset_type_path());

            let (handle, base_should_load) = self
                .0
                .write_infos()
                .get_or_alloc_handle_erased(path, mode, type_id, debug_name);

            base_asset_id = handle_index(&handle);
            base_path = pure_path;
            // The base asset has to stay alive until the sub-asset is resolved.
            _base_handle_guard = Some(handle);

            // Nothing to load: the base is loading or already loaded, so the sub-asset the loader
            // registered under this path is the answer. (When the label slot is not registered —
            // an unknown label, or one whose handle was dropped — this falls through to a real
            // load, which reports the missing label properly.)
            if !base_should_load
                && !force
                && let Some((handle, _)) = self
                    .0
                    .write_infos()
                    .try_get_sub_asset_handle(asset_path.clone(), HandleLoadingMode::NotLoading)
            {
                return Ok(Some(handle));
            }
        } else {
            base_asset_id = asset_id.unwrap();
            base_path = asset_path.clone();
        }

        // ---------------------------------------------------------------------
        // load
        // ---------------------------------------------------------------------

        // The `expect` below holds as long as no loader name was forced: `loader.default_meta()` is
        // always a `Load` config (see `loader.rs`), and when the `.meta` names its own loader,
        // `AssetConfigMinimal::from_bytes` has already rejected every action but `Load`. A caller that
        // forces a loader name skips that inspection, so a `.meta` that is not `Load` would reach
        // here and panic — see `AssetServer::get_meta_loader_and_reader`.
        let settings = meta
            .loader_settings()
            .expect("the meta of a loaded asset is always a `Load` config");

        let loaded_asset = self
            .load_with_loader(&base_path, settings, &*loader, &mut *reader, true, false)
            .await;

        let loaded_asset = match loaded_asset {
            Ok(loaded_asset) => loaded_asset,
            Err(error) => {
                if let Some(asset_id) = asset_id {
                    self.0.send_asset_event(AssetServerEvent::Failed {
                        index: asset_id,
                        path: base_path,
                        error: error.clone(),
                    });
                }
                return Err(error);
            }
        };

        // ---------------------------------------------------------------------
        // resolve the labeled sub-asset, if one was requested
        // ---------------------------------------------------------------------

        let Some(label) = label else {
            // If `input_handle` was `None`, `fetched_handle` is always `Some` at this point; see
            // `fetched_handle = Some(handle)` above.
            self.0.send_asset_event(AssetServerEvent::Loaded {
                index: base_asset_id,
                loaded_asset,
            });
            return Ok(fetched_handle);
        };

        let labeled_handle = match loaded_asset.label_to_label_index.get(label) {
            Some(labeled_index) => loaded_asset.labeled_assets[*labeled_index].handle.clone(),
            None => {
                let mut all_labels: Vec<String> =
                    loaded_asset.iter_labels().map(str::to_owned).collect();
                all_labels.sort_unstable();

                let error: AssetLoadError = MissingLabeledAsset {
                    path: base_path.clone(),
                    label: label.to_owned(),
                    all_labels,
                }
                .into();

                if let Some(asset_id) = asset_id {
                    self.0.send_asset_event(AssetServerEvent::Failed {
                        index: asset_id,
                        path: base_path,
                        error: error.clone(),
                    });
                }

                return Err(error);
            }
        };

        if let Some(asset_id) = asset_id
            && asset_id.type_id != labeled_handle.type_id()
        {
            core::hint::cold_path();
            let error: AssetLoadError = RequestedHandleTypeMismatch {
                path: base_path.clone(),
                requested: asset_id.type_id,
                loader_name: loader.type_path(),
                actual_asset_name: loader.asset_type_path(),
            }
            .into();

            self.0.send_asset_event(AssetServerEvent::Failed {
                index: asset_id,
                path: base_path,
                error: error.clone(),
            });

            return Err(error);
        }

        self.0.send_asset_event(AssetServerEvent::Loaded {
            index: base_asset_id,
            loaded_asset,
        });

        Ok(Some(labeled_handle))
    }

    /// Runs `loader` over `reader`, tracking the dependencies and sub-assets it
    /// declares through `LoadContext`.
    ///
    /// `load_dependencies` is handed to the [`LoadContext`]: when it is set, the assets the loader
    /// requests are recorded as dependencies and are actually loaded, and when it is not (the asset
    /// processor's mode), only their handles are created. `populate_hashes` makes the context read
    /// each asset's content hash out of its `.meta`, which is what the processor needs; a plain
    /// load leaves the hashes zero.
    ///
    /// A loader that panics is caught here and reported as the `AssetLoaderPanic` error, and one
    /// that returns an error as the `AssetLoaderError` that wraps it.
    pub(crate) async fn load_with_loader<'a>(
        &'a self,
        asset_path: &'a AssetPath<'static>,
        settings: &dyn Settings,
        loader: &dyn ErasedAssetLoader,
        reader: &mut dyn Reader,
        load_dependencies: bool,
        populate_hashes: bool,
    ) -> Result<ErasedLoadedAsset, AssetLoadError> {
        let load_context =
            LoadContext::new(self, asset_path.clone(), load_dependencies, populate_hashes);

        let load = AssertUnwindSafe(loader.load(reader, load_context, settings));

        match FutureExt::catch_unwind(load).await {
            Err(_) => {
                ::core::hint::cold_path();
                Err(AssetLoaderPanic {
                    path: asset_path.clone(),
                    loader: loader.type_path(),
                }
                .into())
            }
            Ok(Err(error)) => {
                ::core::hint::cold_path();
                Err(AssetLoaderError {
                    path: asset_path.clone(),
                    loader: loader.type_path(),
                    error: error.to_string(),
                }
                .into())
            }
            Ok(Ok(loaded_asset)) => Ok(loaded_asset),
        }
    }
}

/// Returns the index of a server-managed strong handle.
#[inline]
fn handle_index(handle: &ErasedHandle) -> TypedAssetIndex {
    #[cold]
    #[inline(never)]
    fn unreachable() -> ! {
        unreachable!("server-managed handles are always strong handles")
    }

    match handle {
        ErasedHandle::Strong(handle) => TypedAssetIndex {
            index: handle.index,
            type_id: handle.type_id,
        },
        ErasedHandle::Uuid { .. } => unreachable(),
    }
}

/// Checks that `path` may be loaded at all, and returns the reason when it may not.
///
/// This is the one place the load entry points agree on: an empty path is always refused, and an
/// unapproved one (a path escaping its source root) is refused unless the server is in
/// [`UnapprovedPathMode::Allow`] or the caller asked for it with `override_unapproved`. The
/// override only bypasses [`UnapprovedPathMode::Deny`]; [`UnapprovedPathMode::Forbid`] is never
/// bypassed.
///
/// Callers log the error and hand back a default handle: a refused load has no handle to work
/// with, and must not be recorded as a dependency.
pub(crate) fn validate_asset_path(
    path: &AssetPath<'static>,
    override_unapproved: bool,
    path_mode: &UnapprovedPathMode,
) -> Result<(), AssetError> {
    if path.path().as_os_str().is_empty() {
        ::core::hint::cold_path();
        return Err(AssetError::from(EmptyPathError(path.clone_owned())));
    }

    if path.is_unapproved() {
        ::core::hint::cold_path();
        match (path_mode, override_unapproved) {
            // Explicitly allowed by the server, or by the caller.
            (UnapprovedPathMode::Allow, _) | (UnapprovedPathMode::Deny, true) => {}
            (UnapprovedPathMode::Deny, false) | (UnapprovedPathMode::Forbid, _) => {
                ::core::hint::cold_path();
                return Err(AssetError::from(UnapprovedPath(path.clone_owned())));
            }
        }
    }

    Ok(())
}

// -----------------------------------------------------------------------------
// spawn

impl AssetServer {
    /// Spawns the task that loads the asset behind `handle`.
    ///
    /// `guard` is held until the load is over — successfully or not — and dropped then.
    pub(crate) fn spawn_load_task(
        &self,
        loader: Option<Cow<'static, str>>,
        handle: ErasedHandle,
        path: AssetPath<'static>,
        debug_name: Option<&'static str>,
        guard: Option<Guard>,
    ) {
        self.0.add_started_load_tasks(1);

        let input = Some(handle.clone());
        let server = self.clone();

        let task = IoTaskPool::get().spawn(async move {
            if let Err(error) = server
                .load_internal(loader.as_deref(), input, path, debug_name, false)
                .await
            {
                ::core::hint::cold_path();
                zlim_log::error!("{error}");
            }
            ::core::mem::drop(guard);
        });

        let index = handle_index(&handle);

        self.0.write_infos().pending_tasks.insert(index, task);
    }
}

// -----------------------------------------------------------------------------
// load entry points

impl AssetServer {
    /// Requests the asset at `path` and returns its handle without waiting for it.
    ///
    /// `loader` is the name the caller forced, if any; an empty string is equivalent to [`None`] — the
    /// `.meta` decides again — though the entry points that take one normally have no reason to pass
    /// it.
    ///
    /// A path that cannot be loaded ([`validate_asset_path`]) is refused with a default handle and
    /// nothing is registered for it.
    pub(crate) fn load_erased_asset_impl(
        &self,
        loader: Option<Cow<'static, str>>,
        path: AssetPath<'static>,
        type_id: TypeId,
        debug_name: Option<&'static str>,
        override_unapproved: bool,
        guard: Option<Guard>,
    ) -> ErasedHandle {
        // Log whatever we get, and then return a default handle.
        if let Err(error) = validate_asset_path(&path, override_unapproved, &self.0.path_mode) {
            ::core::hint::cold_path();
            zlim_log::error!("Failed to load Asset `{path}` : {error}");
            return ErasedHandle::default_for_type(type_id);
        }

        let (handle, should_load) = {
            self.0.write_infos().get_or_alloc_handle_erased(
                path.clone(),
                HandleLoadingMode::Request,
                type_id,
                debug_name,
            )
        };

        if should_load {
            self.spawn_load_task(loader, handle.clone(), path, debug_name, guard);
        }

        handle
    }

    /// Type-checked counterpart of [`load_erased_asset_impl`](Self::load_erased_asset_impl).
    #[inline]
    pub(crate) fn load_typed_asset_impl<A: Asset>(
        &self,
        loader: Option<Cow<'static, str>>,
        path: AssetPath<'static>,
        override_unapproved: bool,
        guard: Option<Guard>,
    ) -> Handle<A> {
        self.load_erased_asset_impl(
            loader,
            path,
            TypeId::of::<A>(),
            Some(core::any::type_name::<A>()),
            override_unapproved,
            guard,
        )
        .with_type_debug_checked()
    }
}

// -----------------------------------------------------------------------------
// untyped

/// Returns `path` with the synthetic source used for a type-erased load.
fn untyped_path(path: &AssetPath<'static>) -> AssetPath<'static> {
    let source = match path.source() {
        None => SmolStr::new(UNTYPED_SOURCE_SUFFIX),
        Some(source) => {
            let suffixed = format!("{source}{UNTYPED_SOURCE_SUFFIX}");
            SmolStr::from_str(&suffixed)
        }
    };

    path.clone().with_source(source)
}

impl AssetServer {
    /// Loads the asset at `path` without knowing its type.
    ///
    /// The returned handle refers to a [`LoadedUntypedAsset`] that carries the handle of the
    /// asset that was actually loaded; the concrete asset is registered under `path` as usual.
    /// Use [`wait_for_asset_erased`](Self::wait_for_asset_erased) (or the handle's
    /// [`LoadState`]) to know when it is available.
    ///
    /// A path that cannot be loaded ([`validate_asset_path`]) is refused with a default handle and
    /// nothing is registered for it.
    ///
    /// [`LoadState`]: crate::server::LoadState
    pub(crate) fn load_untyped_asset_impl(
        &self,
        loader: Option<Cow<'static, str>>,
        path: AssetPath<'static>,
        override_unapproved: bool,
        guard: Option<Guard>,
    ) -> Handle<LoadedUntypedAsset> {
        // Log whatever we get, and then return a default handle.
        if let Err(error) = validate_asset_path(&path, override_unapproved, &self.0.path_mode) {
            ::core::hint::cold_path();
            zlim_log::error!("Failed to load Asset `{path}` : {error}");
            return Handle::default();
        }

        let (handle, should_load) = {
            self.0.write_infos().get_or_alloc_handle_erased(
                untyped_path(&path),
                HandleLoadingMode::Request,
                TypeId::of::<LoadedUntypedAsset>(),
                Some(core::any::type_name::<LoadedUntypedAsset>()),
            )
        };

        let handle: Handle<LoadedUntypedAsset> = handle.with_type_debug_checked();

        if !should_load {
            return handle;
        }

        let index = handle_index(&handle.erased());
        self.0.add_started_load_tasks(1);

        let server = self.clone();
        let failed_path = path.clone();

        let task = IoTaskPool::get().spawn(async move {
            // The concrete asset is loaded through the original path (the suffix above only
            // keeps the wrapper's registration separate).
            match server
                .load_internal(loader.as_deref(), None, path, None, false)
                .await
            {
                Ok(Some(handle)) => {
                    let untyped = LoadedUntypedAsset { handle };
                    let loaded_asset = LoadedAsset::new(untyped).erased();
                    // This event index is the index of the untyped asset slot.
                    server.0.send_asset_event(AssetServerEvent::Loaded {
                        index,
                        loaded_asset,
                    });
                }
                Ok(None) => {
                    ::core::hint::cold_path();
                    unreachable!("a handle is returned when no input handle was passed")
                }
                Err(error) => {
                    ::core::hint::cold_path();
                    zlim_log::error!("{error}");
                    // This event index is the index of the untyped asset slot.
                    server.0.send_asset_event(AssetServerEvent::Failed {
                        index,
                        path: failed_path,
                        error,
                    });
                }
            }
            drop(guard);
        });

        self.0.write_infos().pending_tasks.insert(index, task);

        handle
    }
}

// -----------------------------------------------------------------------------
// folders

impl AssetServer {
    /// Loads every asset below `path` into a [`LoadedFolder`].
    pub(crate) fn load_folder_impl(&self, path: AssetPath<'static>) -> Handle<LoadedFolder> {
        let (handle, should_load) = {
            self.0.write_infos().get_or_alloc_handle_erased(
                path.clone(),
                HandleLoadingMode::Request,
                TypeId::of::<LoadedFolder>(),
                Some(core::any::type_name::<LoadedFolder>()),
            )
        };

        let handle: Handle<LoadedFolder> = handle.with_type_debug_checked();

        if !should_load {
            return handle;
        }

        // The start count is added inside `load_folder_internal`.
        let index = handle_index(&handle.erased());
        self.load_folder_internal(index, path);

        handle
    }

    /// Walks `path` and registers every file below it as a type-erased asset.
    pub(crate) fn load_folder_internal(&self, index: TypedAssetIndex, path: AssetPath<'static>) {
        /// Recursively collects the handles of every asset below `path`.
        async fn load_folder(
            server: &AssetServer,
            source: AssetSourceId,
            path: &Path,
            reader: &dyn ErasedAssetReader,
            handles: &mut Vec<ErasedHandle>,
        ) -> Result<(), AssetLoadError> {
            if !reader.is_directory(path).await.map_err(AssetError::from)? {
                return Ok(());
            }

            let mut path_stream = reader
                .read_directory(path)
                .await
                .map_err(AssetError::from)?;

            while let Some(child_path) = path_stream.next().await {
                if reader
                    .is_directory(&child_path)
                    .await
                    .map_err(AssetError::from)?
                {
                    Box::pin(load_folder(
                        server,
                        source.clone(),
                        &child_path,
                        reader,
                        handles,
                    ))
                    .await?;
                    continue;
                }

                let path = child_path
                    .to_str()
                    .expect("a path reported by an asset reader is valid UTF-8");
                let asset_path = AssetPath::parse(path)
                    .with_source_id(source.clone())
                    .into_owned();

                let handle = server.load_untyped_asset_impl(None, asset_path, false, None);

                match server.wait_for_asset_erased(&handle.erased()).await {
                    Ok(()) => handles.push(handle.erased()),
                    // A file without a loader for its extension is simply not part of the folder.
                    Err(WaitForAssetError::Failed(error)) => match &*error {
                        AssetError::MissingAssetLoader { .. } => {}
                        _ => {
                            return Err(AssetError::WaitForAssetError(WaitForAssetError::Failed(
                                error,
                            ))
                            .into());
                        }
                    },
                    Err(e) => return Err(AssetError::WaitForAssetError(e).into()),
                }
            }

            Ok(())
        }

        self.0.add_started_load_tasks(1);

        let server = self.clone();
        IoTaskPool::get()
            .spawn(async move {
                let source = match server.0.sources.get(path.source_id()) {
                    Ok(source) => source,
                    Err(error) => {
                        zlim_log::error!("Failed to load the folder '{path}': {error}");
                        server.0.send_asset_event(AssetServerEvent::Failed {
                            index,
                            path,
                            error: AssetError::from(error).into(),
                        });
                        return;
                    }
                };

                let asset_reader = match server.0.reader_for(source) {
                    Ok(reader) => reader,
                    Err(error) => {
                        ::core::hint::cold_path();
                        zlim_log::error!("Failed to load the folder '{path}': {error}");
                        server.0.send_asset_event(AssetServerEvent::Failed {
                            index,
                            path,
                            error: AssetError::from(error).into(),
                        });
                        return;
                    }
                };

                let mut handles = Vec::new();
                let source_id = source.id();

                match load_folder(&server, source_id, path.path(), asset_reader, &mut handles).await
                {
                    Ok(()) => {
                        let loaded_asset = LoadedAsset::new(LoadedFolder { handles }).erased();
                        server.0.send_asset_event(AssetServerEvent::Loaded {
                            index,
                            loaded_asset,
                        });
                    }
                    Err(error) => {
                        ::core::hint::cold_path();
                        zlim_log::error!("Failed to load the folder '{path}': {error}");
                        server
                            .0
                            .send_asset_event(AssetServerEvent::Failed { index, path, error });
                    }
                }
            })
            .detach();
    }
}

// -----------------------------------------------------------------------------
// reload

impl AssetServer {
    /// Reloads every asset registered under `path`.
    ///
    /// Each live handle for the path is loaded again with `force`, so assets that cancelled
    /// their own load (or were already loaded) are refreshed. When no handle is alive but a
    /// sub-asset still is, a type-erased load is attempted so that the sub-assets get reloaded
    /// with their base asset.
    ///
    /// `log` enables the `Reloaded {path}` info message, sent once at least one asset was reloaded.
    pub(crate) fn reload_internal(&self, path: AssetPath<'static>, log: bool) {
        self.0.add_started_load_tasks(1);

        let server = self.clone();
        IoTaskPool::get()
            .spawn(async move {
                let mut reloaded = false;

                // Collected up front: the read guard must not live across the awaits below.
                let handles = server.0.read_infos().get_handles_by_path(&path);

                for handle in handles {
                    // Count each reload as a started load.
                    server.0.add_started_load_tasks(1);

                    match server
                        .load_internal(None, Some(handle), path.clone(), None, true)
                        .await
                    {
                        Ok(_) => reloaded = true,
                        Err(error) => zlim_log::error!("{error}"),
                    }
                }

                // The base asset may be gone while its sub-assets are still in use: a type-erased
                // load finds the loader by extension and reloads them all.
                if !reloaded && server.0.read_infos().should_reload(&path) {
                    server.0.add_started_load_tasks(1);

                    match server
                        .load_internal(None, None, path.clone(), None, true)
                        .await
                    {
                        Ok(_) => reloaded = true,
                        Err(error) => zlim_log::error!("{error}"),
                    }
                }

                if log && reloaded {
                    zlim_log::info!("Reloaded {path}");
                }
            })
            .detach();
    }
}

// -----------------------------------------------------------------------------
// add

impl AssetServer {
    /// Registers `asset` with this server, without loading anything.
    ///
    /// A [`LoadedAsset`] carries no path, so the value is registered under its id alone.
    #[inline]
    pub(crate) fn add_typed_asset_impl<A: Asset>(&self, asset: LoadedAsset<A>) -> Handle<A> {
        self.add_erased_asset_impl(asset.erased(), core::any::type_name::<A>())
            .with_type_debug_checked()
    }

    /// Type-erased counterpart of [`add_typed_asset_impl`](Self::add_typed_asset_impl).
    pub(crate) fn add_erased_asset_impl(
        &self,
        loaded_asset: ErasedLoadedAsset,
        debug_name: &'static str,
    ) -> ErasedHandle {
        let handle = self
            .0
            .write_infos()
            .alloc_loading_handle_erased(loaded_asset.asset_type_id(), Some(debug_name));

        // Loaded Event -> propagate loaded -> change loading state to loaded
        self.0.send_asset_event(AssetServerEvent::Loaded {
            index: handle_index(&handle),
            loaded_asset,
        });

        handle
    }
}

// -----------------------------------------------------------------------------
// save

impl AssetServer {
    #[inline]
    pub(crate) fn push_save_command(&self, cmd: SaveCommand) {
        self.0.saves.push(cmd);
    }

    /// Runs every queued save to completion, on the IO pool.
    ///
    /// Reading the value a save writes needs the world (a handle only carries an id), which is why
    /// this takes one: it is the body of the world-accessing job in `server.rs`, and it is also
    /// what a driver — or a test — calls when it has a world in hand.
    pub(crate) fn run_pending_saves(&self, world: &World) {
        let hint = self.0.saves.len();
        let hint = hint + (hint >> 2);
        let mut buffer: VecDeque<SaveCommand> = VecDeque::with_capacity(hint);

        while let Some(cmd) = self.0.saves.pop() {
            buffer.push_back(cmd);
        }

        let mut saving: HashSet<AssetPath<'static>> = HashSet::with_capacity(hint);

        while !buffer.is_empty() {
            saving.clear();
            let mut index: usize = 0;

            IoTaskPool::get().scope(|scope| {
                while index < buffer.len() {
                    let path = buffer[index].path.clone_without_label();
                    if saving.contains(&path) {
                        index += 1;
                        continue;
                    }
                    saving.insert(path);
                    let cmd = buffer.swap_remove_front(index).unwrap();
                    self.0.add_started_load_tasks(1);

                    scope.spawn(async move {
                        if let Err(error) = self.save_internal(world, cmd).await {
                            ::core::hint::cold_path();
                            zlim_log::error!("{error}");
                        }
                    });
                }
            });
        }
    }

    /// Reads the value `id` refers to out of `world`, ready to be saved.
    ///
    /// This is the one step of a save that needs the world: a [`Handle`]
    /// only carries an id, and the values live in `Assets<A>`, which the server does not own. The
    /// returned asset borrows `world` for as long as it is alive.
    ///
    /// Returns [`None`] when the asset type was never registered with this server, or when the
    /// value is not there (not loaded, or already dropped).
    ///
    /// [`Handle`]: crate::handle::Handle
    fn resolve_saved_asset<'w>(
        &self,
        world: &'w World,
        id: ErasedAssetId,
    ) -> Option<ErasedSavedAsset<'w>> {
        let getter = {
            self.0
                .read_infos()
                .asset_getters
                .get(id.type_id())
                .copied()?
        };

        Some(ErasedSavedAsset::from_raw(getter(world, id)?))
    }

    /// Writes the asset that `id` refers to into `path`, and — when `save_meta` is set — the `.meta`
    /// the saver builds for it.
    ///
    /// Which saver runs follows from the asset type and the path (see [`AssetSavers::find`]). The
    /// bytes always go to the *source* side of the source that owns `path`: saving writes the asset
    /// the runtime would import, and the processed side belongs to whatever does the importing.
    ///
    /// [`AssetSavers::find`]: crate::saver::AssetSavers::find
    async fn save_internal(&self, world: &World, cmd: SaveCommand) -> Result<(), AssetSaveError> {
        let SaveCommand {
            handle,
            path,
            loader,
            saver,
            save_meta,
            override_unapproved,
            guard: _guard,
        } = cmd;

        validate_asset_path(&path, override_unapproved, &self.0.path_mode)?;

        let id = handle.id();
        let type_id = id.type_id();

        let Some(asset) = self.resolve_saved_asset(world, id) else {
            ::core::hint::cold_path();
            zlim_log::error!(
                "The asset at {id:?} is not loaded, so it cannot be saved to '{path}'; \
                register the asset type with `register_asset` and wait for the load first."
            );
            return Err(AssetError::WaitForAssetError(WaitForAssetError::NotLoaded).into());
        };

        let source = self.0.sources.get(path.source_id())?;

        // Must be `writer`, instead of `processed_writer`.
        let asset_writer = source.writer()?;

        let saver = self
            .0
            .read_savers()
            .find(None, saver.as_deref(), Some(type_id), Some(&path))
            .map_err(|error| {
                ::core::hint::cold_path();
                match error {
                    // The string that named the saver selects several of them: that is the error,
                    // not a missing saver.
                    Some(ambiguous) => AssetSaveError::from(ambiguous),
                    None => AssetSaveError::from(MissingAssetSaver::from(
                        MissingBuilder::new()
                            .may_with_type_name(saver.as_deref())
                            .with_asset_path(&path)
                            .with_asset_type_id(type_id),
                    )),
                }
            })?;

        if save_meta {
            let loader = loader.unwrap_or(Cow::Borrowed(""));
            let meta = saver.build_meta(&path, asset.clone(), None, loader).await?;
            asset_writer
                .write_meta_bytes(path.path(), &meta.serialize())
                .await?;
        } else if let Some(name) = loader.as_deref() {
            zlim_log::warn!(
                "save an asset `{path}` with specified loader name `{name}`, \
                but `save_builder.with_meta(true)` was not called, loader name is ignored."
            );
        }

        let mut writer = asset_writer.write(path.path()).await?;

        let save = AssertUnwindSafe(saver.save(&mut writer, &path, asset, None));

        match FutureExt::catch_unwind(save).await {
            Err(_) => {
                ::core::hint::cold_path();
                return Err(AssetSaverPanic {
                    path: path.clone(),
                    saver: saver.type_path(),
                }
                .into());
            }
            Ok(Err(error)) => {
                ::core::hint::cold_path();
                return Err(AssetSaverError {
                    path: path.clone(),
                    saver: saver.type_path(),
                    error: error.to_string(),
                }
                .into());
            }
            Ok(Ok(_)) => {}
        }

        writer.flush().await.map_err(|error| {
            ::core::hint::cold_path();
            AssetSaveError::from(AssetWriterError::from(error))
        })?;

        Ok(())
    }
}
