//! The public `AssetServer`: construction, queries, the load and save entry points, and the jobs
//! that drive both.

#![expect(clippy::module_inception, reason = "For better structure.")]

use core::any::TypeId;
use core::task::{Context, Poll};
use std::sync::Arc;

use zlim_core::derive::Resource;
use zlim_diagnostic::DiagnosticPath;
use zlim_path::derive::TypePath;

use super::builder::{LoadBuilder, SaveBuilder};
use super::config::{AssetMetaCheckMode, UnapprovedPathMode};
use super::state::{DependencyLoadState, LoadState, RecursiveDependencyLoadState};
use super::{AssetServerData, AssetServerMode};
use crate::asset::{Asset, VisitAssetDependencies};
use crate::assets::Assets;
use crate::error::{AssetError, AssetLoadError};
use crate::error::{AssetMetaWriteError, AssetSaveError};
use crate::error::{EmptyPathError, MissingAssetSource, UnapprovedPath, WaitForAssetError};
use crate::handle::{ErasedHandle, Handle};
use crate::ident::{AssetId, AssetSourceId, ErasedAssetId, TypedAssetIndex};
use crate::loaded::{LoadedAsset, LoadedFolder};
use crate::loader::AssetLoader;
use crate::meta::ErasedAssetMeta;
use crate::path::AssetPath;
use crate::saver::AssetSaver;
use crate::source::{AssetSource, AssetSources};

// -----------------------------------------------------------------------------
// AssetServer

/// Central coordinator for asset loading, caching, and lifecycle tracking.
///
/// [`AssetServer`] is a cheaply-cloneable handle to a sealed `AssetServerData` instance.
#[derive(TypePath, Resource, Clone)]
#[repr(transparent)]
pub struct AssetServer(pub(crate) Arc<AssetServerData>);

// -----------------------------------------------------------------------------
// Construction

impl AssetServer {
    /// Creates a new asset server reading from `sources`.
    ///
    /// `server_mode` picks the side of every source the server reads: the raw source itself, or the
    /// processed side that an importer writes. `meta_mode` decides when a `.meta` sidecar is
    /// consulted while loading. `path_mode` decides how a path that escapes its source root is
    /// treated. `watching_for_changes` makes the server track file dependencies and react to source
    /// changes, which is what hot reloading is built on.
    pub fn new(
        sources: Arc<AssetSources>,
        server_mode: AssetServerMode,
        meta_mode: AssetMetaCheckMode,
        path_mode: UnapprovedPathMode,
        watching_for_changes: bool,
    ) -> Self {
        Self::new_impl(
            sources,
            server_mode,
            meta_mode,
            path_mode,
            watching_for_changes,
        )
    }

    /// Registers `assets` of type `A` with this server.
    ///
    /// The server adopts the handle provider owned by `assets`, so this must be
    /// called once per asset type before any asset of that type can be loaded. It also records the
    /// getter used to read values out of the world for a save, and the senders that report this
    /// type's load success or failure.
    #[inline]
    pub fn register_asset<A: Asset>(&self, assets: &Assets<A>) {
        self.register_asset_impl(assets);
    }

    /// Registers `loader`, so that the assets it produces can be loaded.
    #[inline]
    pub fn register_loader<L: AssetLoader>(&self, loader: L) {
        self.register_loader_impl(loader);
    }

    /// Pre-registers `L` and the extensions it declares.
    ///
    /// The extensions are [`AssetLoader::EXTENSIONS`], the loader's own: they are what its
    /// registration claims later, so there is nothing to pass in.
    #[inline]
    pub fn preregister_loader<L: AssetLoader>(&self) {
        self.preregister_loader_impl::<L>();
    }

    /// Registers `saver`, so that the assets it writes can be saved.
    #[inline]
    pub fn register_saver<S: AssetSaver>(&self, saver: S) {
        self.register_saver_impl(saver);
    }

    /// Returns `true` when the loader is registered and ready.
    #[inline]
    pub fn contains_loader<L: AssetLoader>(&self) -> bool {
        self.contains_loader_impl::<L>()
    }

    /// Returns `true` when the saver is registered and ready.
    #[inline]
    pub fn contains_saver<S: AssetSaver>(&self) -> bool {
        self.contains_saver_impl::<S>()
    }
}

// -----------------------------------------------------------------------------
// Fields

impl AssetServer {
    /// Returns the [`AssetServerMode`] the server was constructed with.
    #[inline]
    pub fn server_mode(&self) -> AssetServerMode {
        self.0.server_mode
    }

    /// Returns the [`UnapprovedPathMode`] the server was constructed with.
    #[inline]
    pub fn unapproved_path_mode(&self) -> &UnapprovedPathMode {
        &self.0.path_mode
    }

    /// Returns the [`AssetMetaCheckMode`] the server was constructed with.
    #[inline]
    pub fn asset_meta_check_mode(&self) -> &AssetMetaCheckMode {
        &self.0.meta_mode
    }

