#![expect(clippy::module_inception, reason = "For better structure.")]

use core::any::TypeId;

use serde::{Deserialize, Serialize};
use zlim_path::TypePath;

use crate::meta::Settings;

// -----------------------------------------------------------------------------
// AssetLoader

pub trait AssetProcessor: TypePath + Send + Sync + 'static {
    /// The settings type used by this [`AssetProcessor`].
    type Settings: Settings + Default + Serialize + for<'a> Deserialize<'a>;

    /// The settings type produced by this processor, used by [`AssetLoader`].
    ///
    /// These correspond to the `L` type parameter of [`AssetMeta<L, P>`] — the
    /// settings the loader needs when reading the final asset back.
    ///
    /// [`AssetLoader`]: crate::loader::AssetLoader
    /// [`AssetMeta<L, P>`]: crate::meta::AssetMeta
    type LoaderSettings: Settings + Default + Serialize + for<'a> Deserialize<'a>;

    // TODO! process(..)
}

// -----------------------------------------------------------------------------
// ErasedAssetProcessor

/// A type-erased variant of [`AssetProcessor`].
pub trait ErasedAssetProcessor: Send + Sync + 'static {
    fn type_id(&self) -> TypeId;

    fn type_path(&self) -> &'static str;

    // TODO! process(..)
    // TODO! default_meta(..)
    // TODO! deserialize_meta(..)
}

// -----------------------------------------------------------------------------
// Placeholder

impl AssetProcessor for () {
    type Settings = ();
    type LoaderSettings = ();
}
