#![expect(clippy::module_inception, reason = "For better structure.")]

use core::any::TypeId;

use serde::{Deserialize, Serialize};
use zlim_core::error::ZlimError;
use zlim_path::TypePath;

use crate::asset::Asset;
use crate::meta::Settings;

// -----------------------------------------------------------------------------
// AssetLoader

pub trait AssetTransformer: TypePath + Send + Sync + 'static {
    /// The [`Asset`] type which this [`AssetTransformer`] inputs.
    type AssetInput: Asset;
    /// The [`Asset`] type which this [`AssetTransformer`] outputs.
    type AssetOutput: Asset;

    /// The settings type used by this [`AssetTransformer`].
    type Settings: Settings + Default + Serialize + for<'a> Deserialize<'a>;

    /// Error type returned when tramsform fails.
    type Error: Into<ZlimError>;

    // TODO! transform(..)
}

// -----------------------------------------------------------------------------
// ErasedAssetTransformer

/// A type-erased variant of [`ErasedAssetTransformer`].
pub trait ErasedAssetTransformer: Send + Sync + 'static {
    fn type_id(&self) -> TypeId;

    fn type_path(&self) -> &'static str;

    // TODO! transform(..)
}

// -----------------------------------------------------------------------------
// Placeholder

impl AssetTransformer for () {
    type AssetInput = ();
    type AssetOutput = ();
    type Settings = ();
    type Error = ZlimError;
}
