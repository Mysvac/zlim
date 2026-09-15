use core::any::{Any, TypeId};
use core::ops::Deref;

use atomicow::CowArc;
use zlim_utils::hash::HashMap;

use crate::asset::Asset;
use crate::handle::{ErasedHandle, Handle};
use crate::ident::{AssetId, ErasedAssetId};
use crate::loaded::{ErasedLoadedAsset, LabeledAsset, LoadedAsset};

// -----------------------------------------------------------------------------
// SavedAsset & ErasedSavedAsset

/// An [`Asset`] (and its labeled sub-assets) ready to be saved.
///
/// Every part of it is borrowed: the asset value, the labeled sub-assets and the
/// two label indexes. A save that only knows a value (the [`AssetServer`] reads
/// one out of the world by its handle) therefore only hands the value over, with
/// the labeled parts empty.
///
/// A saver reads it twice — once to [`build_settings`] and once to [`save`] — which
/// is why both take it by value and the type is [`Clone`].
///
/// [`save`]: crate::saver::AssetSaver::save
/// [`build_settings`]: crate::saver::AssetSaver::build_settings
/// [`AssetServer`]: crate::server::AssetServer
pub struct SavedAsset<'a, A: Asset> {
    pub(super) value: &'a A,
    pub(super) labeled_assets: &'a [LabeledAsset],
    pub(super) label_to_label_index: &'a HashMap<CowArc<'static, str>, usize>,
    pub(super) asset_to_label_index: &'a HashMap<ErasedAssetId, usize>,
}

/// A type-erased [`SavedAsset`].
pub struct ErasedSavedAsset<'a> {
    pub(crate) value: &'a (dyn Any + Send + Sync),
    pub(crate) labeled_assets: &'a [LabeledAsset],
    pub(crate) label_to_label_index: &'a HashMap<CowArc<'static, str>, usize>,
    pub(crate) asset_to_label_index: &'a HashMap<ErasedAssetId, usize>,
}

// -----------------------------------------------------------------------------
// Basic

impl<A: Asset> Clone for SavedAsset<'_, A> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            value: self.value,
            labeled_assets: self.labeled_assets,
            label_to_label_index: self.label_to_label_index,
            asset_to_label_index: self.asset_to_label_index,
        }
    }
}

impl Clone for ErasedSavedAsset<'_> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            value: self.value,
            labeled_assets: self.labeled_assets,
            label_to_label_index: self.label_to_label_index,
            asset_to_label_index: self.asset_to_label_index,
        }
    }
}

impl<A: Asset> Deref for SavedAsset<'_, A> {
    type Target = A;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<'a, A: Asset> SavedAsset<'a, A> {
    /// Returns a reference to the asset value.
    #[inline]
    pub fn get(&self) -> &'a A {
        self.value
    }
}

impl<'a> ErasedSavedAsset<'a> {
    /// Returns a reference to the asset value.
    #[inline]
    pub fn get<A: Asset>(&self) -> Option<&'a A> {
        self.value.downcast_ref::<A>()
    }
}

impl<'a, A: Asset> From<SavedAsset<'a, A>> for ErasedSavedAsset<'a> {
    #[inline]
    fn from(value: SavedAsset<'a, A>) -> Self {
        value.erased()
    }
}

impl<'a, A: Asset> SavedAsset<'a, A> {
    /// Converts this typed asset into a type-erased [`ErasedSavedAsset`].
    #[inline]
    pub fn erased(self) -> ErasedSavedAsset<'a> {
        ErasedSavedAsset {
            value: self.value,
            labeled_assets: self.labeled_assets,
            label_to_label_index: self.label_to_label_index,
            asset_to_label_index: self.asset_to_label_index,
        }
    }
}

// -----------------------------------------------------------------------------
// Ctor

const EMPTY_LABELED_ASSETS: &[LabeledAsset] = &[];
const EMPTY_LABEL_MAP: &HashMap<CowArc<'static, str>, usize> = &HashMap::new();
const EMPTY_ASSET_MAP: &HashMap<ErasedAssetId, usize> = &HashMap::new();

impl<'a, A: Asset> SavedAsset<'a, A> {
    /// Creates a [`SavedAsset`] from an [`Asset`].
    #[doc(alias = "from_asset")]
    #[inline]
    pub fn new(asset: &'a A) -> Self {
        Self {
            value: asset,
            labeled_assets: EMPTY_LABELED_ASSETS,
            label_to_label_index: EMPTY_LABEL_MAP,
            asset_to_label_index: EMPTY_ASSET_MAP,
        }
    }

