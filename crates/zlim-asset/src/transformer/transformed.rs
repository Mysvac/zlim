use core::any::Any;
use core::ops::{Deref, DerefMut};

use atomicow::CowArc;
use zlim_utils::hash::{HashMap, HashSet};

use crate::asset::Asset;
use crate::handle::{ErasedHandle, Handle};
use crate::ident::{AssetId, ErasedAssetId};
use crate::loaded::{ErasedLoadedAsset, LabeledAsset, LoadedAsset};

// -----------------------------------------------------------------------------
// TransformedAsset

/// An [`Asset`] (and any sub-assets) intended to be transformed.
///
/// Except for the whole-asset replacements such as [`Self::replace_asset`] and
/// [`Self::replace_labeled_assets`], most of the methods are
/// encapsulated in [`TransformedAssetRef`] and [`TransformedAssetMut`].
///
/// Please use them for sub-asset operations (through [`as_ref`] and [`as_mut`]).
///
/// [`as_ref`]: Self::as_ref
/// [`as_mut`]: Self::as_mut
pub struct TransformedAsset<A: Asset> {
    pub(crate) value: A,
    pub(crate) labeled_assets: Vec<LabeledAsset>,
    pub(crate) label_to_label_index: HashMap<CowArc<'static, str>, usize>,
    pub(crate) asset_to_label_index: HashMap<ErasedAssetId, usize>,
}

impl<A: Asset> Deref for TransformedAsset<A> {
    type Target = A;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<A: Asset> DerefMut for TransformedAsset<A> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

// -----------------------------------------------------------------------------
// methods

impl<A: Asset> TransformedAsset<A> {
    /// Creates a [`TransformedAsset`] from an [`Asset`].
    #[inline]
    #[doc(alias = "from_asset")]
    pub fn new(asset: A) -> Self {
        TransformedAsset {
            value: asset,
            labeled_assets: Vec::new(),
            label_to_label_index: HashMap::new(),
            asset_to_label_index: HashMap::new(),
        }
    }

    /// Creates a [`TransformedAsset`] from an [`LoadedAsset`].
    ///
    /// For a raw asset, use [`TransformedAsset::new`] instead.
    ///
    /// For [`ErasedLoadedAsset`], use [`with_type`]
    /// to convert to [`LoadedAsset`].
    ///
    /// [`with_type`]: crate::loaded::ErasedLoadedAsset::with_type
    #[inline]
    pub fn from_loaded(asset: LoadedAsset<A>) -> Self {
        TransformedAsset {
            value: asset.value,
            labeled_assets: asset.labeled_assets,
            label_to_label_index: asset.label_to_label_index,
            asset_to_label_index: asset.asset_to_label_index,
        }
    }

    /// Converts self to [`LoadedAsset`].
    ///
    /// The returned asset starts with empty dependency sets: a [`TransformedAsset`] does not carry
    /// the load dependencies of its source, so there is nothing to move into them.
    #[inline]
    pub fn finish(self) -> LoadedAsset<A> {
        LoadedAsset {
            value: self.value,
            dependencies: HashSet::new(),
            labeled_assets: self.labeled_assets,
            label_to_label_index: self.label_to_label_index,
            asset_to_label_index: self.asset_to_label_index,
            loader_dependencies: HashMap::new(),
        }
    }

    /// Returns a reference to the asset value.
    #[inline]
    pub fn get(&self) -> &A {
        &self.value
    }

    /// Returns a mutable reference to the asset value.
    #[inline]
    pub fn get_mut(&mut self) -> &mut A {
        &mut self.value
    }

    /// Returns a [`TransformedAssetRef`] of this asset value.
    ///
    /// That can be used to access sub-assets.
    #[inline]
    pub fn as_ref(&self) -> TransformedAssetRef<'_, A> {
        TransformedAssetRef {
            value: &self.value,
            labeled_assets: &self.labeled_assets,
            label_to_label_index: &self.label_to_label_index,
            asset_to_label_index: &self.asset_to_label_index,
        }
    }

    /// Returns a [`TransformedAssetMut`] of this asset value.
    ///
    /// That can be used to access and modify sub-assets.
    #[inline]
    pub fn as_mut(&mut self) -> TransformedAssetMut<'_, A> {
        TransformedAssetMut {
            value: &mut self.value,
            labeled_assets: &mut self.labeled_assets,
            label_to_label_index: &mut self.label_to_label_index,
            asset_to_label_index: &mut self.asset_to_label_index,
        }
    }
}

