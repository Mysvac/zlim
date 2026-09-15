use core::any::TypeId;
use std::borrow::Cow;

use atomicow::CowArc;
use zlim_utils::hash::map::Entry;
use zlim_utils::hash::{HashMap, HashSet};

use crate::asset::Asset;
use crate::error::{
    AssetError, AssetLoadError, LoadDirectError, MissingAssetLoader, MissingBuilder,
    ReadAssetBytesError,
};
use crate::handle::{ErasedHandle, Handle};
use crate::ident::{ErasedAssetId, TypedAssetIndex};
use crate::io::{AssetReaderError, Reader};
use crate::loaded::{ErasedLoadedAsset, LabeledAsset, LoadedAsset, LoadedUntypedAsset};
use crate::loader::AssetLoader;
use crate::meta::{AssetHash, AssetMetaParseError, ProcessedInfoMinimal};
use crate::path::AssetPath;
use crate::server::{AssetServer, HandleLoadingMode, UnapprovedPathMode};

/// Context passed to [`AssetLoader::load`] during an asset load.
///
/// The context is how a loader reaches the rest of the asset system: it collects the
/// dependencies of the asset being loaded (both the assets it references and the files it
/// reads), and it registers the sub-assets ("labeled assets") the format contains.
///
/// # Dependencies
///
/// [`LoadContext::load`] requests another asset from the server and records it as a
/// dependency, and [`LoadContext::read_asset_bytes`] reads a file directly and
/// records it as a *loader* dependency (which is what hot reloading uses). The server
/// only reports the asset as fully loaded once all of them are done.
///
/// A dependency is what ends up in the set [`finish`] hands to the server, and three things put a
/// handle there:
///
/// - every handle the value itself refers to (`Asset::visit_dependencies`), which is what makes a
///   loader that *stores* the handles it uses need no extra call;
/// - every handle [`load`] (and `load_erased` / `load_untyped`) returns;
/// - every handle [`add_dependency`] returns.
///
/// A sub-asset registered with [`add_labeled_asset`] is **not** one of them: the loader provides
/// that value itself, so nothing is loaded for it. Call [`add_dependency`] as well when the asset
/// should depend on it — otherwise the sub-asset is only known through its own label, and its
/// handle lives no longer than the load that produced it.
///
/// # Sub-assets
///
/// Formats that contain several assets (glTF, scenes, …) register each of them under a
/// label with [`LoadContext::add_labeled_asset`] (or [`LoadContext::labeled_asset_scope`]
/// / [`LoadContext::add_loaded_labeled_asset`]). Callers can then address them with the
/// `#label` suffix, for example `"scene.gltf#Mesh0"`.
///
/// # Finishing
///
/// Call [`LoadContext::finish`] with the primary asset value to produce the
/// [`LoadedAsset`] the server stores.
///
/// [`LoadedAsset`]: crate::loaded::LoadedAsset
/// [`AssetLoader::load`]: crate::loader::AssetLoader::load
/// [`load`]: LoadContext::load
/// [`finish`]: LoadContext::finish
/// [`add_labeled_asset`]: LoadContext::add_labeled_asset
/// [`add_dependency`]: LoadContext::add_dependency
pub struct LoadContext<'a> {
    // use `&AssetServerData` instead of `&AssetServer`
    // to reduce once indirect addressing
    pub(crate) asset_server: &'a AssetServer,

    /// Whether every file [read](Self::read_asset_bytes) through this context also records the
    /// content hash from its `.meta` file; a plain load leaves the hashes zero, and the importer
    /// sets this because it needs them.
    pub(crate) populate_hashes: bool,
    /// Whether the assets a loader requests are started as loads. The importer sets this to `false`:
    /// it only wants the handles and the recorded hashes, not the loads themselves.
    pub(crate) load_dependencies: bool,

    pub(crate) asset_path: AssetPath<'static>,
    pub(crate) dependencies: HashSet<TypedAssetIndex>,
    /// Stores the subassets added to this context.
    pub(crate) labeled_assets: Vec<LabeledAsset>,
    pub(crate) label_to_label_index: HashMap<CowArc<'static, str>, usize>,
    pub(crate) asset_to_label_index: HashMap<ErasedAssetId, usize>,

    /// Direct dependencies used by this loader.
    pub(crate) loader_dependencies: HashMap<AssetPath<'static>, AssetHash>,
}

impl<'a> LoadContext<'a> {
    /// Creates a new [`LoadContext`] instance.
    pub(crate) fn new(
        asset_server: &'a AssetServer,
        asset_path: AssetPath<'static>,
        load_dependencies: bool,
        populate_hashes: bool,
    ) -> Self {
        Self {
            asset_server,
            asset_path,
            load_dependencies,
            populate_hashes,
            dependencies: HashSet::new(),
            loader_dependencies: HashMap::new(),
            labeled_assets: Vec::new(),
            label_to_label_index: HashMap::new(),
            asset_to_label_index: HashMap::new(),
        }
    }

