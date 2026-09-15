//! The saver interface, the assets handed to it, and the registry that picks one.
//!
//! [`AssetSaver`] writes a runtime [`Asset`] back to bytes and reports the settings those bytes are
//! read back with, reading the asset — value and labeled sub-assets — through a [`SavedAsset`].
//! [`ErasedAssetSaver`] is its type-erased mirror, which is how the crate-internal registry stores
//! savers and how the save path hands over an [`ErasedSavedAsset`] without knowing the type.
//!
//! [`Asset`]: crate::asset::Asset

mod saved;
mod saver;
mod savers;

pub use saved::{ErasedSavedAsset, SavedAsset};
pub use saver::{AssetSaver, ErasedAssetSaver};
pub(crate) use savers::AssetSavers;
