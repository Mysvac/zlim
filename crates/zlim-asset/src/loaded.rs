use core::any::Any;
use core::any::TypeId;

use atomicow::CowArc;
use zlim_core::world::World;
use zlim_path::TypePath;
use zlim_utils::hash::{HashMap, HashSet};

use crate::asset::Asset;
use crate::assets::Assets;
use crate::handle::ErasedHandle;
use crate::ident::{AssetIndex, ErasedAssetId, TypedAssetIndex};
use crate::meta::AssetHash;
use crate::path::AssetPath;

// -----------------------------------------------------------------------------
// LoadedFolder

/// A "loaded folder" containing handles for all assets stored in a given [`AssetPath`].
#[derive(Asset, TypePath)]
#[type_path = "zlim_asset::loaded::LoadedFolder"]
pub struct LoadedFolder {
    /// The handles of all assets stored in the folder.
    #[asset(dependency)]
    pub handles: Vec<ErasedHandle>,
}

// -----------------------------------------------------------------------------
// LoadedUntypedAsset

/// A "loaded asset" containing the handle of the asset that was loaded without knowing its type.
#[derive(Asset, TypePath)]
#[type_path = "zlim_asset::loaded::LoadedUntypedAsset"]
pub struct LoadedUntypedAsset {
    /// The handle of the loaded asset, typed only at runtime.
    #[asset(dependency)]
    pub handle: ErasedHandle,
}

// -----------------------------------------------------------------------------
// AssetContainer

/// A loaded asset value that can be handed back to the storage it belongs to.
///
/// It is the object-safe bridge [`ErasedLoadedAsset`] holds a concrete asset through: the blanket
/// implementation over every [`Asset`] is what lets an [`ErasedLoadedAsset`] hand the value out
/// again by type (`get` / `with_type`), and what lets a finished asset be put back into the world
/// under the index it was loaded for.
pub(crate) trait AssetContainer: Any + Send + Sync + 'static {
    /// Moves the asset into the [`Assets`] storage of `world`, under `index`.
    fn apply_asset(self: Box<Self>, index: AssetIndex, world: &mut World);

    /// Returns the fully-qualified type path of the concrete asset value.
    fn asset_type_path(&self) -> &'static str;
}

impl<A: Asset> AssetContainer for A {
    fn apply_asset(self: Box<Self>, index: AssetIndex, world: &mut World) {
        world
            .resource_mut::<Assets<A>>()
            .insert(index, *self)
            .expect("the AssetIndex is still valid");
    }

    fn asset_type_path(&self) -> &'static str {
        <A as TypePath>::type_path()
    }
}

// -----------------------------------------------------------------------------

/// A sub-asset together with the handle it is registered under.
///
/// It is what the two label indexes of a loaded asset point at: by label
/// (`label_to_label_index`) and by the handle's id (`asset_to_label_index`).
pub(crate) struct LabeledAsset {
    /// The sub-asset itself, type-erased.
    pub(crate) asset: ErasedLoadedAsset,
    /// The handle the sub-asset is reachable by, which also carries its id.
    pub(crate) handle: ErasedHandle,
}

/// A type-erased loaded asset produced by an [`AssetLoader`].
///
/// Use [`with_type`] or the typed accessors [`get`] / [`get_mut`] to recover the concrete asset.
///
/// [`AssetLoader`]: crate::loader::AssetLoader
/// [`with_type`]: ErasedLoadedAsset::with_type
/// [`get`]: ErasedLoadedAsset::get
/// [`get_mut`]: ErasedLoadedAsset::get_mut
pub struct ErasedLoadedAsset {
    pub(crate) value: Box<dyn AssetContainer>,
    pub(crate) dependencies: HashSet<TypedAssetIndex>,
    pub(crate) labeled_assets: Vec<LabeledAsset>,
    pub(crate) label_to_label_index: HashMap<CowArc<'static, str>, usize>,
    pub(crate) asset_to_label_index: HashMap<ErasedAssetId, usize>,
    pub(crate) loader_dependencies: HashMap<AssetPath<'static>, AssetHash>,
}