    /// The path of the asset this context loads.
    #[inline]
    pub fn path(&self) -> &AssetPath<'static> {
        &self.asset_path
    }

    /// Creates a child [`LoadContext`] for building a labeled sub-asset.
    ///
    /// Finish the child context with [`LoadContext::finish`] and pass the result to
    /// [`LoadContext::add_loaded_labeled_asset`]: the sub-asset's
    /// dependencies and loader dependencies are collected by the child and moved into the
    /// labeled asset, so they stay attached to the sub-asset rather than to its parent.
    ///
    /// [`LoadContext::finish`]: crate::loader::LoadContext::finish
    /// [`LoadContext::add_loaded_labeled_asset`]: crate::loader::LoadContext::add_loaded_labeled_asset
    #[inline]
    pub fn begin_labeled_asset(&self) -> LoadContext<'a> {
        Self {
            asset_server: self.asset_server,
            asset_path: self.asset_path.clone(),
            load_dependencies: self.load_dependencies,
            populate_hashes: self.populate_hashes,
            dependencies: HashSet::new(),
            loader_dependencies: HashMap::new(),
            labeled_assets: Vec::new(),
            label_to_label_index: HashMap::new(),
            asset_to_label_index: HashMap::new(),
        }
    }
}

// -----------------------------------------------------------------------------
// finish

impl LoadContext<'_> {
    /// "Finishes" this context by populating the final [`Asset`] value.
    ///
    /// Every handle the value refers to (`Asset::visit_dependencies`) is added to the dependencies
    /// of the finished asset, so a loader that stores the handles it uses needs no other call — see
    /// [`LoadContext`]'s "Dependencies" section for what else does and does not land there.
    pub fn finish<A: Asset>(mut self, value: A) -> LoadedAsset<A> {
        // At this point, we assume the asset/subasset is "locked in" and won't be changed, so we
        // can ensure all the dependencies are included (in case a handle was used without loading
        // it through this `LoadContext`). If in the future we provide an API for mutating assets in
        // `LoadedAsset`, `ErasedLoadedAsset`, or `LoadContext` (for mutating existing subassets),
        // we should move this to some point after those mutations are not possible. This spot is
        // convenient because we still have access to the static type of `A`.
        value.visit_dependencies(&mut |asset_id| {
            let (type_id, index) = match asset_id {
                ErasedAssetId::Index { type_id, index } => (type_id, index),
                // UUID assets can't be loaded anyway, so just ignore this ID.
                ErasedAssetId::Uuid { .. } => return,
            };
            self.dependencies.insert(TypedAssetIndex { type_id, index });
        });

        LoadedAsset {
            value,
            dependencies: self.dependencies,
            loader_dependencies: self.loader_dependencies,
            labeled_assets: self.labeled_assets,
            label_to_label_index: self.label_to_label_index,
            asset_to_label_index: self.asset_to_label_index,
        }
    }
}

impl LoadContext<'_> {
    /// Returns `true` if a sub-asset with this `label` is **currently alive**.
    pub fn has_labeled_asset(&self, label: impl Into<CowArc<'static, str>>) -> bool {
        let path = self.asset_path.clone().with_label(label);
        self.asset_server.contains_by_path(path)
    }

    /// Returns the erased sub-asset registered under `label`.
    #[doc(alias = "get_labeled")]
    pub fn labeled(&self, label: impl AsRef<str>) -> Option<&ErasedLoadedAsset> {
        let index = self.label_to_label_index.get(label.as_ref())?;
        Some(&self.labeled_assets[*index].asset)
    }

    /// Returns the erased sub-asset identified by `id`.
    #[doc(alias = "get_labeled_by_id")]
    pub fn labeled_by_id(&self, id: impl Into<ErasedAssetId>) -> Option<&ErasedLoadedAsset> {
        let index = self.asset_to_label_index.get(&id.into())?;
        Some(&self.labeled_assets[*index].asset)
    }
}

