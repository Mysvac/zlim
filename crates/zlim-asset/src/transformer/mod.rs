//! The transformer interface and the views it reads assets through.
//!
//! An [`AssetTransformer`] turns one [`Asset`] type into another while the asset is being imported.
//! [`TransformedAsset`] is the owned value it is handed, and [`TransformedAssetRef`] /
//! [`TransformedAssetMut`] are the views of that value and its labeled sub-assets.
//! [`IdentityTransformer`] is the no-op implementation for a pipeline that only changes the format,
//! and [`ErasedAssetTransformer`] is the type-erased mirror of the interface.
//!
//! [`Asset`]: crate::asset::Asset

mod transformed;
mod transformer;

pub use transformed::{TransformedAsset, TransformedAssetMut, TransformedAssetRef};
pub use transformer::{AssetTransformer, ErasedAssetTransformer, IdentityTransformer};