    /// Returns whether the server tracks file dependencies and reacts to source changes.
    ///
    /// This is the flag hot reloading is gated on; see [`AssetServer::new`].
    #[inline]
    pub fn watching_for_changes(&self) -> bool {
        self.0.watching_for_changes
    }
}

// -----------------------------------------------------------------------------
// builder

impl AssetServer {
    /// Returns a builder that configures how the next asset is saved.
    ///
    /// See [`SaveBuilder`] for what can be configured, and what deliberately cannot.
    #[inline]
    pub fn save_builder(&self) -> SaveBuilder<'_> {
        SaveBuilder::new(self)
    }

    /// Returns a builder that configures how the next asset is loaded.
    ///
    /// See [`LoadBuilder`] for what can be configured, and what deliberately cannot.
    #[inline]
    pub fn load_builder(&self) -> LoadBuilder<'_> {
        LoadBuilder::new(self)
    }
}

// -----------------------------------------------------------------------------
// source

impl AssetServer {
    /// Returns all sources registered with this server.
    #[inline]
    pub fn sources(&self) -> &AssetSources {
        &self.0.sources
    }

    /// Returns the asset source registered under `source`.
    #[inline]
    #[doc(alias = "source")]
    pub fn get_source(
        &self,
        source: impl Into<AssetSourceId>,
    ) -> Result<&AssetSource, MissingAssetSource> {
        self.0.sources.get(source.into())
    }
}

// -----------------------------------------------------------------------------
// write_meta

impl AssetServer {
    /// Writes the `.meta` bytes for the asset at `path`.
    ///
    /// The `.meta` goes next to the *source* asset — that is the side a `.meta`
    /// describes. Whether an existing `.meta` may be replaced is `overwrite`:
    ///
    /// - `overwrite == false`: if a `.meta` already exists, this returns
    ///   [`AssetMetaWriteError::MetaAlreadyExists`] without writing anything.
    ///
    /// - `overwrite == true`: an existing `.meta` is replaced.
    ///
    /// # Errors
    ///
    /// Fails when the source is not registered, when the existing `.meta` cannot be
    /// checked (only when `overwrite == false`), when the source has no writer, or
    /// when the new `.meta` cannot be written.
    pub async fn write_meta(
        &self,
        path: impl Into<AssetPath<'_>>,
        meta: impl AsRef<[u8]>,
        overwrite: bool,
    ) -> Result<(), AssetMetaWriteError> {
        self.write_meta_raw(&path.into(), meta.as_ref(), overwrite)
            .await
    }

    /// Writes raw `.meta` bytes for the asset at `path`.
    ///
    /// This is the non-generic counterpart to [`AssetServer::write_meta`], see that for details.
    pub async fn write_meta_raw(
        &self,
        path: &AssetPath<'_>,
        meta: &[u8],
        overwrite: bool,
    ) -> Result<(), AssetMetaWriteError> {
        let source = self.resolve_meta_write_source(path, overwrite).await?;

        source.writer()?.write_meta_bytes(path.path(), meta).await?;

        Ok(())
    }

    /// Writes the `.meta` an asset with none should have: the one naming its loader, with default
    /// settings.
    ///
    /// - `overwrite == false`: if a `.meta` already exists, this returns
    ///   [`AssetMetaWriteError::MetaAlreadyExists`] without writing anything.
    ///
    /// - `overwrite == true`: an existing `.meta` is replaced.
    ///
    /// # Errors
    ///
    /// Fails when no loader can read the asset, when the source is not registered, when the source
    /// has no writer, and when the existing `.meta` cannot be checked or the new one cannot be
    /// written.
    pub async fn write_default_meta(
        &self,
        path: impl Into<AssetPath<'_>>,
        overwrite: bool,
    ) -> Result<(), AssetMetaWriteError> {
        let path = path.into();
        let loader = self.get_asset_loader_by_asset_path(&path).await?;
        let meta = loader.default_meta();
        self.write_meta_erased(&path, &*meta, overwrite).await
    }

    /// Writes [`ErasedAssetMeta`] for the asset at `path`.
    ///
    /// This is the type-erased counterpart to [`AssetServer::write_meta`], see that for what
    /// `overwrite` means.
    pub async fn write_meta_erased(
        &self,
        path: impl Into<AssetPath<'_>>,
        meta: &dyn ErasedAssetMeta,
        overwrite: bool,
    ) -> Result<(), AssetMetaWriteError> {
        let path = path.into();
        let source = self.resolve_meta_write_source(&path, overwrite).await?;

        let writer = source.writer()?;
        writer
            .write_meta_bytes(path.path(), &meta.serialize())
            .await?;
        Ok(())
    }