impl LoadContext<'_> {
    /// Returns a handle for the sub-asset called `label`, **recording it as a dependency**.
    ///
    /// This is the call that makes an asset depend on one of its own sub-assets:
    /// [`add_labeled_asset`] and friends register the sub-asset's *value*, but nothing is loaded
    /// for it, so they do not make the parent depend on it. The handle this returns is a way of
    /// naming that sub-asset — storing it inside the asset would have recorded the dependency on
    /// its own, through [`finish`]'s `visit_dependencies`.
    ///
    /// Call this before *or* after [`add_labeled_asset`]; the returned handle refers to the same
    /// asset either way. This context must eventually register an asset of type `A` under `label`,
    /// otherwise the dependencies of this asset never become "fully loaded".
    ///
    /// [`add_labeled_asset`]: LoadContext::add_labeled_asset
    /// [`finish`]: LoadContext::finish
    #[doc(alias = "add_label")]
    #[doc(alias = "register_label")]
    #[doc(alias = "register_dependency")]
    #[must_use = "not using the returned handle may cause the asset to be released"]
    pub fn add_dependency<A: Asset>(
        &mut self,
        label: impl Into<CowArc<'static, str>>,
    ) -> Handle<A> {
        let path = self.asset_path.clone().with_label(label);

        let handle: Handle<A> = self
            .asset_server
            .0
            .write_infos()
            .get_or_alloc_handle::<A>(path, HandleLoadingMode::NotLoading);

        if let Handle::Strong(h) = &handle {
            self.dependencies
                .insert(TypedAssetIndex::new(h.index, h.type_id));
        } else {
            unreachable!("handles allocated by the asset server are never UUID handles")
        }
        handle
    }

    fn add_labeled_internal<A: Asset>(
        &mut self,
        label: CowArc<'static, str>,
        loaded_asset: LoadedAsset<A>,
    ) -> Handle<A> {
        let labeled_path = self.asset_path.clone().with_label(label.clone());

        // A sub-asset is provided by this loader, so no load is requested for it.
        let handle: Handle<A> = self
            .asset_server
            .0
            .write_infos()
            .get_or_alloc_handle::<A>(labeled_path, HandleLoadingMode::NotLoading);

        let asset = LabeledAsset {
            asset: loaded_asset.erased(),
            handle: handle.erased(),
        };

        match self.label_to_label_index.entry(label) {
            Entry::Occupied(entry) => {
                ::core::hint::cold_path();
                zlim_log::warn!(
                    "Duplicate label '{}' for asset '{}': the previously registered sub-asset \
                    is replaced. If that is unintended, it may indicate a bug in the loader.",
                    entry.key(),
                    self.asset_path,
                );

                let index = *entry.get();
                // `asset_to_label_index` needs no update: the same path and type always map
                // to the same handle for as long as that handle is alive, and we hold it in the
                // `LabeledAsset` we are replacing.
                self.labeled_assets[index] = asset;
            }
            Entry::Vacant(entry) => {
                let index = self.labeled_assets.len();

                entry.insert(index);
                let k = handle.id().erased();
                self.asset_to_label_index.insert(k, index);
                self.labeled_assets.push(asset);
            }
        }

        handle
    }

    /// Registers an already built [`LoadedAsset`] under `label` and returns a strong handle.
    ///
    /// Re-using a label replaces the previous sub-asset (with a warning): a sub-asset can only
    /// be identified by its label, so a duplicate is treated as a loader bug.
    ///
    /// This registers the sub-asset's value, and nothing else: the asset being loaded does *not*
    /// depend on it. Call [`LoadContext::add_dependency`] for the same `label` when it should.
    #[inline]
    pub fn add_loaded_labeled_asset<A: Asset>(
        &mut self,
        label: impl Into<CowArc<'static, str>>,
        loaded_asset: LoadedAsset<A>,
    ) -> Handle<A> {
        self.add_labeled_internal(label.into(), loaded_asset)
    }

    /// Registers `asset` as the sub-asset called `label` and returns a strong handle to it.
    ///
    /// The value is provided by this load, so nothing is loaded for it and the asset being loaded
    /// does not depend on it; call [`LoadContext::add_dependency`] for the same `label` to make it
    /// a dependency (see [`LoadContext`]'s "Dependencies" section).
    #[inline]
    pub fn add_labeled_asset<A: Asset>(
        &mut self,
        label: impl Into<CowArc<'static, str>>,
        asset: A,
    ) -> Handle<A> {
        // Manually inline, faster than `labeled_asset_scope`.
        let context = self.begin_labeled_asset();
        let loaded_asset = context.finish(asset);
        self.add_labeled_internal(label.into(), loaded_asset)
    }

    /// Builds a labeled sub-asset inside `load` and registers it under `label`.
    ///
    /// The closure receives a child context, so the sub-asset's *own* dependencies are recorded
    /// with it — that is the reason to use this over [`add_labeled_asset`]. It does not make the
    /// asset being loaded depend on the sub-asset: for that, call
    /// [`LoadContext::add_dependency`] with the same `label`. Nothing is registered when the
    /// closure returns an error.
    ///
    /// [`add_labeled_asset`]: Self::add_labeled_asset
    #[inline]
    pub fn labeled_asset_scope<A: Asset, E>(
        &mut self,
        label: impl Into<CowArc<'static, str>>,
        load: impl FnOnce(&mut LoadContext) -> Result<A, E>,
    ) -> Result<Handle<A>, E> {
        let mut context = self.begin_labeled_asset();
        let asset = load(&mut context)?;
        let loaded_asset = context.finish(asset);
        Ok(self.add_labeled_internal(label.into(), loaded_asset))
    }
}

