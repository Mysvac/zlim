use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::meta::AssetHash;
use crate::meta::error::MetaParseError;
use crate::path::AssetPath;

// -----------------------------------------------------------------------------
// ProcessedInfo

/// Information about a dependency used to process an asset.
///
/// This is used to determine whether an asset's "process dependency" has changed.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProcessDependencyInfo {
    /// A hash of the dependency's `.meta` data and [`ProcessedInfo`] information.
    pub full_hash: AssetHash,
    /// The dependency's asset path.
    pub path: AssetPath<'static>,
}

/// Info produced by the `AssetProcessor` for a given processed asset.
///
/// This is used to determine if an asset source file (or its dependencies) has changed.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ProcessedInfo {
    /// A hash of the asset bytes and the asset `.meta` data
    pub hash: AssetHash,
    /// A hash of the asset bytes, the asset `.meta` data,
    /// and the `full_hash` of every `process_dependency`.
    pub full_hash: AssetHash,
    /// Information about the process dependencies used to process this asset.
    pub process_dependencies: Vec<ProcessDependencyInfo>,
}

// -----------------------------------------------------------------------------
// AssetConfig

/// Configures how an asset source file should be handled by the asset system.
#[derive(Serialize, Deserialize)]
pub enum AssetConfig<LoaderSettings, ProcessSettings> {
    /// Load the asset with the given loader and settings.
    Load {
        /// The loader's type name, as it was written by whatever produced the config: the short name
        /// when the loader sets [`SHORT_NAME`](crate::loader::AssetLoader::SHORT_NAME), its full type
        /// path otherwise. Both are resolved by the lenient name look-up.
        ///
        /// An empty string is the config that names no loader — it is not serialized (see the `serde`
        /// attributes), so writing such a config simply omits the field.
        #[serde(default)]
        #[serde(skip_serializing_if = "str::is_empty")]
        loader: Cow<'static, str>,
        settings: LoaderSettings,
    },

    /// Process the asset with the given processor and settings.
    Process {
        /// The processor's type name, with the same naming and empty-string rules as
        /// [`load`](Self::Load)'s loader.
        #[serde(default)]
        #[serde(skip_serializing_if = "str::is_empty")]
        processor: Cow<'static, str>,
        settings: ProcessSettings,
    },

    /// Do nothing with the asset (it is not an asset).
    Ignore,

    /// Without special configurations, use default instead.
    ///
    /// This is essentially equivalent to the absence of meta.
    None,
}

impl<L, P> Default for AssetConfig<L, P> {
    #[inline]
    fn default() -> Self {
        Self::None
    }
}

impl<L, P> AssetConfig<L, P> {
    /// Return `true` if self is [`AssetConfig::None`].
    #[inline]
    pub const fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Return `true` if self is [`AssetConfig::Ignore`].
    #[inline]
    pub const fn is_ignore(&self) -> bool {
        matches!(self, Self::Ignore)
    }

    /// Return `true` if self is [`AssetConfig::Load`].
    #[inline]
    pub const fn is_load(&self) -> bool {
        matches!(self, Self::Load { .. })
    }

    /// Return `true` if self is [`AssetConfig::Process`].
    #[inline]
    pub const fn is_process(&self) -> bool {
        matches!(self, Self::Process { .. })
    }
}

// -----------------------------------------------------------------------------
// accelerator

/// A minimal counterpart to [`ProcessedInfo`] that exists to speed up
/// deserialization in cases where the whole `AssetMeta` isn't necessary.
#[derive(Deserialize)]
pub struct ProcessedInfoMinimal {
    /// The processed info recorded in the `.meta`, if it carries any.
    pub processed_info: Option<ProcessedInfo>,
}

/// A minimal counterpart to [`AssetConfig`]'s loader or processor path.
#[derive(Default, Deserialize)]
pub enum AssetActionMinimal {
    /// Load the asset with the named loader.
    Load {
        /// The loader's type name; an empty string means "records none".
        #[serde(default)]
        loader: String,
    },
    /// Process the asset with the named processor.
    Process {
        /// The processor's type name; an empty string means "records none".
        #[serde(default)]
        processor: String,
    },
    /// Do nothing with the asset (it is not an asset).
    Ignore,
    /// Without special configurations, use default instead.
    #[default]
    None,
}

/// A minimal counterpart to [`AssetConfig`] that exists to speed up
/// deserialization in cases where the whole `AssetMeta` isn't necessary.
#[derive(Deserialize)]
pub struct AssetConfigMinimal {
    /// The action the `.meta` recorded for the asset.
    #[serde(default)]
    pub asset_config: AssetActionMinimal,
}

impl ProcessedInfoMinimal {
    /// Deserialize a value of [`ProcessedInfoMinimal`] from bytes.
    #[inline]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MetaParseError> {
        Ok(ron::de::from_bytes::<Self>(bytes)?)
    }
}

impl AssetConfigMinimal {
    /// Deserialize a value of [`AssetConfigMinimal`] from bytes.
    #[inline]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MetaParseError> {
        Ok(ron::de::from_bytes::<Self>(bytes)?)
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;

    /// A manual scratchpad rather than a check: it prints the RON form of every `AssetConfig`
    /// variant and of `ProcessedInfo`, so the on-disk `.meta` format can be read off after the
    /// types change.
    #[test]
    #[ignore = "manual trigger"]
    #[expect(clippy::print_stderr, reason = "format display")]
    fn ron_text() {
        // ---- AssetConfig ----
        let load = AssetConfig::<&'static str, &'static str>::Load {
            loader: "png".into(),
            settings: "load-settings",
        };
        let process = AssetConfig::<&'static str, &'static str>::Process {
            processor: "compress".into(),
            settings: "process-settings",
        };
        let ignore = AssetConfig::<&'static str, &'static str>::Ignore;

        std::eprintln!(
            "----------------------------\nAssetConfig::Load :\n{}\n----------------------------",
            ron::ser::to_string_pretty(&load, ron::ser::PrettyConfig::default()).unwrap()
        );
        std::eprintln!(
            "----------------------------\nAssetConfig::Process :\n{}\n----------------------------",
            ron::ser::to_string_pretty(&process, ron::ser::PrettyConfig::default()).unwrap()
        );
        std::eprintln!(
            "----------------------------\nAssetConfig::Ignore :\n{}\n----------------------------",
            ron::ser::to_string_pretty(&ignore, ron::ser::PrettyConfig::default()).unwrap()
        );

        // ---- ProcessedInfo ----
        let processed = ProcessedInfo {
            hash: AssetHash::ZERO,
            full_hash: AssetHash::ZERO,
            process_dependencies: vec![
                ProcessDependencyInfo {
                    full_hash: AssetHash::ZERO,
                    path: AssetPath::from("textures/foo.png"),
                },
                ProcessDependencyInfo {
                    full_hash: AssetHash::ZERO,
                    path: AssetPath::from("textures/bar.png"),
                },
            ],
        };

        std::eprintln!(
            "----------------------------\nProcessedInfo :\n{}\n----------------------------",
            ron::ser::to_string_pretty(&processed, ron::ser::PrettyConfig::default()).unwrap()
        );
    }
}