/// A typed loaded asset together with its labeled sub-assets and dependency sets.
///
/// Returned by [`AssetLoader::load`] through [`LoadContext`].
///
/// [`LoadContext`]: crate::loader::LoadContext
/// [`AssetLoader::load`]: crate::loader::AssetLoader::load
pub struct LoadedAsset<A: Asset> {
    pub(crate) value: A,
    pub(crate) dependencies: HashSet<TypedAssetIndex>,
    pub(crate) labeled_assets: Vec<LabeledAsset>,
    pub(crate) label_to_label_index: HashMap<CowArc<'static, str>, usize>,
    pub(crate) asset_to_label_index: HashMap<ErasedAssetId, usize>,
    pub(crate) loader_dependencies: HashMap<AssetPath<'static>, AssetHash>,
}

// -----------------------------------------------------------------------------

impl<A: Asset> LoadedAsset<A> {
    /// Constructs a [`LoadedAsset`] from `value` with dependencies.
    ///
    /// The dependencies are the handles `value` refers to (`Asset::visit_dependencies`). A handle
    /// that names a UUID is not recorded, since a UUID asset cannot be loaded anyway.
    ///
    /// If no dependency is required, use [`LoadedAsset::independent`] instead.
    #[inline]
    #[doc(alias = "with_dependencies")]
    #[doc(alias = "new_with_dependencies")]
    pub fn new(value: A) -> Self {
        let mut dependencies = HashSet::<TypedAssetIndex>::new();

        value.visit_dependencies(&mut |id| {
            // UUID assets can't be loaded anyway, so just ignore this ID.
            if let ErasedAssetId::Index { type_id, index } = id {
                dependencies.insert(TypedAssetIndex { type_id, index });
            }
        });

        LoadedAsset {
            value,
            dependencies,
            labeled_assets: Vec::new(),
            label_to_label_index: HashMap::new(),
            asset_to_label_index: HashMap::new(),
            loader_dependencies: HashMap::new(),
        }
    }

    /// Constructs a [`LoadedAsset`] from `value` without dependencies.
    #[inline]
    #[doc(alias = "without_dependencies")]
    #[doc(alias = "new_without_dependencies")]
    pub fn independent(value: A) -> Self {
        LoadedAsset {
            value,
            dependencies: HashSet::new(),
            labeled_assets: Vec::new(),
            label_to_label_index: HashMap::new(),
            asset_to_label_index: HashMap::new(),
            loader_dependencies: HashMap::new(),
        }
    }

    /// Consumes the [`LoadedAsset`] and returns the inner value.
    #[inline]
    pub fn take(self) -> A {
        self.value
    }

    /// Returns a reference to the inner asset value.
    #[inline]
    pub fn get(&self) -> &A {
        &self.value
    }

    /// Returns an iterator over all sub-asset labels.
    #[inline]
    pub fn iter_labels(&self) -> impl ExactSizeIterator<Item = &str> {
        self.label_to_label_index.keys().map(|s| &**s)
    }

    /// Returns the erased sub-asset with the given `label`, or [`None`] if not found.
    #[inline]
    #[doc(alias = "get_labeled")]
    pub fn labeled(&self, label: impl AsRef<str>) -> Option<&ErasedLoadedAsset> {
        let index = self.label_to_label_index.get(label.as_ref())?;
        Some(&self.labeled_assets[*index].asset)
    }

    /// Returns the erased sub-asset identified by `id`, or [`None`] if not found.
    #[doc(alias = "get_labeled_by_id")]
    #[inline]
    pub fn labeled_by_id(&self, id: impl Into<ErasedAssetId>) -> Option<&ErasedLoadedAsset> {
        let index = self.asset_to_label_index.get(&id.into())?;
        Some(&self.labeled_assets[*index].asset)
    }

    /// Converts this typed [`LoadedAsset`] into a type-erased [`ErasedLoadedAsset`].
    #[inline]
    pub fn erased(self) -> ErasedLoadedAsset {
        ErasedLoadedAsset {
            value: Box::new(self.value),
            dependencies: self.dependencies,
            labeled_assets: self.labeled_assets,
            label_to_label_index: self.label_to_label_index,
            asset_to_label_index: self.asset_to_label_index,
            loader_dependencies: self.loader_dependencies,
        }
    }
}

impl<A: Asset> From<A> for LoadedAsset<A> {
    fn from(asset: A) -> Self {
        LoadedAsset::new(asset)
    }
}

impl<A: Asset> From<LoadedAsset<A>> for ErasedLoadedAsset {
    #[inline]
    fn from(asset: LoadedAsset<A>) -> Self {
        asset.erased()
    }
}

// -----------------------------------------------------------------------------