    /// Resolves the source that owns `path` and refuses to replace an existing `.meta` unless
    /// `overwrite` is set.
    ///
    /// Both `.meta` writers need this guard before they touch anything: the source is where the
    /// bytes go, and the existence check is what makes `overwrite == false` mean "never replace".
    /// The check follows the meta mode, so a mode that ignores `.meta` files does not start
    /// refusing writes because one happens to exist.
    async fn resolve_meta_write_source(
        &self,
        path: &AssetPath<'_>,
        overwrite: bool,
    ) -> Result<&AssetSource, AssetMetaWriteError> {
        let source = self
            .0
            .sources
            .get(path.source_id())
            .map_err(AssetMetaWriteError::from)?;

        if !overwrite && self.0.meta_mode.should_check(path) {
            match source.reader().read_meta(path.path()).await {
                Ok(_) => return Err(AssetMetaWriteError::MetaAlreadyExists),
                Err(e) if e.is_not_found() => {}
                Err(e) => return Err(AssetMetaWriteError::CheckError(e)),
            }
        }

        Ok(source)
    }
}

// -----------------------------------------------------------------------------
// handle

impl AssetServer {
    /// Returns `true` when `id` is an asset this server manages.
    ///
    /// This asks whether the server still tracks the id at all — not whether its value is loaded
    /// (see [`is_loaded`](Self::is_loaded) for that). A UUID id is never managed, and neither is
    /// an index whose slot was dropped after the last handle went away.
    #[inline]
    pub fn is_managed(&self, id: impl Into<ErasedAssetId>) -> bool {
        let Ok(index) = TypedAssetIndex::try_from(id.into()) else {
            return false;
        };

        self.0.read_infos().contains_key(index)
    }

    /// Returns `true` if any live handle refers to an asset at `path`.
    #[inline]
    pub fn contains_by_path<'a>(&self, path: impl Into<AssetPath<'a>>) -> bool {
        let path = path.into();
        self.0.read_infos().contains_by_path(&path)
    }

    /// Returns the path the asset with `id` was loaded from.
    ///
    /// Returns [`None`] for UUID assets and for indices the server doesn't know.
    #[doc(alias = "resolve_path")]
    pub fn get_path(&self, id: impl Into<ErasedAssetId>) -> Option<AssetPath<'static>> {
        let ErasedAssetId::Index { type_id, index } = id.into() else {
            return None;
        };

        let infos = self.0.read_infos();
        let path = infos
            .get(TypedAssetIndex::new(index, type_id))?
            .path
            .as_ref()?;

        Some(path.clone())
    }

    /// Returns the handle of the asset of type `A` at `path`, if it is known.
    pub fn get_handle<'a, A: Asset>(&self, path: impl Into<AssetPath<'a>>) -> Option<Handle<A>> {
        let path = path.into();

        self.0
            .read_infos()
            .get_handle_by_path_and_type_id(&path, TypeId::of::<A>())
            .map(ErasedHandle::with_type_debug_checked)
    }

    /// Returns the handle of one of the assets registered at `path`.
    ///
    /// Which one is unspecified when several asset types share the path;
    /// see [`get_erased_handles`](Self::get_erased_handles) for all of them.
    pub fn get_erased_handle<'a>(&self, path: impl Into<AssetPath<'a>>) -> Option<ErasedHandle> {
        let path = path.into().into_owned();

        self.0
            .read_infos()
            .get_handles_by_path(&path)
            .into_iter()
            .next()
    }

    /// Returns the handles of every asset registered at `path`, one per asset type.
    ///
    /// Empty when nothing was registered for it, and assets whose handles have been dropped are
    /// not reported. Unlike [`get_handle`](Self::get_handle) this names no asset type, so it
    /// answers for all of them at once.
    #[inline]
    pub fn get_erased_handles<'a>(&self, path: impl Into<AssetPath<'a>>) -> Vec<ErasedHandle> {
        let path = path.into().into_owned();

        self.0.read_infos().get_handles_by_path(&path)
    }

    /// Returns the id of one of the assets registered at `path`.
    ///
    /// Which one is unspecified when several asset types share the path;
    /// see [`get_erased_ids`](Self::get_erased_ids) for all of them.
    pub fn get_erased_id<'a>(&self, path: impl Into<AssetPath<'a>>) -> Option<ErasedAssetId> {
        let path = path.into().into_owned();

        self.0
            .read_infos()
            .get_indices_by_path(&path)
            .into_iter()
            .next()
    }

    /// Returns the ids of every asset registered at `path`, one per asset type.
    ///
    /// Empty when nothing was registered for it, and assets whose handles have been dropped are not
    /// reported. Unlike [`get_erased_id`](Self::get_erased_id), which returns one of them, this
    /// answers for all of them at once.
    #[inline]
    pub fn get_erased_ids<'a>(&self, path: impl Into<AssetPath<'a>>) -> Vec<ErasedAssetId> {
        let path = path.into().into_owned();

        self.0.read_infos().get_indices_by_path(&path)
    }
}

// -----------------------------------------------------------------------------
// handle lookups

impl AssetServer {
    /// Returns the handle of the asset with `id`, if the server still tracks it.
    ///
    /// The id is answered for whenever the server tracks it and a strong handle is alive, no matter
    /// how the value arrived — loaded from a path or [added](Self::add) directly — so this is also
    /// the way back from an id to a handle for an added asset. ([`Assets::resolve_handle`] does the
    /// same from the `Assets` collection's side.) A UUID id never has a handle.
    pub fn get_handle_by_id<A: Asset>(&self, id: AssetId<A>) -> Option<Handle<A>> {
        self.get_erased_handle_by_id(id.erased())
            .map(ErasedHandle::with_type_debug_checked)
    }

