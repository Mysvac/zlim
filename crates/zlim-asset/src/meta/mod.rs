mod error;
mod hash;
mod info;
mod setting;
mod version;

pub use error::DeserializeMetaError;
pub use hash::AssetHash;
pub use info::*;
pub use setting::Settings;
pub use version::FormatVersion;
pub use version::FormatVersionMinimal;

// -----------------------------------------------------------------------------
// AssetMeta

use crate::loader::AssetLoader;
use crate::processor::AssetProcessor;
use core::any::Any;
use serde::{Deserialize, Serialize};

/// Asset metadata that informs how an [`Asset`] should be handled by the asset system.
///
/// - `L` is the [`AssetLoader`] (if one is configured) for the [`AssetConfig`].
/// - `P` is the [`AssetProcessor`] processor, if one is configured for the [`AssetConfig`].
/// - `L` / `P` can be `()` if it is not required.
///
/// [`Asset`]: crate::asset::Asset
#[derive(Serialize, Deserialize)]
// Currently only one version, no need to customize serializer and deserializer.
pub struct AssetMeta<L: AssetLoader, P: AssetProcessor> {
    #[serde(default)]
    pub format_version: FormatVersion,
    pub asset_config: AssetConfig<L::Settings, P::Settings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processed_info: Option<ProcessedInfo>,
}

// -----------------------------------------------------------------------------
// DynamicAssetMeta

/// A dynamic type-erased counterpart to [`AssetMeta`].
pub trait ErasedAssetMeta: Any + Send + Sync {
    /// Serializes the internal [`AssetMeta`].
    fn serialize(&self) -> Vec<u8>;

    /// Returns a reference to the [`ProcessedInfo`] if it exists.
    fn processed_info(&self) -> &Option<ProcessedInfo>;

    /// Returns a mutable reference to the [`ProcessedInfo`] if it exists.
    fn processed_info_mut(&mut self) -> &mut Option<ProcessedInfo>;

    /// Returns a reference to the [`AssetLoader`] settings, if they exist.
    fn loader_settings(&self) -> Option<&dyn Settings>;

    /// Returns a mutable reference to the [`AssetLoader`] settings, if they exist.
    fn loader_settings_mut(&mut self) -> Option<&mut dyn Settings>;

    /// Returns a reference to the asset processor settings, if they exist.
    fn process_settings(&self) -> Option<&dyn Settings>;
}

// -----------------------------------------------------------------------------
// Implementation

impl<L: AssetLoader, P: AssetProcessor> AssetMeta<L, P> {
    /// Create a new asset meta from given [`AssetConfig`],
    /// with default format version and empty processed info.
    #[inline]
    pub fn new(config: AssetConfig<L::Settings, P::Settings>) -> Self {
        Self {
            asset_config: config,
            format_version: FormatVersion::default(),
            processed_info: None,
        }
    }

    /// Deserializes the given serialized byte representation of the asset meta.
    #[inline]
    pub fn deserialize(bytes: &[u8]) -> Result<Self, DeserializeMetaError> {
        Ok(ron::de::from_bytes(bytes)?)
    }

    /// Serializes the asset meta in the recommended RON layout in a pretty way.
    #[inline]
    pub fn serialize(&self) -> Vec<u8> {
        use ron::ser::{PrettyConfig, to_string_pretty};
        const EXP: &str = "type is convertible to ron";
        // `newlines` is defaults to \r\n on Windows, hard-code it to \n for consistent.
        // `indentor` is default to 4 space, hard-code it to 2 space for compact format.
        let config = PrettyConfig::default().new_line("\n").indentor("  ");
        to_string_pretty(&self, config).expect(EXP).into_bytes()
    }
}

impl<L: AssetLoader, P: AssetProcessor> ErasedAssetMeta for AssetMeta<L, P> {
    fn serialize(&self) -> Vec<u8> {
        AssetMeta::serialize(self)
    }

    fn processed_info(&self) -> &Option<ProcessedInfo> {
        &self.processed_info
    }

    fn processed_info_mut(&mut self) -> &mut Option<ProcessedInfo> {
        &mut self.processed_info
    }

    fn loader_settings(&self) -> Option<&dyn Settings> {
        match &self.asset_config {
            AssetConfig::Load { settings, .. } => Some(settings),
            _ => None,
        }
    }

    fn loader_settings_mut(&mut self) -> Option<&mut dyn Settings> {
        match &mut self.asset_config {
            AssetConfig::Load { settings, .. } => Some(settings),
            _ => None,
        }
    }

    fn process_settings(&self) -> Option<&dyn Settings> {
        match &self.asset_config {
            AssetConfig::Process { settings, .. } => Some(settings),
            _ => None,
        }
    }
}

// -----------------------------------------------------------------------------