impl<'a, A: Asset> From<&'a TransformedAsset<A>> for TransformedAssetRef<'a, A> {
    #[inline]
    fn from(value: &'a TransformedAsset<A>) -> Self {
        value.as_ref()
    }
}

impl<'a, A: Asset> From<&'a mut TransformedAsset<A>> for TransformedAssetRef<'a, A> {
    #[inline]
    fn from(value: &'a mut TransformedAsset<A>) -> Self {
        value.as_ref()
    }
}

impl<'a, A: Asset> From<&'a mut TransformedAsset<A>> for TransformedAssetMut<'a, A> {
    #[inline]
    fn from(value: &'a mut TransformedAsset<A>) -> Self {
        value.as_mut()
    }
}

// -----------------------------------------------------------------------------
// TransformedAssetRef

/// A shared reference of a [`TransformedAsset`].
pub struct TransformedAssetRef<'a, A: Asset> {
    value: &'a A,
    labeled_assets: &'a Vec<LabeledAsset>,
    label_to_label_index: &'a HashMap<CowArc<'static, str>, usize>,
    asset_to_label_index: &'a HashMap<ErasedAssetId, usize>,
}

/// A mutable view of a [`TransformedAsset`].
pub struct TransformedAssetMut<'a, A: Asset> {
    value: &'a mut A,
    labeled_assets: &'a mut Vec<LabeledAsset>,
    label_to_label_index: &'a mut HashMap<CowArc<'static, str>, usize>,
    asset_to_label_index: &'a mut HashMap<ErasedAssetId, usize>,
}

// -----------------------------------------------------------------------------

impl<'a, A: Asset> From<TransformedAssetMut<'a, A>> for TransformedAssetRef<'a, A> {
    #[inline]
    fn from(value: TransformedAssetMut<'a, A>) -> Self {
        Self {
            value: value.value,
            labeled_assets: value.labeled_assets,
            label_to_label_index: value.label_to_label_index,
            asset_to_label_index: value.asset_to_label_index,
        }
    }
}

impl<'a, A: Asset> TransformedAssetMut<'a, A> {
    /// Reborrow self as [`TransformedAssetRef`] with smaller lifetime.
    #[inline]
    pub fn as_ref(&self) -> TransformedAssetRef<'_, A> {
        TransformedAssetRef {
            value: self.value,
            labeled_assets: self.labeled_assets,
            label_to_label_index: self.label_to_label_index,
            asset_to_label_index: self.asset_to_label_index,
        }
    }

    /// Reborrow self as [`TransformedAssetMut`] with smaller lifetime.
    #[inline]
    pub fn as_mut(&mut self) -> TransformedAssetMut<'_, A> {
        TransformedAssetMut {
            value: self.value,
            labeled_assets: self.labeled_assets,
            label_to_label_index: self.label_to_label_index,
            asset_to_label_index: self.asset_to_label_index,
        }
    }
}

impl<'a, A: Asset> TransformedAssetRef<'a, A> {
    /// Reborrow self with *same* lifetime.
    #[inline]
    pub fn as_ref(&self) -> TransformedAssetRef<'a, A> {
        TransformedAssetRef {
            value: self.value,
            labeled_assets: self.labeled_assets,
            label_to_label_index: self.label_to_label_index,
            asset_to_label_index: self.asset_to_label_index,
        }
    }
}

// -----------------------------------------------------------------------------
// deref

impl<A: Asset> Deref for TransformedAssetRef<'_, A> {
    type Target = A;
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<A: Asset> Deref for TransformedAssetMut<'_, A> {
    type Target = A;
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<A: Asset> DerefMut for TransformedAssetMut<'_, A> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value
    }
}

impl<'a, A: Asset> TransformedAssetRef<'a, A> {
    /// Returns a reference to the value.
    #[inline]
    pub fn get(&self) -> &'a A {
        self.value
    }
}

impl<'a, A: Asset> TransformedAssetMut<'a, A> {
    /// Returns a reference to the value.
    #[inline]
    pub fn get(&self) -> &A {
        self.value
    }

    /// Returns a mutable reference to the value.
    #[inline]
    pub fn get_mut(&mut self) -> &mut A {
        self.value
    }
}

// -----------------------------------------------------------------------------
// builder