// -----------------------------------------------------------------------------
// read asset bytes

impl LoadContext<'_> {
    /// Reads all bytes of the asset at `path` and records it as a *loader* dependency.
    ///
    /// Loader dependencies are file-level: they drive hot reloading (a change to any of them
    /// reloads this asset) and are stored in the server's per-asset record (`AssetInfo`) rather
    /// than in the dependency graph.
    ///
    /// When `populate_hashes` is enabled (the asset processor), the asset's content hash is
    /// read from its `.meta` file as well, and an error is returned when that hash is missing.
    #[inline]
    pub fn read_asset_bytes<'b, 'c>(
        &'b mut self,
        path: impl Into<AssetPath<'c>>,
    ) -> impl Future<Output = Result<Vec<u8>, ReadAssetBytesError>> + Send {
        self.rdbytes_impl(path.into())
    }

    async fn rdbytes_impl(&mut self, path: AssetPath<'_>) -> Result<Vec<u8>, ReadAssetBytesError> {
        if path.path().as_os_str().is_empty() {
            return Err(ReadAssetBytesError::EmptyPath(path.into_owned()));
        }

        let source = self.asset_server.get_source(path.source_id())?;

        let asset_reader = self.asset_server.0.reader_for(source)?;

        let mut reader = asset_reader.read(path.path()).await?;

        let hash: AssetHash = if self.populate_hashes {
            // NOTE: read the meta while the asset reader is still active, so that the
            // hash and the bytes come from the same revision of the asset.
            let meta_bytes = asset_reader.read_meta_bytes(path.path()).await?;

            let minimal = ProcessedInfoMinimal::from_bytes(&meta_bytes).map_err(|error| {
                ::core::hint::cold_path();
                let path: Box<str> = path.to_string().into_boxed_str();
                let err = AssetMetaParseError { path, error };
                ReadAssetBytesError::AssetMetaParseError(err)
            })?;

            let processed_info = minimal.processed_info.ok_or_else(|| {
                ::core::hint::cold_path();
                ReadAssetBytesError::MissingAssetHash(path.clone_owned())
            })?;

            processed_info.full_hash
        } else {
            AssetHash::ZERO
        };

        let mut bytes = Vec::new();

        if let Err(error) = reader.read_all_bytes(&mut bytes).await {
            ::core::hint::cold_path();
            return Err(AssetReaderError::from(error).into());
        }

        self.loader_dependencies.insert(path.clone_owned(), hash);
        Ok(bytes)
    }
}

// -----------------------------------------------------------------------------

/// A builder for loading nested assets inside a [`LoadContext`].
///
/// Unlike the server's own builder, a nested load happens right here — the loader that runs it is
/// already inside its own load — so the terminal methods are `async` and return the loaded data (or
/// a handle, for the deferred forms).
pub struct NestedLoadBuilder<'ctx, 'builder> {
    load_context: &'builder mut LoadContext<'ctx>,
    /// The loader the caller forces, if any: a type path or a type name, as the registry accepts both.
    loader_name: Option<Cow<'static, str>>,
    /// Whether unapproved paths are allowed to be loaded.
    override_unapproved: bool,
}

impl<'c, 'b> NestedLoadBuilder<'c, 'b> {
    #[inline(always)]
    fn new(load_context: &'b mut LoadContext<'c>) -> Self {
        NestedLoadBuilder {
            load_context,
            loader_name: None,
            override_unapproved: false,
        }
    }

    /// Sets whether a path that escapes its source root is loaded anyway.
    #[inline]
    #[must_use = "the load doesn't start until LoadBuilder has been consumed"]
    pub fn override_unapproved(mut self, value: bool) -> Self {
        self.override_unapproved = value;
        self
    }

    /// Forces `L` to be the loader that reads the nested asset.
    ///
    /// Without it the asset's `.meta` chooses the loader, and the path's extension after that; with
    /// it the `.meta` still provides the settings, but no longer the choice. This is the typed form of
    /// [`with_loader_name`](Self::with_loader_name): it passes the fully-qualified type path of `L`,
    /// which is always registered under exactly that key.
    #[inline]
    #[must_use = "the load doesn't start until LoadBuilder has been consumed"]
    pub fn with_loader<L: AssetLoader>(mut self) -> Self {
        self.loader_name = Some(Cow::Borrowed(L::type_path()));
        self
    }

    /// Forces the loader registered under `name` to be the loader that reads the nested asset.
    ///
    /// The name is the lenient form: a fully-qualified type path and a short type name both resolve.
    ///
    /// **An empty `name` is not a name.** It is treated exactly like not specifying a loader at all —
    /// the look-up proceeds as if this method had not been called — which is the same convention
    /// [`AssetMeta`](crate::meta::AssetMeta) uses: an empty loader name in a config means "this config
    /// names no loader" (it is not even serialized), so the two spell the same thing. If there is no
    /// name to force, simply do not call this.
    #[inline]
    #[must_use = "the load doesn't start until LoadBuilder has been consumed"]
    pub fn with_loader_name(mut self, name: impl Into<Cow<'static, str>>) -> Self {
        self.loader_name = Some(name.into());
        self
    }
}