impl ErasedLoadedAsset {
    /// Consumes the asset and downcasts it to `A`, returning [`None`] if the type doesn't match.
    pub fn take<A: Asset>(self) -> Option<A> {
        <Box<dyn Any>>::downcast::<A>(self.value).map(|a| *a).ok()
    }

    /// Returns a reference to the asset downcast to `A`, or [`None`] if the type doesn't match.
    pub fn get<A: Asset>(&self) -> Option<&A> {
        // `&*self.value` (not `&self.value`): the downcast has to see the concrete asset type,
        // not the `Box<dyn AssetContainer>` that holds it.
        <dyn Any>::downcast_ref::<A>(&*self.value)
    }

    /// Returns a mutable reference to the asset downcast to `A`, or [`None`] if the type doesn't match.
    pub fn get_mut<A: Asset>(&mut self) -> Option<&mut A> {
        <dyn Any>::downcast_mut::<A>(&mut *self.value)
    }

    /// Returns the [`TypeId`] of the concrete asset.
    pub fn asset_type_id(&self) -> TypeId {
        self.value.as_ref().type_id()
    }

    /// Returns the fully qualified path for underlying asset type.
    pub fn asset_type_path(&self) -> &'static str {
        self.value.asset_type_path()
    }

    /// Returns the erased sub-asset with the given `label`, or [`None`] if not found.
    #[doc(alias = "get_labeled")]
    pub fn labeled(&self, label: impl AsRef<str>) -> Option<&ErasedLoadedAsset> {
        let index = self.label_to_label_index.get(label.as_ref())?;
        Some(&self.labeled_assets[*index].asset)
    }

    /// Returns the erased sub-asset identified by `id`, or [`None`] if not found.
    #[doc(alias = "get_labeled_by_id")]
    pub fn labeled_by_id(&self, id: impl Into<ErasedAssetId>) -> Option<&ErasedLoadedAsset> {
        let index = self.asset_to_label_index.get(&id.into())?;
        Some(&self.labeled_assets[*index].asset)
    }

    /// Returns an iterator over all sub-asset labels.
    pub fn iter_labels(&self) -> impl ExactSizeIterator<Item = &str> {
        self.label_to_label_index.keys().map(|s| &**s)
    }

    /// Downcast into a typed [`LoadedAsset<A>`].
    ///
    /// # Panics
    ///
    /// Panics when the erased asset is not an `A`.
    #[inline(always)]
    pub fn with_type<A: Asset>(self) -> LoadedAsset<A> {
        #[cold]
        #[inline(never)]
        fn mismatched(name: &str) -> ! {
            panic!("Failed to downcast a ErasedLoadedAsset to LoadedAsset<{name}>")
        }

        if <dyn Any>::is::<A>(&*self.value) {
            LoadedAsset {
                #[expect(unsafe_code, reason = "already checked")]
                value: unsafe { *<Box<dyn Any>>::downcast::<A>(self.value).unwrap_unchecked() },
                dependencies: self.dependencies,
                loader_dependencies: self.loader_dependencies,
                labeled_assets: self.labeled_assets,
                label_to_label_index: self.label_to_label_index,
                asset_to_label_index: self.asset_to_label_index,
            }
        } else {
            mismatched(::core::any::type_name::<A>())
        }
    }

    /// Attempts to downcast into a typed [`LoadedAsset<A>`].
    ///
    /// Returns `Err(self)` (the original erased asset) if the type does not match.
    #[inline]
    #[doc(alias = "downcast")]
    // The error type only reaches the size this lint complains about on 64-bit targets; on a 32-bit
    // one (wasm32) clippy stays quiet, and an unfulfilled `expect` would itself be an error.
    #[expect(clippy::allow_attributes, reason = "wasm32 clippy stays quiet")]
    #[allow(clippy::result_large_err, reason = "Err(self) is not a error")]
    pub fn try_with_type<A: Asset>(self) -> Result<LoadedAsset<A>, ErasedLoadedAsset> {
        if <dyn Any>::is::<A>(&*self.value) {
            Ok(LoadedAsset {
                #[expect(unsafe_code, reason = "already checked")]
                value: unsafe { *<Box<dyn Any>>::downcast::<A>(self.value).unwrap_unchecked() },
                dependencies: self.dependencies,
                loader_dependencies: self.loader_dependencies,
                labeled_assets: self.labeled_assets,
                label_to_label_index: self.label_to_label_index,
                asset_to_label_index: self.asset_to_label_index,
            })
        } else {
            Err(self)
        }
    }
}

// -----------------------------------------------------------------------------