impl<'a, A: Asset> TransformedAssetRef<'a, A> {
    /// Creates a [`TransformedAssetRef`] from a [`LoadedAsset`].
    #[inline]
    #[doc(alias = "from_loaded")]
    pub fn new(asset: &'a LoadedAsset<A>) -> Self {
        TransformedAssetRef {
            value: &asset.value,
            labeled_assets: &asset.labeled_assets,
            label_to_label_index: &asset.label_to_label_index,
            asset_to_label_index: &asset.asset_to_label_index,
        }
    }

    /// Tries to create a [`TransformedAssetRef`] from an [`ErasedLoadedAsset`], returning [`None`]
    /// when it does not hold an `A`.
    #[inline]
    pub fn from_erased(asset: &'a ErasedLoadedAsset) -> Option<Self> {
        let value = <dyn Any>::downcast_ref::<A>(&*asset.value)?;
        Some(TransformedAssetRef {
            value,
            labeled_assets: &asset.labeled_assets,
            label_to_label_index: &asset.label_to_label_index,
            asset_to_label_index: &asset.asset_to_label_index,
        })
    }
}

impl<'a, A: Asset> TransformedAssetMut<'a, A> {
    /// Creates a [`TransformedAssetMut`] from a [`LoadedAsset`].
    #[inline]
    #[doc(alias = "from_loaded")]
    pub fn new(asset: &'a mut LoadedAsset<A>) -> Self {
        TransformedAssetMut {
            value: &mut asset.value,
            labeled_assets: &mut asset.labeled_assets,
            label_to_label_index: &mut asset.label_to_label_index,
            asset_to_label_index: &mut asset.asset_to_label_index,
        }
    }

    /// Tries to create a [`TransformedAssetMut`] from an [`ErasedLoadedAsset`], returning [`None`]
    /// when it does not hold an `A`.
    #[inline]
    pub fn from_erased(asset: &'a mut ErasedLoadedAsset) -> Option<Self> {
        let value = <dyn Any>::downcast_mut::<A>(&mut *asset.value)?;
        Some(TransformedAssetMut {
            value,
            labeled_assets: &mut asset.labeled_assets,
            label_to_label_index: &mut asset.label_to_label_index,
            asset_to_label_index: &mut asset.asset_to_label_index,
        })
    }
}

// -----------------------------------------------------------------------------
// handle

impl<'a, A: Asset> TransformedAssetRef<'a, A> {
    /// Returns the typed [`Handle<B>`] of the nested labeled asset with the given `label`.
    #[doc(alias = "get_labeled_handle")]
    pub fn labeled_handle<B: Asset>(&self, label: impl AsRef<str>) -> Option<Handle<B>> {
        let index = *self.label_to_label_index.get(label.as_ref())?;
        Handle::<B>::try_from(self.labeled_assets[index].handle.clone()).ok()
    }

    /// Returns the [`ErasedHandle`] of the nested labeled asset with the given `label`.
    #[doc(alias = "get_erased_labeled_handle")]
    pub fn erased_labeled_handle(&self, label: impl AsRef<str>) -> Option<ErasedHandle> {
        let index = *self.label_to_label_index.get(label.as_ref())?;
        Some(self.labeled_assets[index].handle.clone())
    }
}

impl<'a, A: Asset> TransformedAssetMut<'a, A> {
    /// Returns the typed [`Handle<B>`] of the nested labeled asset with the given `label`.
    #[doc(alias = "get_labeled_handle")]
    pub fn labeled_handle<B: Asset>(&self, label: impl AsRef<str>) -> Option<Handle<B>> {
        let index = *self.label_to_label_index.get(label.as_ref())?;
        Handle::<B>::try_from(self.labeled_assets[index].handle.clone()).ok()
    }

    /// Returns the [`ErasedHandle`] of the nested labeled asset with the given `label`.
    #[doc(alias = "get_erased_labeled_handle")]
    pub fn erased_labeled_handle(&self, label: impl AsRef<str>) -> Option<ErasedHandle> {
        let index = *self.label_to_label_index.get(label.as_ref())?;
        Some(self.labeled_assets[index].handle.clone())
    }
}

// -----------------------------------------------------------------------------
// labeled

impl<'a, A: Asset> TransformedAssetRef<'a, A> {
    /// Iterates over all labels of the nested labeled sub-assets.
    pub fn iter_labels(&self) -> impl ExactSizeIterator<Item = &str> {
        self.label_to_label_index.keys().map(|s| &**s)
    }