// -----------------------------------------------------------------------------
// Load Handle

impl<'c, 'b> NestedLoadBuilder<'c, 'b> {
    /// Loads the provided path as the given type and returns the handle.
    ///
    /// This is a "deferred" load, meaning the caller will not have access to the loaded data; to
    /// access the loaded data, use [`Self::load_value`].
    #[inline]
    pub fn load<'a, A: Asset>(self, path: impl Into<AssetPath<'a>>) -> Handle<A> {
        let debug_name = Some(core::any::type_name::<A>());
        // The doc comment slightly lies: if `load_dependencies` is false (the importer), the load
        // will not be started, but the matching handle will still be returned. The caller
        // can't tell the difference.
        self.load_internal(TypeId::of::<A>(), debug_name, path.into().into_owned())
            .with_type_debug_checked()
    }

    /// Loads the provided path as the given type and returns the handle.
    ///
    /// This is a "deferred" load, meaning the caller will not have access to the loaded data; to
    /// access the loaded data, use [`Self::load_erased_value`].
    #[inline]
    pub fn load_erased<'a>(self, type_id: TypeId, path: impl Into<AssetPath<'a>>) -> ErasedHandle {
        self.load_internal(type_id, None, path.into().into_owned())
    }

    /// Loads the provided path with an unknown type (which is guessed based on the path or meta
    /// file).
    ///
    /// This is a "deferred" load, meaning the caller will not have access to the loaded data; to
    /// access the loaded data, use [`Self::load_untyped_value`].
    #[inline]
    pub fn load_untyped<'a>(self, path: impl Into<AssetPath<'a>>) -> Handle<LoadedUntypedAsset> {
        self.load_untyped_internal(path.into().into_owned())
    }
}

impl<'c, 'b> NestedLoadBuilder<'c, 'b> {
    fn load_internal(
        self,
        type_id: TypeId,
        debug_name: Option<&'static str>,
        path: AssetPath<'static>,
    ) -> ErasedHandle {
        if path.path().as_os_str().is_empty() {
            ::core::hint::cold_path();
            zlim_log::error!("Attempted to load an asset with an empty path \"{path}\"!");
            return ErasedHandle::default_for_type(type_id);
        }

        if path.is_unapproved() {
            ::core::hint::cold_path();
            match (
                &self.load_context.asset_server.0.path_mode,
                self.override_unapproved,
            ) {
                // Explicitly allowed by the server, or by the caller.
                (UnapprovedPathMode::Allow, _) | (UnapprovedPathMode::Deny, true) => {}
                (UnapprovedPathMode::Deny, false) | (UnapprovedPathMode::Forbid, _) => {
                    zlim_log::error!(
                        "Attempted to load an unapproved asset path (escapes the source root) \"{path}\"."
                    );
                    return ErasedHandle::default_for_type(type_id);
                }
            }
        }

        let handle = if self.load_context.load_dependencies {
            self.load_context.asset_server.load_erased_asset_impl(
                self.loader_name,
                path,
                type_id,
                debug_name,
                self.override_unapproved,
                None,
            )
        } else {
            // not load_dependencies -> nest loader is a dep -> NotLoading
            const M: HandleLoadingMode = HandleLoadingMode::NotLoading;
            self.load_context
                .asset_server
                .0
                .write_infos()
                .get_or_alloc_handle_erased(path, M, type_id, debug_name)
                .0
        };

        // `load_with_meta_transform` returns a default `Uuid` handle when it refuses to start the
        // load, for example because the path is unapproved. There is no load to track as a
        // dependency in that case, and the reason has already been logged.
        if let ErasedHandle::Strong(h) = &handle {
            let index = TypedAssetIndex::new(h.index, h.type_id);
            self.load_context.dependencies.insert(index);
        }

        handle
    }

