use std::borrow::Cow;

use zlim_utils::hash::HashSet;

use crate::ident::AssetSourceId;
use crate::path::AssetPath;

/// Selects whether the asset server reads from raw sources or processed/imported sources.
///
/// - `Unprocessed` (default): assets are read directly from the file system source
///   (e.g. `assets/`).  No import step is applied.
/// - `Processed`: assets are read from the processed output folder
///   (default `imported_assets/default`), which is what an asset processor writes. `AssetPlugin`
///   builds and starts the importer for this mode unless [`use_asset_processor_override`] is
///   `Some(false)`.
///
/// [`use_asset_processor_override`]: crate::plugin::AssetPlugin::use_asset_processor_override
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetServerMode {
    #[default]
    Unprocessed,
    Processed,
}

/// Controls how the asset server handles paths that escape their source root (e.g. `../`).
///
/// - `Allow`: unapproved paths are loaded without any special treatment.
/// - `Deny` (default): unapproved paths are refused, unless the caller asks for them explicitly
///   with [`LoadBuilder::override_unapproved`] or [`SaveBuilder::override_unapproved`].
/// - `Forbid`: unapproved paths are always refused; the override does not apply.
///
/// A refused path is not loaded: the caller gets a default handle, the reason is logged, and
/// nothing is registered as a dependency.
///
/// [`LoadBuilder::override_unapproved`]: crate::server::LoadBuilder::override_unapproved
/// [`SaveBuilder::override_unapproved`]: crate::server::SaveBuilder::override_unapproved
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub enum UnapprovedPathMode {
    Allow,
    #[default]
    Deny,
    Forbid,
}

/// Controls when the asset server reads `.meta` sidecar files.
///
/// - `Always` (default): every asset load checks for a `.meta` file and, if found,
///   uses it to select the loader and settings.
/// - `Custom { .. }`: sources are answered first — `checked_sources` wins, then
///   `skipped_sources` — and the per-path and per-prefix lists only decide for the sources that
///   are in neither; whatever is left is checked.
/// - `Never`: skip `.meta` checks entirely; always use default settings and
///   extension-based loader selection.
#[derive(Default, Clone, Debug, PartialEq)]
pub enum AssetMetaCheckMode {
    #[default]
    Always,
    Custom {
        /// Sources whose assets always read a `.meta`, whatever the lists below say.
        checked_sources: HashSet<AssetSourceId>,
        /// Sources whose assets never read a `.meta`, consulted after `checked_sources`.
        skipped_sources: HashSet<AssetSourceId>,
        /// Paths whose `.meta` is not read, compared against the path's printed form
        /// (`[source://]path[#label]`). Formatted on every check, so it is meant to name a few
        /// exceptions only.
        skipped_paths: HashSet<Cow<'static, str>>,
        /// Path prefixes whose `.meta` is not read, matched against the same printed form as
        /// `skipped_paths`. Formatted and scanned on every check, so it is meant to name a few
        /// exceptions only.
        skipped_prefixes: Vec<Cow<'static, str>>,
    },
    Never,
}

impl AssetMetaCheckMode {
    /// Returns `true` if a `.meta` next to `path` should be read.
    ///
    /// [`Custom`](AssetMetaCheckMode::Custom) answers per source first — `checked_sources` wins
    /// over `skipped_sources` — and then per path: a path in `skipped_paths`, or one starting with
    /// a `skipped_prefixes` entry, is not checked, and everything else is.
    #[inline]
    pub fn should_check(&self, path: &AssetPath<'_>) -> bool {
        match self {
            AssetMetaCheckMode::Always => true,
            AssetMetaCheckMode::Never => false,
            AssetMetaCheckMode::Custom {
                checked_sources,
                skipped_sources,
                skipped_paths,
                skipped_prefixes,
            } => {
                let source = path.source_id();
                if checked_sources.contains(&source) {
                    return true;
                }
                if skipped_sources.contains(&source) {
                    return false;
                }
                if skipped_paths.is_empty() && skipped_prefixes.is_empty() {
                    return true;
                }
                let path = path.to_string();
                if skipped_paths.contains(path.as_str()) {
                    return false;
                }
                if skipped_prefixes
                    .iter()
                    .any(|s| path.starts_with(s.as_ref()))
                {
                    return false;
                }
                true
            }
        }
    }
}