    /// Returns a reference of the nested labeled sub-asset with the given `label` downcast to `B`.
    #[doc(alias = "get_labeled")]
    pub fn labeled<B: Asset>(&self, label: impl AsRef<str>) -> Option<TransformedAssetRef<'a, B>> {
        let index = *self.label_to_label_index.get(label.as_ref())?;
        let inner = &self.labeled_assets[index].asset;
        let value = <dyn Any>::downcast_ref::<B>(&*inner.value)?;
        Some(TransformedAssetRef {
            value,
            labeled_assets: &inner.labeled_assets,
            label_to_label_index: &inner.label_to_label_index,
            asset_to_label_index: &inner.asset_to_label_index,
        })
    }

    /// Returns a type-erased reference to the nested labeled sub-asset with the given `label`.
    #[doc(alias = "get_erased_labeled")]
    pub fn erased_labeled(&self, label: impl AsRef<str>) -> Option<&'a ErasedLoadedAsset> {
        let index = *self.label_to_label_index.get(label.as_ref())?;
        Some(&self.labeled_assets[index].asset)
    }

    /// Returns a reference of the nested labeled sub-asset for the given asset `id` downcast to `B`.
    #[doc(alias = "get_labeled_by_id")]
    pub fn labeled_by_id<B: Asset>(&self, id: AssetId<B>) -> Option<TransformedAssetRef<'a, B>> {
        let erased_id: ErasedAssetId = id.into();
        let index = *self.asset_to_label_index.get(&erased_id)?;
        let inner = &self.labeled_assets[index].asset;
        let value = <dyn Any>::downcast_ref::<B>(&*inner.value)?;
        Some(TransformedAssetRef {
            value,
            labeled_assets: &inner.labeled_assets,
            label_to_label_index: &inner.label_to_label_index,
            asset_to_label_index: &inner.asset_to_label_index,
        })
    }

    /// Returns a type-erased reference to the nested labeled sub-asset for the given asset `id`.
    #[doc(alias = "get_erased_labeled_by_id")]
    pub fn erased_labeled_by_id(&self, id: ErasedAssetId) -> Option<&'a ErasedLoadedAsset> {
        let index = *self.asset_to_label_index.get(&id)?;
        Some(&self.labeled_assets[index].asset)
    }
}

impl<'a, A: Asset> TransformedAssetMut<'a, A> {
    /// Iterates over all labels of the nested labeled sub-assets.
    pub fn iter_labels(&self) -> impl ExactSizeIterator<Item = &str> {
        self.label_to_label_index.keys().map(|s| &**s)
    }

    /// Returns a reference of the nested labeled sub-asset with the given `label` downcast to `B`.
    #[doc(alias = "get_labeled")]
    pub fn labeled<'s, B: Asset>(
        &'s mut self,
        label: impl AsRef<str>,
    ) -> Option<TransformedAssetMut<'s, B>> {
        let index = *self.label_to_label_index.get(label.as_ref())?;
        let inner = &mut self.labeled_assets[index].asset;
        let value = <dyn Any>::downcast_mut::<B>(&mut *inner.value)?;
        Some(TransformedAssetMut {
            value,
            labeled_assets: &mut inner.labeled_assets,
            label_to_label_index: &mut inner.label_to_label_index,
            asset_to_label_index: &mut inner.asset_to_label_index,
        })
    }

    /// Returns a type-erased reference to the nested labeled sub-asset with the given `label`.
    #[doc(alias = "get_erased_labeled")]
    pub fn erased_labeled(&mut self, label: impl AsRef<str>) -> Option<&mut ErasedLoadedAsset> {
        let index = *self.label_to_label_index.get(label.as_ref())?;
        Some(&mut self.labeled_assets[index].asset)
    }

    /// Returns a reference of the nested labeled sub-asset for the given asset `id` downcast to `B`.
    #[doc(alias = "get_labeled_by_id")]
    pub fn labeled_by_id<B: Asset>(
        &mut self,
        id: AssetId<B>,
    ) -> Option<TransformedAssetMut<'_, B>> {
        let erased_id: ErasedAssetId = id.into();
        let index = *self.asset_to_label_index.get(&erased_id)?;
        let inner = &mut self.labeled_assets[index].asset;
        let value = <dyn Any>::downcast_mut::<B>(&mut *inner.value)?;
        Some(TransformedAssetMut {
            value,
            labeled_assets: &mut inner.labeled_assets,
            label_to_label_index: &mut inner.label_to_label_index,
            asset_to_label_index: &mut inner.asset_to_label_index,
        })
    }

    /// Returns a type-erased reference to the nested labeled sub-asset for the given asset `id`.
    #[doc(alias = "get_erased_labeled_by_id")]
    pub fn erased_labeled_by_id(&mut self, id: ErasedAssetId) -> Option<&'_ mut ErasedLoadedAsset> {
        let index = *self.asset_to_label_index.get(&id)?;
        Some(&mut self.labeled_assets[index].asset)
    }
}

