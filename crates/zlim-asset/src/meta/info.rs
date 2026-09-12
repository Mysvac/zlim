use serde::{Deserialize, Serialize};

use crate::meta::{AssetHash, DeserializeMetaError};
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
    /// dependency's asset path
    pub path: AssetPath<'static>,
}

/// Info produced by the `AssetProcessor` for a given processed asset.
///
/// This is used to determine if an asset source file (or its dependencies) has changed.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProcessedInfo {
    /// A hash of the asset bytes and the asset `.meta` data
    pub hash: AssetHash,
    /// A hash of the asset bytes, the asset .meta data,
    /// and the `full_hash` of every `process_dependency`.
    pub full_hash: AssetHash,
    // Information about the "process dependencies" used to process this asset.
    pub process_dependencies: Vec<ProcessDependencyInfo>,
}

// -----------------------------------------------------------------------------
// AssetInfo

/// Configures how an asset source file should be handled by the asset system.
#[derive(Serialize, Deserialize)]
pub enum AssetConfig<LoaderSettings, ProcessSettings> {
    /// Load the asset with the given loader and settings.
    Load {
        loader: String,
        settings: LoaderSettings,
    },
    /// Process the asset with the given processor and settings.
    Process {
        processor: String,
        settings: ProcessSettings,
    },
    /// Do nothing with the asset
    Ignore,
}

/// A lightweight [`AssetConfig`].
#[derive(Serialize, Deserialize)]
pub enum AssetConfigKind {
    Load { loader: String },
    Process { processor: String },
    Ignore,
}

// -----------------------------------------------------------------------------
// accelerator

/// A minimal counterpart to [`ProcessedInfo`] that exists to speed up
/// deserialization in cases where the whole `AssetMeta` isn't necessary.
#[derive(Deserialize)]
pub struct ProcessedInfoMinimal {
    pub processed_info: Option<ProcessedInfo>,
}

/// A minimal counterpart to [`AssetConfig`] that exists to speed up
/// deserialization in cases where the whole `AssetMeta` isn't necessary.
#[derive(Deserialize)]
pub struct AssetConfigMinimal {
    pub asset_config: AssetConfigKind,
}

impl ProcessedInfoMinimal {
    /// Deserialize a value of [`ProcessedInfoMinimal`] from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DeserializeMetaError> {
        ron::de::from_bytes::<Self>(bytes).map_err(DeserializeMetaError::process_info)
    }
}

impl AssetConfigMinimal {
    /// Deserialize a value of [`AssetConfigMinimal`] from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DeserializeMetaError> {
        ron::de::from_bytes::<Self>(bytes).map_err(DeserializeMetaError::asset_config)
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "manual trigger"]
    #[expect(clippy::print_stderr, reason = "format display")]
    fn ron_text() {
        // ---- AssetConfig ----
        let load = AssetConfig::<&'static str, &'static str>::Load {
            loader: "png".to_string(),
            settings: "load-settings",
        };
        let process = AssetConfig::<&'static str, &'static str>::Process {
            processor: "compress".to_string(),
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