    fn load_untyped_internal(self, path: AssetPath<'static>) -> Handle<LoadedUntypedAsset> {
        if path.path().as_os_str().is_empty() {
            ::core::hint::cold_path();
            zlim_log::error!("Attempted to load an asset with an empty path \"{path}\"!");
            return Handle::default();
        }

        if path.is_unapproved() {
            ::core::hint::cold_path();
            match (
                &self.load_context.asset_server.0.path_mode,
                self.override_unapproved,
            ) {
                // Explicitly allowed by the server, or by the caller.
                (UnapprovedPathMode::Allow, _) | (UnapprovedPathMode::Deny, true) => {}
                (UnapprovedPathMode::Deny, false) | (UnapprovedPathMode::Forbid, _) => {
                    zlim_log::error!(
                        "Attempted to load an unapproved asset path (escapes the source root) \"{path}\"."
                    );
                    return Handle::default();
                }
            }
        }

        let handle = if self.load_context.load_dependencies {
            self.load_context.asset_server.load_untyped_asset_impl(
                self.loader_name,
                path,
                self.override_unapproved,
                None,
            )
        } else {
            // not load_dependencies -> NotLoading
            const M: HandleLoadingMode = HandleLoadingMode::NotLoading;
            self.load_context
                .asset_server
                .0
                .write_infos()
                .get_or_alloc_handle(path, M)
        };

        // `load_untyped_asset_impl` returns a default `Uuid` handle when it refuses to start the
        // load, for example because the path is unapproved. There is no load to track as a
        // dependency in that case, and the reason has already been logged.
        if let Handle::Strong(h) = &handle {
            let index = TypedAssetIndex::new(h.index, h.type_id);
            self.load_context.dependencies.insert(index);
        }

        handle
    }
}

// -----------------------------------------------------------------------------
// Load Value

enum ReaderRef<'a> {
    Borrowed(&'a mut dyn Reader),
    Boxed(Box<dyn Reader + 'a>),
}

impl ReaderRef<'_> {
    #[inline]
    fn as_mut(&mut self) -> &mut dyn Reader {
        match self {
            ReaderRef::Borrowed(r) => &mut **r,
            ReaderRef::Boxed(b) => &mut **b,
        }
    }
}

impl<'c, 'b> NestedLoadBuilder<'c, 'b> {
    async fn load_typed_value_internal<A: Asset>(
        self,
        path: AssetPath<'static>,
        reader: Option<&'b mut dyn Reader>,
    ) -> Result<LoadedAsset<A>, LoadDirectError> {
        self.load_value_internal(
            Some(TypeId::of::<A>()),
            Some(core::any::type_name::<A>()),
            &path,
            reader,
        )
        .await
        .and_then(move |untyped_asset| {
            if untyped_asset.asset_type_id() == TypeId::of::<A>() {
                Ok(untyped_asset.with_type::<A>())
            } else {
                ::core::hint::cold_path();
                Err(LoadDirectError::AssetTypeMismatch {
                    path,
                    expect: ::core::any::type_name::<A>(),
                    actual: untyped_asset.asset_type_path(),
                })
            }
        })
    }

    async fn load_value_internal(
        self,
        type_id: Option<TypeId>,
        debug_name: Option<&'static str>,
        path: &AssetPath<'static>,
        reader: Option<&'b mut dyn Reader>,
    ) -> Result<ErasedLoadedAsset, LoadDirectError> {
        if path.path().as_os_str().is_empty() {
            ::core::hint::cold_path();
            return Err(LoadDirectError::EmptyPath(path.clone()));
        }

        if path.label().is_some() {
            ::core::hint::cold_path();
            return Err(LoadDirectError::RequestedSubAsset(path.clone()));
        }

        if path.is_unapproved() {
            ::core::hint::cold_path();
            match (
                &self.load_context.asset_server.0.path_mode,
                self.override_unapproved,
            ) {
                // Explicitly allowed by the server, or by the caller.
                (UnapprovedPathMode::Allow, _) | (UnapprovedPathMode::Deny, true) => {}
                (UnapprovedPathMode::Deny, false) | (UnapprovedPathMode::Forbid, _) => {
                    ::core::hint::cold_path();
                    return Err(LoadDirectError::UnapprovedPath(path.clone()));
                }
            }
        }

        let (meta, loader, mut reader) = if let Some(reader) = reader {
            let error = |error: AssetError| {
                ::core::hint::cold_path();
                let error = AssetLoadError::from(error);
                let asset = path.clone();
                LoadDirectError::AssetLoadError { asset, error }
            };

            let missing = || {
                ::core::hint::cold_path();
                MissingAssetLoader::from(
                    MissingBuilder::new()
                        .with_asset_path(path)
                        .may_with_asset_type(debug_name)
                        .may_with_asset_type_id(type_id),
                )
            };

            // The registry lock is released before the await:
            // the loader may still be waiting to be registered.
            let entry = {
                self.load_context.asset_server.0.read_loaders().find(
                    None,
                    self.loader_name.as_deref(),
                    type_id,
                    Some(path),
                )
            };

            let entry = match entry {
                Ok(entry) => entry,
                // Only the loader type path is not passed; a short name that several loaders share
                // is carried by the arm below, so this arm is the plain miss.
                Err(None) => {
                    ::core::hint::cold_path();
                    return Err(error(missing().into()));
                }
                Err(Some(ambiguous)) => {
                    ::core::hint::cold_path();
                    return Err(error(ambiguous.into()));
                }
            };

            let loader = match entry {
                Ok(loader) => loader,
                Err(pending) => {
                    ::core::hint::cold_path();
                    pending.get().await.ok_or_else(|| error(missing().into()))?
                }
            };

            let meta = loader.default_meta();
            (meta, loader, ReaderRef::Borrowed(reader))
        } else {
            let (meta, loader, reader) = self
                .load_context
                .asset_server
                .get_meta_loader_and_reader(self.loader_name.as_deref(), path, type_id, debug_name)
                .await
                .map_err(|error| {
                    ::core::hint::cold_path();
                    let asset = path.clone();
                    LoadDirectError::AssetLoadError { asset, error }
                })?;
            (meta, loader, ReaderRef::Boxed(reader))
        };

        let settings = meta
            .loader_settings()
            .expect("the meta of a loaded asset is always a `Load` config");

        let loaded_asset = self
            .load_context
            .asset_server
            .load_with_loader(
                path,
                settings,
                &*loader,
                reader.as_mut(),
                self.load_context.load_dependencies,
                self.load_context.populate_hashes,
            )
            .await
            .map_err(|error| {
                ::core::hint::cold_path();
                let asset = path.clone();
                LoadDirectError::AssetLoadError { asset, error }
            })?;

        let processed_info = meta.processed_info().as_ref();
        let hash = processed_info.map(|i| i.full_hash).unwrap_or_default();
        self.load_context
            .loader_dependencies
            .insert(path.clone(), hash);

        Ok(loaded_asset)
    }
}

