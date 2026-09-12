#![expect(clippy::module_inception, reason = "For better structure.")]

use core::any::TypeId;

use serde::{Deserialize, Serialize};
use zlim_core::error::ZlimError;
use zlim_path::TypePath;

use crate::asset::Asset;
use crate::meta::Settings;

// -----------------------------------------------------------------------------
// AssetLoader

/// A file-format plugin that deserialises raw bytes into an [`Asset`].
pub trait AssetLoader: TypePath + Send + Sync + 'static {
    /// The asset type produced by this loader.
    type Asset: Asset;
    /// Per-asset configuration; stored in `.meta` files next to the asset.
    type Settings: Settings + Default + Serialize + for<'d> Deserialize<'d>;
    /// Error type returned when loading fails.
    type Error: Into<ZlimError>;

    /// File extensions handled by this loader (without leading `.`).
    ///
    /// Returns an empty slice by default, which means the loader must be selected
    /// explicitly (e.g. via a `.meta` file) rather than by extension matching.
    const EXTENSIONS: &[&'static str] = &[];

    // TODO! load(..)
}

// -----------------------------------------------------------------------------
// ErasedAssetLoader

pub trait ErasedAssetLoader: Send + Sync + 'static {
    fn type_id(&self) -> TypeId;

    fn type_path(&self) -> &'static str;

    fn asset_type_id(&self) -> TypeId;

    fn asset_type_path(&self) -> &'static str;

    fn extensions(&self) -> &[&str];

    // TODO! load(..)
    // TODO! default_meta(..)
    // TODO! deserialize_meta(..)
}

// -----------------------------------------------------------------------------
// Placeholder

impl AssetLoader for () {
    type Asset = ();
    type Settings = ();
    type Error = ZlimError;
    const EXTENSIONS: &[&'static str] = &[];
}
