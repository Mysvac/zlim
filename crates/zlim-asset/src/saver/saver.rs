#![expect(clippy::module_inception, reason = "For better structure.")]

use core::any::TypeId;

use serde::{Deserialize, Serialize};
use zlim_core::error::ZlimError;
use zlim_path::TypePath;

use crate::asset::Asset;
use crate::meta::Settings;

// -----------------------------------------------------------------------------
// AssetLoader

pub trait AssetSaver: TypePath + Send + Sync + 'static {
    /// The top level [`Asset`] saved by this [`AssetSaver`].
    type Asset: Asset;

    /// The settings type used by this [`AssetSaver`].
    type Settings: Settings + Default + Serialize + for<'a> Deserialize<'a>;

    /// The settings type produced by this saver and stored in the saved **output**.
    ///
    /// These correspond to the `L` type parameter of [`AssetMeta<L, P>`] — the
    /// settings the loader needs when reading the final asset back.
    ///
    /// [`AssetMeta<L, P>`]: crate::meta::AssetMeta
    type LoaderSettings: Settings + Default + Serialize + for<'a> Deserialize<'a>;

    /// Error type returned when saving fails.
    type Error: Into<ZlimError>;

    // TODO! save(..)
}

// -----------------------------------------------------------------------------
// ErasedAssetSaver

/// A type-erased variant of [`AssetSaver`].
pub trait ErasedAssetSaver: Send + Sync + 'static {
    fn type_id(&self) -> TypeId;

    fn type_path(&self) -> &'static str;

    // TODO! save(..)
}

// -----------------------------------------------------------------------------
// Placeholder

impl AssetSaver for () {
    type Asset = ();
    type Settings = ();
    type LoaderSettings = ();
    type Error = ZlimError;
}