    /// Returns the handle of the asset with `id`, if the server still tracks it.
    ///
    /// This is the type-erased counterpart of [`get_handle_by_id`](Self::get_handle_by_id).
    pub fn get_erased_handle_by_id(&self, id: ErasedAssetId) -> Option<ErasedHandle> {
        let Ok(index) = TypedAssetIndex::try_from(id) else {
            // A UUID id is never managed by a server.
            return None;
        };

        self.0.read_infos().get_handle_by_index(index)
    }

    /// Returns the handle of the asset of type `type_id` at `path`, if it is known.
    ///
    /// This is [`get_handle`](Self::get_handle) with the asset type given as a [`TypeId`].
    pub fn get_handle_by_path_and_type_id(
        &self,
        path: &AssetPath<'_>,
        type_id: TypeId,
    ) -> Option<ErasedHandle> {
        self.0
            .read_infos()
            .get_handle_by_path_and_type_id(path, type_id)
    }
}

// -----------------------------------------------------------------------------
// load state

impl AssetServer {
    /// Returns `true` if the value of the asset with `id` has been loaded.
    ///
    /// Unknown (and UUID) assets are reported as `false`.
    pub fn is_loaded(&self, id: impl Into<ErasedAssetId>) -> bool {
        let ErasedAssetId::Index { type_id, index } = id.into() else {
            return false;
        };

        let infos = self.0.read_infos();
        let Some(info) = infos.get(TypedAssetIndex::new(index, type_id)) else {
            return false;
        };

        info.load_state.is_loaded()
    }

    /// Returns `true` if the asset with `id` finished loading **and** all of its dependencies did.
    ///
    /// Unknown (and UUID) assets are reported as `false`.
    #[doc(alias = "is_loaded_with_dependencies")]
    pub fn is_fully_loaded(&self, id: impl Into<ErasedAssetId>) -> bool {
        let ErasedAssetId::Index { type_id, index } = id.into() else {
            return false;
        };

        let infos = self.0.read_infos();

        let Some(info) = infos.get(TypedAssetIndex::new(index, type_id)) else {
            return false;
        };

        info.load_state.is_loaded() && info.rec_dep_load_state.is_loaded()
    }

    /// Returns `true` if every dependency of `value`, recursive ones included, is loaded.
    ///
    /// This asks about the dependencies a value *holds*, not about the value itself, which is what
    /// makes it usable on a resource or a component: it answers whether everything that value points
    /// at is ready. A UUID id is not tracked by a server at all, so it counts as loaded; an id the
    /// server no longer knows does not.
    pub fn are_dependencies_loaded(&self, value: &impl VisitAssetDependencies) -> bool {
        let infos = self.0.read_infos();
        let mut loaded = true;

        value.visit_dependencies(&mut |id| {
            if !loaded {
                return;
            }

            let ErasedAssetId::Index { type_id, index } = id else {
                return;
            };

            let Some(info) = infos.get(TypedAssetIndex::new(index, type_id)) else {
                loaded = false;
                return;
            };

            if !info.rec_dep_load_state.is_loaded() {
                loaded = false;
            }
        });

        loaded
    }

    /// Returns `true` if every direct dependency of `value` is loaded.
    ///
    /// Like [`are_dependencies_loaded`](Self::are_dependencies_loaded) this is about the value's
    /// dependencies rather than the value itself, but it does not look past them.
    pub fn are_direct_dependencies_loaded(&self, value: &impl VisitAssetDependencies) -> bool {
        let infos = self.0.read_infos();
        let mut loaded = true;

        value.visit_dependencies(&mut |id| {
            if !loaded {
                return;
            }

            let ErasedAssetId::Index { type_id, index } = id else {
                return;
            };

            let Some(info) = infos.get(TypedAssetIndex::new(index, type_id)) else {
                loaded = false;
                return;
            };

            if !info.dep_load_state.is_loaded() {
                loaded = false;
            }
        });

        loaded
    }

    /// Returns the [`LoadState`], the [`DependencyLoadState`] and the
    /// [`RecursiveDependencyLoadState`] of the asset with `id`, if the server knows it.
    pub fn get_load_states(
        &self,
        id: impl Into<ErasedAssetId>,
    ) -> Option<(LoadState, DependencyLoadState, RecursiveDependencyLoadState)> {
        let ErasedAssetId::Index { type_id, index } = id.into() else {
            return None;
        };

        let infos = self.0.read_infos();
        let info = infos.get(TypedAssetIndex::new(index, type_id))?;

        Some((
            info.load_state.clone(),
            info.dep_load_state.clone(),
            info.rec_dep_load_state.clone(),
        ))
    }

    /// Returns the [`LoadState`] of the asset with `id`, if the server knows it.
    pub fn get_load_state(&self, id: impl Into<ErasedAssetId>) -> Option<LoadState> {
        let ErasedAssetId::Index { type_id, index } = id.into() else {
            return None;
        };

        let infos = self.0.read_infos();
        let info = infos.get(TypedAssetIndex::new(index, type_id))?;

        Some(info.load_state.clone())
    }