impl<'c, 'b> NestedLoadBuilder<'c, 'b> {
    /// Loads the provided path as the given type, returning the loaded data.
    ///
    /// This load is async and therefore needs to be awaited before returning the loaded data.
    ///
    /// # Errors
    ///
    /// The variants below are the ones a caller cannot guess; everything else the load reported —
    /// a missing loader or asset reader, settings that do not parse, and whatever the loader itself
    /// failed with (or panicked with) — arrives as [`LoadDirectError::AssetLoadError`].
    ///
    /// - [`LoadDirectError::EmptyPath`]: the path names no file.
    /// - [`LoadDirectError::RequestedSubAsset`]: the path carries a `#label`; a sub-asset is only
    ///   produced by the loader of its base asset, so it cannot be loaded on its own.
    /// - [`LoadDirectError::UnapprovedPath`]: the path escapes its source root and neither the
    ///   server's [`UnapprovedPathMode`](crate::server::UnapprovedPathMode) nor
    ///   [`override_unapproved`](Self::override_unapproved) allows it.
    /// - [`LoadDirectError::AssetTypeMismatch`]: the asset loaded, but it is not of type `A`.
    ///
    /// [`load_erased_value`](Self::load_erased_value) and
    /// [`load_untyped_value`](Self::load_untyped_value) ask for no type of their own, so they can
    /// report every variant above except [`LoadDirectError::AssetTypeMismatch`]: there is nothing
    /// for them to compare the loaded type with. The three `*_from_reader` variants report the same
    /// variants as the method they mirror.
    pub async fn load_value<'a, A: Asset>(
        self,
        path: impl Into<AssetPath<'a>>,
    ) -> Result<LoadedAsset<A>, LoadDirectError> {
        self.load_typed_value_internal::<A>(path.into().into_owned(), None)
            .await
    }

    /// Loads the provided path as the given type, returning the loaded data.
    ///
    /// This load is async and therefore needs to be awaited before returning the loaded data.
    pub async fn load_erased_value<'a>(
        self,
        type_id: TypeId,
        path: impl Into<AssetPath<'a>>,
    ) -> Result<ErasedLoadedAsset, LoadDirectError> {
        self.load_value_internal(Some(type_id), None, &path.into().into_owned(), None)
            .await
    }

    /// Loads the provided path with an unknown type (which is guessed based on the path or meta
    /// file), returning the loaded data.
    ///
    /// This load is async and therefore needs to be awaited before returning the loaded data.
    pub async fn load_untyped_value<'a>(
        self,
        path: impl Into<AssetPath<'a>>,
    ) -> Result<ErasedLoadedAsset, LoadDirectError> {
        self.load_value_internal(None, None, &path.into().into_owned(), None)
            .await
    }

    /// Loads the given type from the given `reader`, returning the loaded data.
    ///
    /// This load is async and therefore needs to be awaited before returning the loaded data.
    ///
    /// The provided path determines the path used for handles of subassets, as well as any
    /// relative paths of assets used by the nested loader.
    ///
    /// The meta is created through [`ErasedAssetLoader::default_meta`] instead of being read and
    /// deserialized from the meta file.
    ///
    /// [`ErasedAssetLoader::default_meta`]: crate::loader::ErasedAssetLoader::default_meta
    pub async fn load_value_from_reader<'a, A: Asset>(
        self,
        path: impl Into<AssetPath<'a>>,
        reader: &'b mut dyn Reader,
    ) -> Result<LoadedAsset<A>, LoadDirectError> {
        self.load_typed_value_internal::<A>(path.into().into_owned(), Some(reader))
            .await
    }

    /// Loads the given type from the given `reader`, returning the loaded data.
    ///
    /// This load is async and therefore needs to be awaited before returning the loaded data.
    ///
    /// The provided path determines the path used for handles of subassets, as well as any
    /// relative paths of assets used by the nested loader.
    ///
    /// The meta is created through [`ErasedAssetLoader::default_meta`] instead of being read and
    /// deserialized from the meta file.
    ///
    /// [`ErasedAssetLoader::default_meta`]: crate::loader::ErasedAssetLoader::default_meta
    pub async fn load_erased_value_from_reader<'a>(
        self,
        type_id: TypeId,
        path: impl Into<AssetPath<'a>>,
        reader: &'b mut dyn Reader,
    ) -> Result<ErasedLoadedAsset, LoadDirectError> {
        self.load_value_internal(Some(type_id), None, &path.into().into_owned(), Some(reader))
            .await
    }

    /// Loads an asset from the given `reader` with an unknown type (which is guessed based on the
    /// path or meta file), returning the loaded data.
    ///
    /// This load is async and therefore needs to be awaited before returning the loaded data.
    ///
    /// The provided path determines the path used for handles of subassets, as well as any
    /// relative paths of assets used by the nested loader.
    ///
    /// The meta is created through [`ErasedAssetLoader::default_meta`] instead of being read and
    /// deserialized from the meta file.
    ///
    /// [`ErasedAssetLoader::default_meta`]: crate::loader::ErasedAssetLoader::default_meta
    pub async fn load_untyped_value_from_reader<'a>(
        self,
        path: impl Into<AssetPath<'a>>,
        reader: &'b mut dyn Reader,
    ) -> Result<ErasedLoadedAsset, LoadDirectError> {
        self.load_value_internal(None, None, &path.into().into_owned(), Some(reader))
            .await
    }
}

