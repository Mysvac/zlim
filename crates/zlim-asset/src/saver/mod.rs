//! The saver interface, the assets handed to it, and the registry that picks one.
//!
//! [`AssetSaver`] writes a runtime [`Asset`] back to bytes and reports the settings those bytes are
//! read back with, reaching the asset — value and labeled sub-assets — and the rest of the asset
//! system through a [`SaverContext`]. [`ErasedAssetSaver`] is its type-erased mirror, which is how the
//! crate-internal registry stores savers and how the save path hands over an [`ErasedSavedAsset`]
//! without knowing the type.
//!
//! [`Asset`]: crate::asset::Asset

mod context;
mod saved;
mod saver;
mod savers;

pub use context::SaverContext;
pub use saved::{ErasedSavedAsset, SavedAsset};
pub use saver::{AssetSaver, ErasedAssetSaver};
pub(crate) use savers::AssetSavers;
