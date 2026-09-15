//! The `.meta` format: the asset config, the processed info and the content hash.

mod error;
mod hash;
mod info;
mod setting;
mod version;

pub use error::AssetMetaParseError;
pub use error::MetaParseError;
pub use hash::AssetHash;
pub use info::*;
pub use setting::Settings;
pub use version::FormatVersion;
pub use version::FormatVersionMinimal;

// -----------------------------------------------------------------------------
// AssetMeta

use core::any::Any;
use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// Asset metadata that informs how an [`Asset`] should be handled by the asset system.
///
/// The two type parameters are the **settings** types of the configured loader and processor:
///
/// - `L` is the [`AssetLoader::Settings`].
/// - `P` is the [`AssetProcessor::Settings`].
///
/// Either parameter is `()` when the matching [`AssetConfig`] variant is not used.
///
/// [`Asset`]: crate::asset::Asset
/// [`AssetLoader::Settings`]: crate::loader::AssetLoader
/// [`AssetProcessor::Settings`]: crate::processor::AssetProcessor
#[derive(Serialize, Deserialize)]
// Currently only one version, no need to customize serializer and deserializer.
pub struct AssetMeta<LoaderSettings, ProcessorSettings> {
    /// The `.meta` format version this file was written with.
    #[serde(default)]
    pub format_version: FormatVersion,
    /// How the asset is handled: ignore it, load it, or process it.
    #[serde(default = "AssetConfig::default")]
    pub asset_config: AssetConfig<LoaderSettings, ProcessorSettings>,
    /// What the importer recorded the last time it processed this asset.
    ///
    /// It is omitted from the serialized form when it is `None`, which is how a `.meta`
    /// that the importer never touched stays free of its bookkeeping.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processed_info: Option<ProcessedInfo>,
}

// -----------------------------------------------------------------------------
// ErasedAssetMeta

/// A dynamic type-erased counterpart to [`AssetMeta`].
pub trait ErasedAssetMeta: Any + Send + Sync {
    /// Serializes the internal [`AssetMeta`].
    ///
    /// Serialization must not consume internal data.
    fn serialize(&self) -> Vec<u8>;

    /// Returns a reference to the [`ProcessedInfo`] if it exists.
    fn processed_info(&self) -> &Option<ProcessedInfo>;

    /// Returns a mutable reference to the [`ProcessedInfo`] if it exists.
    fn processed_info_mut(&mut self) -> &mut Option<ProcessedInfo>;

    /// Returns a reference to the loader settings, if the config carries any.
    fn loader_settings(&self) -> Option<&dyn Settings>;

    /// Returns a mutable reference to the loader settings, if the config carries any.
    fn loader_settings_mut(&mut self) -> Option<&mut dyn Settings>;

    /// Returns a reference to the asset processor settings, if the config carries any.
    fn process_settings(&self) -> Option<&dyn Settings>;

    /// Returns the loader's type name, if the config is a `Load` one.
    ///
    /// The name is the empty string when the config records none: an empty name is what a `.meta`
    /// with no loader serializes to, and what deserializing one without the field gives. It is treated
    /// as "no name" rather than as a name wherever it is looked up — the same way a load builder treats
    /// an empty name handed to [`with_loader_name`](crate::server::LoadBuilder::with_loader_name).
    fn loader_name(&self) -> Option<&str>;

    /// Returns the processor's type name, if the config is a `Process` one.
    ///
    /// As with [`loader_name`](Self::loader_name), an empty string means "records none".
    fn processor_name(&self) -> Option<&str>;

    /// Returns a mutable reference to the loader's type name, if the config is a `Load` one.
    fn loader_name_mut(&mut self) -> Option<&mut Cow<'static, str>>;

    /// Returns a mutable reference to the processor's type name, if the config is a `Process` one.
    fn processor_name_mut(&mut self) -> Option<&mut Cow<'static, str>>;
}

// -----------------------------------------------------------------------------
// Implementation

impl<L, P> AssetMeta<L, P>
where
    L: Settings + Serialize + for<'d> Deserialize<'d>,
    P: Settings + Serialize + for<'d> Deserialize<'d>,
{
    /// Create a new asset meta from given [`AssetConfig`],
    /// with default format version and empty processed info.
    #[inline]
    pub fn new(config: AssetConfig<L, P>) -> Self {
        Self {
            asset_config: config,
            format_version: FormatVersion::default(),
            processed_info: None,
        }
    }

    /// Deserializes the given serialized byte representation of the asset meta.
    #[inline]
    pub fn deserialize(bytes: &[u8]) -> Result<Self, MetaParseError> {
        Ok(ron::de::from_bytes::<Self>(bytes)?)
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

impl<L, P> ErasedAssetMeta for AssetMeta<L, P>
where
    L: Settings + Serialize + for<'d> Deserialize<'d>,
    P: Settings + Serialize + for<'d> Deserialize<'d>,
{
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

    fn loader_name(&self) -> Option<&str> {
        match &self.asset_config {
            AssetConfig::Load { loader, .. } => Some(loader),
            _ => None,
        }
    }

    fn processor_name(&self) -> Option<&str> {
        match &self.asset_config {
            AssetConfig::Process { processor, .. } => Some(processor),
            _ => None,
        }
    }

    fn loader_name_mut(&mut self) -> Option<&mut Cow<'static, str>> {
        match &mut self.asset_config {
            AssetConfig::Load { loader, .. } => Some(loader),
            _ => None,
        }
    }

    fn processor_name_mut(&mut self) -> Option<&mut Cow<'static, str>> {
        match &mut self.asset_config {
            AssetConfig::Process { processor, .. } => Some(processor),
            _ => None,
        }
    }
}

impl<L, P> From<AssetMeta<L, P>> for Box<dyn ErasedAssetMeta>
where
    L: Settings + Serialize + for<'d> Deserialize<'d>,
    P: Settings + Serialize + for<'d> Deserialize<'d>,
{
    #[inline]
    fn from(value: AssetMeta<L, P>) -> Self {
        Box::new(value)
    }
}

impl<'a, L, P> From<&'a AssetMeta<L, P>> for &'a dyn ErasedAssetMeta
where
    L: Settings + Serialize + for<'d> Deserialize<'d>,
    P: Settings + Serialize + for<'d> Deserialize<'d>,
{
    #[inline]
    fn from(value: &'a AssetMeta<L, P>) -> Self {
        value
    }
}

impl<'a, L, P> From<&'a mut AssetMeta<L, P>> for &'a dyn ErasedAssetMeta
where
    L: Settings + Serialize + for<'d> Deserialize<'d>,
    P: Settings + Serialize + for<'d> Deserialize<'d>,
{
    #[inline]
    fn from(value: &'a mut AssetMeta<L, P>) -> Self {
        value
    }
}

// -----------------------------------------------------------------------------