// -----------------------------------------------------------------------------

impl<'a> LoadContext<'a> {
    /// Starts building a nested load, which is what the rest of the loading API hangs off.
    ///
    /// [`load`] is the shortcut for the common case — request the asset and get its handle. The
    /// builder is for everything else:
    ///
    /// - [`load_erased`] and [`load_untyped`] request an asset whose type is not known at compile
    ///   time: the first returns an [`ErasedHandle`], the second a handle to a
    ///   [`LoadedUntypedAsset`] that carries the real one;
    /// - [`load_value`] (and its erased, untyped and `*_from_reader` variants) *awaits* the load and
    ///   hands back the value. It allocates no handle, so it fails with a [`LoadDirectError`] where
    ///   the others fail through the handle's load state, and the file it reads is recorded as a
    ///   *loader* dependency (what hot reloading follows) instead of as a dependency handle;
    /// - [`override_unapproved`] allows a path that escapes its source root.
    ///
    /// Every handle the requesting methods return is recorded as a dependency of this asset, which
    /// is what makes the server report it as fully loaded only once those assets are: see
    /// [`LoadContext`]'s "Dependencies" section.
    ///
    /// [`load`]: Self::load
    /// [`load_erased`]: NestedLoadBuilder::load_erased
    /// [`load_untyped`]: NestedLoadBuilder::load_untyped
    /// [`load_value`]: NestedLoadBuilder::load_value
    /// [`override_unapproved`]: NestedLoadBuilder::override_unapproved
    /// [`ErasedHandle`]: crate::handle::ErasedHandle
    /// [`LoadedUntypedAsset`]: crate::loaded::LoadedUntypedAsset
    /// [`LoadDirectError`]: crate::error::LoadDirectError
    #[inline]
    pub fn load_builder(&mut self) -> NestedLoadBuilder<'a, '_> {
        NestedLoadBuilder::new(self)
    }

    /// Requests the asset of type `A` at `path` and records it as a dependency.
    ///
    /// The load is only *requested*: this returns as soon as the handle exists, and the asset is
    /// reported as fully loaded once everything it depends on has finished. When this context
    /// was created with `load_dependencies == false` (asset processing), only the handle is
    /// created and no load is started.
    ///
    /// An empty path is refused with a default handle, and so is a path the server refuses (an
    /// unapproved one, in either [`Deny`] or [`Forbid`] mode); nothing is recorded as a dependency
    /// in either case.
    ///
    /// [`Deny`]: crate::server::UnapprovedPathMode::Deny
    /// [`Forbid`]: crate::server::UnapprovedPathMode::Forbid
    #[must_use = "not using the returned handle may cause the asset to be released"]
    pub fn load<'b, A: Asset>(&mut self, path: impl Into<AssetPath<'b>>) -> Handle<A> {
        self.load_builder().load(path)
    }
}

// -----------------------------------------------------------------------------