// -----------------------------------------------------------------------------

impl<A: Asset> TransformedAsset<A> {
    /// Replaces the asset value with `asset`, transferring labeled sub-assets.
    #[inline]
    pub fn replace_asset<B: Asset>(self, asset: B) -> TransformedAsset<B> {
        TransformedAsset {
            value: asset,
            labeled_assets: self.labeled_assets,
            label_to_label_index: self.label_to_label_index,
            asset_to_label_index: self.asset_to_label_index,
        }
    }

    /// Replaces this asset's labeled sub-assets with those from `source`.
    #[inline]
    pub fn replace_labeled_assets<B: Asset>(&mut self, source: TransformedAsset<B>) {
        self.labeled_assets = source.labeled_assets;
        self.label_to_label_index = source.label_to_label_index;
        self.asset_to_label_index = source.asset_to_label_index;
    }

    /// Inserts or replaces a nested labeled sub-asset.
    ///
    /// `handle` and `asset` are expected to have the same asset type; a mismatch is asserted in
    /// debug builds.
    #[inline]
    #[doc(alias = "replace_labeled")]
    pub fn insert_labeled(
        &mut self,
        label: impl Into<CowArc<'static, str>>,
        handle: impl Into<ErasedHandle>,
        asset: impl Into<ErasedLoadedAsset>,
    ) {
        self.as_mut()
            .insert_internal(label.into(), handle.into(), asset.into());
    }
}

impl<'a, A: Asset> TransformedAssetMut<'a, A> {
    /// Replaces this asset's labeled sub-assets with those from `source`.
    #[inline]
    pub fn replace_labeled_assets<B: Asset>(&mut self, source: TransformedAsset<B>) {
        *self.labeled_assets = source.labeled_assets;
        *self.label_to_label_index = source.label_to_label_index;
        *self.asset_to_label_index = source.asset_to_label_index;
    }

    /// Inserts or replaces a nested labeled sub-asset.
    ///
    /// `handle` and `asset` are expected to have the same asset type; a mismatch is asserted in
    /// debug builds.
    #[inline]
    #[doc(alias = "replace_labeled")]
    pub fn insert_labeled(
        &mut self,
        label: impl Into<CowArc<'static, str>>,
        handle: impl Into<ErasedHandle>,
        asset: impl Into<ErasedLoadedAsset>,
    ) {
        self.insert_internal(label.into(), handle.into(), asset.into());
    }

    #[inline(never)]
    fn insert_internal(
        &mut self,
        label: CowArc<'static, str>,
        handle: ErasedHandle,
        asset: ErasedLoadedAsset,
    ) {
        use zlim_utils::hash::map::Entry;
        let labeled = LabeledAsset { asset, handle };

        debug_assert_eq!(
            labeled.asset.asset_type_id(),
            labeled.handle.type_id(),
            "LabeledAsset type mismatched, handle is `{:?}`, asset is `{:?}`-`{}`",
            labeled.handle.type_id(),
            labeled.asset.asset_type_id(),
            labeled.asset.asset_type_path(),
        );

        match self.label_to_label_index.entry(label) {
            Entry::Occupied(entry) => {
                let index = *entry.get();
                let new_id = labeled.handle.id();
                let old_id = self.labeled_assets[index].handle.id();
                if new_id != old_id {
                    self.asset_to_label_index.remove(&old_id);
                    self.asset_to_label_index.insert(new_id, index);
                }
                self.labeled_assets[index] = labeled;
            }
            Entry::Vacant(entry) => {
                let index = self.labeled_assets.len();
                entry.insert(index);
                self.asset_to_label_index.insert(labeled.handle.id(), index);
                self.labeled_assets.push(labeled);
            }
        }
    }
}

// -----------------------------------------------------------------------------