    /// Returns the [`DependencyLoadState`] of the asset with `id`, if it is known.
    pub fn get_dependency_load_state(
        &self,
        id: impl Into<ErasedAssetId>,
    ) -> Option<DependencyLoadState> {
        let ErasedAssetId::Index { type_id, index } = id.into() else {
            return None;
        };

        let infos = self.0.read_infos();
        let info = infos.get(TypedAssetIndex::new(index, type_id))?;

        Some(info.dep_load_state.clone())
    }

    /// Returns the [`RecursiveDependencyLoadState`] of the asset with `id`, if it is known.
    pub fn get_recursive_dependency_load_state(
        &self,
        id: impl Into<ErasedAssetId>,
    ) -> Option<RecursiveDependencyLoadState> {
        let ErasedAssetId::Index { type_id, index } = id.into() else {
            return None;
        };

        let infos = self.0.read_infos();
        let info = infos.get(TypedAssetIndex::new(index, type_id))?;

        Some(info.rec_dep_load_state.clone())
    }

    /// Returns the [`LoadState`] of the asset with `id`.
    ///
    /// Unknown (and UUID) assets are reported as [`LoadState::NotLoaded`].
    pub fn load_state(&self, id: impl Into<ErasedAssetId>) -> LoadState {
        self.get_load_state(id.into())
            .unwrap_or(LoadState::NotLoaded)
    }

    /// Returns the [`DependencyLoadState`] of the asset with `id`.
    ///
    /// Unknown (and UUID) assets are reported as [`DependencyLoadState::NotLoaded`].
    pub fn dependency_load_state(&self, id: impl Into<ErasedAssetId>) -> DependencyLoadState {
        self.get_dependency_load_state(id.into())
            .unwrap_or(DependencyLoadState::NotLoaded)
    }

    /// Returns the [`RecursiveDependencyLoadState`] of the asset with `id`.
    ///
    /// Unknown (and UUID) assets are reported as [`RecursiveDependencyLoadState::NotLoaded`].
    pub fn recursive_dependency_load_state(
        &self,
        id: impl Into<ErasedAssetId>,
    ) -> RecursiveDependencyLoadState {
        self.get_recursive_dependency_load_state(id.into())
            .unwrap_or(RecursiveDependencyLoadState::NotLoaded)
    }
}

// -----------------------------------------------------------------------------
// load & save