    /// Creates a [`SavedAsset`] from a [`LoadedAsset`].
    ///
    /// For a raw asset, use [`SavedAsset::new`] instead.
    ///
    /// For [`ErasedLoadedAsset`], use [`with_type`]
    /// to convert to [`LoadedAsset`].
    ///
    /// [`with_type`]: ErasedLoadedAsset::with_type
    #[inline]
    pub fn from_loaded(asset: &'a LoadedAsset<A>) -> Self {
        Self {
            value: &asset.value,
            labeled_assets: &asset.labeled_assets,
            label_to_label_index: &asset.label_to_label_index,
            asset_to_label_index: &asset.asset_to_label_index,
        }
    }

    /// Creates a [`SavedAsset`] from a [`TransformedAsset`].
    ///
    /// Like [`from_loaded`](Self::from_loaded) this is a view: the transformed asset keeps owning
    /// its value and labeled sub-assets, which is why only borrows of them are taken.
    ///
    /// [`TransformedAsset`]: crate::transformer::TransformedAsset
    #[inline]
    pub fn from_transformed(asset: &'a crate::transformer::TransformedAsset<A>) -> Self {
        Self {
            value: asset.get(),
            labeled_assets: &asset.labeled_assets,
            label_to_label_index: &asset.label_to_label_index,
            asset_to_label_index: &asset.asset_to_label_index,
        }
    }
}

impl<'a> ErasedSavedAsset<'a> {
    /// Creates an [`ErasedSavedAsset`] from a bare value.
    ///
    /// Used when the value is the only thing known about the asset — read out of the world by its
    /// handle, for instance. The labeled sub-assets and the two label indexes stay empty: they are
    /// part of the load result, which the server does not keep.
    #[inline]
    pub(crate) fn from_raw(value: &'a (dyn Any + Send + Sync)) -> Self {
        Self {
            value,
            labeled_assets: EMPTY_LABELED_ASSETS,
            label_to_label_index: EMPTY_LABEL_MAP,
            asset_to_label_index: EMPTY_ASSET_MAP,
        }
    }

    /// Creates an [`ErasedSavedAsset`] from an [`ErasedLoadedAsset`].
    #[inline]
    pub fn from_loaded(asset: &'a ErasedLoadedAsset) -> Self {
        Self {
            value: &asset.value,
            labeled_assets: &asset.labeled_assets,
            label_to_label_index: &asset.label_to_label_index,
            asset_to_label_index: &asset.asset_to_label_index,
        }
    }

    /// Returns the [`TypeId`] of the inner asset.
    #[inline]
    pub fn asset_type_id(&self) -> TypeId {
        self.value.type_id()
    }

    /// Returns the typed view of this asset, if it holds an `A` at all.
    ///
    /// Unlike [`with_type`](Self::with_type) this does not panic on a type mismatch, which is what
    /// a saver wants when it is handed an asset it may not know the type of.
    #[inline]
    #[doc(alias = "downcast")]
    #[doc(alias = "downcast_ref")]
    pub fn try_with_type<A: Asset>(&self) -> Option<SavedAsset<'a, A>> {
        Some(SavedAsset {
            value: self.value.downcast_ref::<A>()?,
            labeled_assets: self.labeled_assets,
            label_to_label_index: self.label_to_label_index,
            asset_to_label_index: self.asset_to_label_index,
        })
    }

    /// Converts an [`ErasedSavedAsset`] to a [`SavedAsset`].
    ///
    /// # Panics
    ///
    /// Panics when the erased value is not an `A`.
    #[inline]
    pub fn with_type<A: Asset>(&self) -> SavedAsset<'a, A> {
        let value = self.value.downcast_ref::<A>().unwrap_or_else(|| {
            let n = ::core::any::type_name::<A>();
            panic!("Failed to convert a ErasedSavedAsset to SavedAsset<{n}>")
        });
        SavedAsset {
            value,
            labeled_assets: self.labeled_assets,
            label_to_label_index: self.label_to_label_index,
            asset_to_label_index: self.asset_to_label_index,
        }
    }
}

// -----------------------------------------------------------------------------
// Labeled

impl<'a, A: Asset> SavedAsset<'a, A> {
    /// Iterates over all labels of the labeled sub-assets.
    #[inline]
    pub fn iter_labels(&self) -> impl ExactSizeIterator<Item = &'a str> {
        self.label_to_label_index.keys().map(|s| &**s)
    }

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

    /// Returns the labeled sub-asset with the given `label` downcast to `B`.
    #[doc(alias = "get_labeled")]
    pub fn labeled<B: Asset>(&self, label: impl AsRef<str>) -> Option<SavedAsset<'_, B>> {
        let index = self.label_to_label_index.get(label.as_ref())?;
        let loaded = &self.labeled_assets[*index].asset;
        Some(SavedAsset {
            value: loaded.get::<B>()?,
            labeled_assets: &loaded.labeled_assets,
            label_to_label_index: &loaded.label_to_label_index,
            asset_to_label_index: &loaded.asset_to_label_index,
        })
    }

    /// Returns the type-erased labeled sub-asset with the given `label`.
    #[doc(alias = "get_erased_labeled")]
    pub fn erased_labeled(&self, label: impl AsRef<str>) -> Option<ErasedSavedAsset<'_>> {
        let index = self.label_to_label_index.get(label.as_ref())?;
        let loaded = &self.labeled_assets[*index].asset;
        Some(ErasedSavedAsset {
            value: &loaded.value,
            labeled_assets: &loaded.labeled_assets,
            label_to_label_index: &loaded.label_to_label_index,
            asset_to_label_index: &loaded.asset_to_label_index,
        })
    }

    /// Returns a reference of the nested labeled sub-asset for the given asset `id` downcast to `B`.
    #[doc(alias = "get_labeled_by_id")]
    pub fn labeled_by_id<B: Asset>(&self, id: AssetId<B>) -> Option<SavedAsset<'_, B>> {
        let erased_id: ErasedAssetId = id.into();
        let index = *self.asset_to_label_index.get(&erased_id)?;
        let loaded = &self.labeled_assets[index].asset;
        Some(SavedAsset {
            value: loaded.get::<B>()?,
            labeled_assets: &loaded.labeled_assets,
            label_to_label_index: &loaded.label_to_label_index,
            asset_to_label_index: &loaded.asset_to_label_index,
        })
    }

    /// Returns a type-erased reference to the nested labeled sub-asset for the given asset `id`.
    #[doc(alias = "get_erased_labeled_by_id")]
    pub fn erased_labeled_by_id(&self, id: ErasedAssetId) -> Option<ErasedSavedAsset<'_>> {
        let index = self.asset_to_label_index.get(&id)?;
        let loaded = &self.labeled_assets[*index].asset;
        Some(ErasedSavedAsset {
            value: &loaded.value,
            labeled_assets: &loaded.labeled_assets,
            label_to_label_index: &loaded.label_to_label_index,
            asset_to_label_index: &loaded.asset_to_label_index,
        })
    }
}

impl<'a> ErasedSavedAsset<'a> {
    /// Iterates over all labels of the labeled sub-assets.
    #[inline]
    pub fn iter_labels(&self) -> impl ExactSizeIterator<Item = &'a str> {
        self.label_to_label_index.keys().map(|s| &**s)
    }

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

    /// Returns the labeled sub-asset with the given `label` downcast to `B`.
    #[doc(alias = "get_labeled")]
    pub fn labeled<B: Asset>(&self, label: impl AsRef<str>) -> Option<SavedAsset<'_, B>> {
        let index = self.label_to_label_index.get(label.as_ref())?;
        let loaded = &self.labeled_assets[*index].asset;
        Some(SavedAsset {
            value: loaded.get::<B>()?,
            labeled_assets: &loaded.labeled_assets,
            label_to_label_index: &loaded.label_to_label_index,
            asset_to_label_index: &loaded.asset_to_label_index,
        })
    }

    /// Returns the type-erased labeled sub-asset with the given `label`.
    #[doc(alias = "get_erased_labeled")]
    pub fn erased_labeled(&self, label: impl AsRef<str>) -> Option<ErasedSavedAsset<'_>> {
        let index = self.label_to_label_index.get(label.as_ref())?;
        let loaded = &self.labeled_assets[*index].asset;
        Some(ErasedSavedAsset {
            value: &loaded.value,
            labeled_assets: &loaded.labeled_assets,
            label_to_label_index: &loaded.label_to_label_index,
            asset_to_label_index: &loaded.asset_to_label_index,
        })
    }

    /// Returns a reference of the nested labeled sub-asset for the given asset `id` downcast to `B`.
    #[doc(alias = "get_labeled_by_id")]
    pub fn labeled_by_id<B: Asset>(&self, id: AssetId<B>) -> Option<SavedAsset<'_, B>> {
        let erased_id: ErasedAssetId = id.into();
        let index = *self.asset_to_label_index.get(&erased_id)?;
        let loaded = &self.labeled_assets[index].asset;
        Some(SavedAsset {
            value: loaded.get::<B>()?,
            labeled_assets: &loaded.labeled_assets,
            label_to_label_index: &loaded.label_to_label_index,
            asset_to_label_index: &loaded.asset_to_label_index,
        })
    }

    /// Returns a type-erased reference to the nested labeled sub-asset for the given asset `id`.
    #[doc(alias = "get_erased_labeled_by_id")]
    pub fn erased_labeled_by_id(&self, id: ErasedAssetId) -> Option<ErasedSavedAsset<'_>> {
        let index = self.asset_to_label_index.get(&id)?;
        let loaded = &self.labeled_assets[*index].asset;
        Some(ErasedSavedAsset {
            value: &loaded.value,
            labeled_assets: &loaded.labeled_assets,
            label_to_label_index: &loaded.label_to_label_index,
            asset_to_label_index: &loaded.asset_to_label_index,
        })
    }
}