impl AssetServer {
    /// Begins loading the asset of type `A` at `path` and returns its handle
    /// without waiting for the load to finish.
    #[inline]
    #[must_use = "not using the returned handle may cause the asset to be released"]
    pub fn load<'a, A: Asset>(&self, path: impl Into<AssetPath<'a>>) -> Handle<A> {
        self.load_builder().load(path)
    }

    /// Queues a save of the asset `handle` points at, to be written to `path`.
    ///
    /// Nothing is written yet: the bytes go out when the server next runs its save commands, and
    /// a failure there is logged rather than returned. See [`SaveBuilder`] for what can be
    /// configured.
    #[inline]
    pub fn save<'a, A: Asset>(&self, path: impl Into<AssetPath<'a>>, handle: Handle<A>) {
        self.save_builder().save(path.into(), handle);
    }

    /// Reloads every asset registered under `path`.
    ///
    /// This is what hot reloading calls when a watched file changes.
    #[inline]
    pub fn reload<'a>(&self, path: impl Into<AssetPath<'a>>) {
        self.reload_internal(path.into().into_owned(), true);
    }

    /// Loads every asset below `path` into a [`LoadedFolder`].
    ///
    /// Files whose extension has no loader are skipped; the folder is "fully loaded" once all
    /// the assets it contains are.
    #[inline]
    #[must_use = "not using the returned handle may cause the asset to be released"]
    pub fn load_folder<'a>(&self, path: impl Into<AssetPath<'a>>) -> Handle<LoadedFolder> {
        self.load_folder_impl(path.into().into_owned())
    }

    /// Reads the bytes at `path` and returns them, without a loader.
    ///
    /// No `.meta` is consulted and nothing is stored: this is for callers that handle the bytes
    /// themselves. The side that is read follows the server mode, exactly like a load.
    ///
    /// # Errors
    ///
    /// Fails when the path is empty, when it is unapproved and the server does not allow that,
    /// when the source has no such reader, or when reading fails.
    pub async fn load_bytes<'a>(
        &self,
        path: impl Into<AssetPath<'a>>,
    ) -> Result<Vec<u8>, AssetLoadError> {
        let path = path.into().into_owned();
        self.check_raw_path(&path)?;

        let source = self.0.sources.get(AssetSourceId::new(path.source_raw()))?;

        let reader = self.0.reader_for(source)?;

        reader
            .read_bytes(path.path())
            .await
            .map_err(AssetLoadError::from)
    }

    /// Writes `bytes` to `path`, without a saver.
    ///
    /// There is no asset to serialize, no meta and no dependencies: the bytes go straight to the
    /// writer of the source the path belongs to (the source side, like a save).
    ///
    /// # Errors
    ///
    /// Fails when the path is empty, when it is unapproved and the server does not allow that,
    /// when the source has no writer, or when writing fails.
    pub async fn save_bytes<'a>(
        &self,
        path: impl Into<AssetPath<'a>>,
        bytes: &[u8],
    ) -> Result<(), AssetSaveError> {
        let path = path.into().into_owned();
        self.check_raw_path(&path)?;

        let source = self.0.sources.get(AssetSourceId::new(path.source_raw()))?;

        source
            .writer()?
            .write_bytes(path.path(), bytes)
            .await
            .map_err(AssetSaveError::from)
    }

    /// Rejects the paths the raw byte APIs may not touch.
    ///
    /// They have no `override_unapproved` of their own, so an unapproved path is refused unless
    /// the server allows unapproved paths anyway.
    fn check_raw_path(&self, path: &AssetPath<'static>) -> Result<(), AssetError> {
        if path.path().as_os_str().is_empty() {
            ::core::hint::cold_path();
            return Err(EmptyPathError(path.clone()).into());
        }

        if path.is_unapproved() && !matches!(self.0.path_mode, UnapprovedPathMode::Allow) {
            ::core::hint::cold_path();
            return Err(UnapprovedPath(path.clone()).into());
        }

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// add

impl AssetServer {
    /// Registers `asset` with this server and returns its handle.
    ///
    /// Nothing is loaded: the value is already in hand, which makes this the way to publish
    /// procedurally generated assets.
    #[inline]
    #[must_use = "not using the returned handle may cause the asset to be released"]
    pub fn add<A: Asset>(&self, asset: impl Into<LoadedAsset<A>>) -> Handle<A> {
        self.add_typed_asset_impl(asset.into())
    }

    // NOTE: bevy also provides `add_async`, which registers the asset a future produces. Nothing
    // here needs it yet, so it is deliberately left out for now.
}

// -----------------------------------------------------------------------------
// waiting

impl AssetServer {
    /// Waits until the asset of `handle` **and all of its dependencies** finished loading.
    ///
    /// The handle is taken by reference: this makes sure it outlives the future, and the asset
    /// cannot be released while it is being awaited. For an id without a live handle, keep one
    /// alive elsewhere (or the asset may be dropped mid-wait).
    ///
    /// # Errors
    ///
    /// Returns:
    ///
    /// - [`Uuid`] when the handle is a UUID asset, which can never be waited for.
    /// - [`NotLoaded`] when the asset is not being loaded (nothing will ever complete it).
    /// - [`Failed`] when the asset itself failed.
    /// - [`DependencyFailed`] when something it depends on failed.
    ///
    /// [`Uuid`]: WaitForAssetError::Uuid
    /// [`Failed`]: WaitForAssetError::Failed
    /// [`NotLoaded`]: WaitForAssetError::NotLoaded
    /// [`DependencyFailed`]: WaitForAssetError::DependencyFailed
    pub async fn wait_for_asset<A: Asset>(
        &self,
        handle: &Handle<A>,
    ) -> Result<(), WaitForAssetError> {
        self.wait_for_asset_id(handle.id().erased()).await
    }

    /// Type-erased counterpart of [`wait_for_asset`].
    ///
    /// A UUID asset can never be waited for; this always returns an error.
    ///
    /// See [`wait_for_asset`] for details.
    ///
    /// [`wait_for_asset`]: Self::wait_for_asset
    pub async fn wait_for_asset_erased(
        &self,
        handle: &ErasedHandle,
    ) -> Result<(), WaitForAssetError> {
        self.wait_for_asset_id(handle.id()).await
    }

    /// Waits until the asset with `id` and all of its dependencies finished loading.
    ///
    /// A UUID asset can never be waited for; this always returns an error.
    ///
    /// See [`wait_for_asset`] for details.
    ///
    /// [`wait_for_asset`]: Self::wait_for_asset
    pub async fn wait_for_asset_id(
        &self,
        id: impl Into<ErasedAssetId>,
    ) -> Result<(), WaitForAssetError> {
        let Ok(index) = TypedAssetIndex::try_from(id.into()) else {
            // UUID assets are never loaded, so waiting for one is meaningless.
            return Err(WaitForAssetError::Uuid);
        };

        core::future::poll_fn(move |cx| self.wait_for_asset_id_poll_fn(cx, index)).await
    }

    /// The body of [`wait_for_asset_id`](Self::wait_for_asset_id).
    fn wait_for_asset_id_poll_fn(
        &self,
        cx: &mut Context<'_>,
        index: TypedAssetIndex,
    ) -> Poll<Result<(), WaitForAssetError>> {
        let infos = self.0.read_infos();

        let Some(info) = infos.get(index) else {
            return Poll::Ready(Err(WaitForAssetError::NotLoaded));
        };

        #[inline]
        fn is_loading(x: &LoadState, y: &RecursiveDependencyLoadState) -> bool {
            use RecursiveDependencyLoadState as RLoadState;
            matches!(
                (x, y),
                (LoadState::Loaded, RLoadState::NotLoaded)
                    | (LoadState::Loading, _)
                    | (_, RLoadState::Loading)
            )
        }

        if is_loading(&info.load_state, &info.rec_dep_load_state) {
            // `will_wake` makes repeated polls of the same task cheap.
            if info
                .waiting_tasks
                .iter()
                .any(|waker| waker.will_wake(cx.waker()))
            {
                return Poll::Pending;
            }

            let mut infos = {
                // The write guard can only be taken once the read guard is gone.
                ::core::mem::drop(infos);
                self.0.write_infos()
            };

            let Some(info) = infos.get_mut(index) else {
                return Poll::Ready(Err(WaitForAssetError::NotLoaded));
            };

            if is_loading(&info.load_state, &info.rec_dep_load_state) {
                // Leave the waker behind: whoever settles the state wakes it.
                info.waiting_tasks.push(cx.waker().clone());
            } else {
                // The state settled while the guard was reacquired: re-poll instead of
                // waiting for a wake-up that already happened.
                cx.waker().wake_by_ref();
            }

            return Poll::Pending;
        }

        match (&info.load_state, &info.rec_dep_load_state) {
            (LoadState::Loaded, RecursiveDependencyLoadState::Loaded) => Poll::Ready(Ok(())),
            (LoadState::NotLoaded, _) => Poll::Ready(Err(WaitForAssetError::NotLoaded)),
            (LoadState::Failed(error), _) => {
                Poll::Ready(Err(WaitForAssetError::Failed(Arc::clone(&error.0))))
            }
            (_, RecursiveDependencyLoadState::Failed(error)) => Poll::Ready(Err(
                WaitForAssetError::DependencyFailed(Arc::clone(&error.0)),
            )),
            _ => unreachable!(),
        }
    }
}

// -----------------------------------------------------------------------------
// job seal

// -----------------------------------------------------------------------------
// Diagnostic

impl AssetServer {
    /// Cumulative count of all load tasks started since the server was created.
    pub const STARTED_LOAD_COUNT: DiagnosticPath = DiagnosticPath::new("asset/started_load_count");
}

pub(crate) mod jobs {
    use super::AssetServer;
    use crate::event::{AssetSourceEvent, ErasedAssetLoadFailedEvent};
    use crate::ident::{AssetSourceId, TypedAssetIndex};
    use crate::path::AssetPath;
    use crate::server::info::AssetInfos;
    use crate::server::{AssetServerEvent, AssetServerMode};
    use core::task::Waker;
    use std::path::PathBuf;
    use zlim_core::borrow::{Res, ResMut};
    use zlim_core::job_fn;
    use zlim_core::system::If;
    use zlim_core::world::World;
    use zlim_diagnostic::Diagnostics;
    use zlim_utils::hash::HashSet;

    // -----------------------------------------------------------------------------
    // diagnostic

    #[job_fn(type = AssetServerDiagnostic)]
    fn asset_server_diagnostic_system(
        server: If<Res<AssetServer>>,
        mut store: If<ResMut<Diagnostics>>,
    ) {
        let started = server.0.into_inner().0.get_started_load_tasks();
        store.add_measurement(&AssetServer::STARTED_LOAD_COUNT, || started as f64);
    }

    // -----------------------------------------------------------------------------
    // Clear Finished Tasks

    #[job_fn(type = ClearFinishedAssetTask)]
    fn clear_asset_tasks(server: ResMut<AssetServer>) {
        server
            .0
            .write_infos()
            .pending_tasks
            .retain(|_, load_task| !load_task.is_finished());
    }

    // -----------------------------------------------------------------------------
    // HandleAssetSeverEvents

    #[job_fn(type = HandleAssetSeverEvents)]
    fn handle_asset_sever_events(world: &mut World) {
        world.resource_scope(|world, server: ResMut<AssetServer>| {
            let server = server.as_ref();
            let mut infos = server.0.write_infos();
            let mut failures: Vec<ErasedAssetLoadFailedEvent> = Vec::new();

            while let Some(event) = server.0.queue.pop() {
                match event {
                    AssetServerEvent::Failed { index, path, error } => {
                        infos.process_asset_fail(index, error.clone());

                        // Send untyped failure event
                        failures.push(ErasedAssetLoadFailedEvent {
                            id: index.into(),
                            path: path.clone(),
                            error: error.clone(),
                        });

                        // Send typed failure event
                        let sender = infos
                            .dependency_failed_event_sender
                            .get(index.type_id)
                            .expect("Asset failed event sender should exist");

                        sender(world, index.index, path, error);
                    }
                    AssetServerEvent::Loaded {
                        index,
                        loaded_asset,
                    } => {
                        infos.process_asset_load(index, loaded_asset, world, &server.0.queue);
                    }
                    AssetServerEvent::FullyLoaded { index } => {
                        let sender = infos
                            .dependency_loaded_event_sender
                            .get(index.type_id)
                            .expect("Asset event sender should exist");

                        sender(world, index.index);

                        if let Some(info) = infos.get_mut(index) {
                            core::mem::take(&mut info.waiting_tasks)
                                .into_iter()
                                .for_each(Waker::wake);
                        }
                    }
                }
            }

            ::core::mem::drop(infos);

            if !failures.is_empty() {
                world.write_message_batch::<ErasedAssetLoadFailedEvent>(failures);
            }

            // The following code all deals with hot-reloading,
            // which we can skip if the server isn't watching for changes.
            if server.watching_for_changes() {
                handle_hot_reload(server);
            }
        })
    }

    #[inline(never)]
    #[cfg_attr(not(debug_assertions), cold)]
    fn handle_hot_reload(server: &AssetServer) {
        let infos = server.0.read_infos();

        fn queue_ancestors(
            asset_path: &AssetPath<'_>,
            infos: &AssetInfos,
            paths_to_reload: &mut HashSet<AssetPath<'static>>,
        ) {
            if let Some(dependents) = infos.loader_dependents.get(asset_path) {
                for dependent in dependents {
                    paths_to_reload.insert(dependent.to_owned());
                    queue_ancestors(dependent, infos, paths_to_reload);
                }
            }
        }

        let mut folders_to_reload = Vec::new();
        let mut reload_parent_folders = |path: &PathBuf, source: &AssetSourceId| {
            for parent in path.ancestors().skip(1) {
                let parent_path = AssetPath::from_path(parent).with_source_id(source.clone());
                for folder_handle in infos.iter_handles_by_path(&parent_path) {
                    zlim_log::info!(
                        "Reloading folder {parent_path} because the content has changed"
                    );
                    folders_to_reload.push((folder_handle, parent_path.clone_owned()));
                }
            }
        };

        let mut paths_to_reload: HashSet<AssetPath<'static>> = HashSet::new();
        let mut reload_path = |path: PathBuf, source: &AssetSourceId| {
            let path = AssetPath::from(path).with_source_id(source.clone());
            queue_ancestors(&path, &infos, &mut paths_to_reload);
            paths_to_reload.insert(path);
        };

        let mut handle_event = |source: AssetSourceId, event: AssetSourceEvent| {
            match event {
                AssetSourceEvent::AddedAsset(path) => {
                    reload_parent_folders(&path, &source);
                    reload_path(path, &source);
                }
                // TODO: if the asset was processed and the processed file was changed,
                // the first modified event should be skipped?
                AssetSourceEvent::ModifiedAsset(path) | AssetSourceEvent::ModifiedMeta(path) => {
                    reload_path(path, &source);
                }
                AssetSourceEvent::RenamedFolder { old, new } => {
                    reload_parent_folders(&old, &source);
                    reload_parent_folders(&new, &source);
                }
                AssetSourceEvent::RemovedAsset(path)
                | AssetSourceEvent::RemovedFolder(path)
                | AssetSourceEvent::AddedFolder(path) => {
                    reload_parent_folders(&path, &source);
                }
                _ => {}
            }
        };

        for source in server.0.sources.iter() {
            // The side this server reads is the side it follows — and the *only* side it drains: the
            // events of the other side belong to whoever owns it (in processed mode the importer follows
            // the source side, so stealing its events would make it miss changes).
            match server.0.server_mode {
                AssetServerMode::Unprocessed => {
                    if let Some(receiver) = source.event_receiver() {
                        while let Some(event) = receiver.try_recv() {
                            handle_event(source.id(), event);
                        }
                    }

                    debug_assert!(
                        source.processed_event_receiver().is_none(),
                        "run on unprocessed mode, `processed_event_receiver` must be None"
                    );
                }
                AssetServerMode::Processed => {
                    // If `AssetProcessServer` is exist, `event_receiver` is received by it.
                    // If `AssetProcessServer` is not exist, `event_receiver` must be `None`.
                    // See `AssetPlugin` for details.

                    if let Some(receiver) = source.processed_event_receiver() {
                        while let Some(event) = receiver.try_recv() {
                            handle_event(source.id(), event);
                        }
                    }
                }
            }
        }

        ::core::mem::drop(infos);

        // The load count for these reloads is added by `load_folder_internal` and `reload_internal`.

        for (handle, path) in folders_to_reload {
            let index = TypedAssetIndex::try_from(handle.id())
                .expect("`iter_handles_by_path` yields strong handles");
            server.load_folder_internal(index, path);
        }

        for path in paths_to_reload {
            server.reload_internal(path, true);
        }
    }

    // -----------------------------------------------------------------------------
    // HandleAssetSaveCommands

    #[job_fn(type = HandleAssetSaveCommands, run_if = contains_save_command)]
    fn handle_asset_save_commands(world: &World, server: Res<AssetServer>) {
        let server = &*server;

        // The body lives on the server so that a driver (or a test) with a world in hand can run the
        // queue itself; this job is the world-accessing caller the App schedules.
        server.run_pending_saves(world);
    }

    fn contains_save_command(server: Res<AssetServer>) -> bool {
        !server.0.saves.is_empty()
    }
}
