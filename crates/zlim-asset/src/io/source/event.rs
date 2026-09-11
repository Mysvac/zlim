use std::path::PathBuf;

// -----------------------------------------------------------------------------
// AssetSourceEvent

/// An "asset source change event" that occurs whenever
/// asset (or asset metadata) is created/added/removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetSourceEvent {
    /// An asset at this path was added.
    AddedAsset(PathBuf),
    /// An asset at this path was modified.
    ModifiedAsset(PathBuf),
    /// An asset at this path was removed.
    RemovedAsset(PathBuf),
    /// An asset at this path was renamed.
    RenamedAsset { old: PathBuf, new: PathBuf },
    /// Asset metadata at this path was added.
    AddedMeta(PathBuf),
    /// Asset metadata at this path was modified.
    ModifiedMeta(PathBuf),
    /// Asset metadata at this path was removed.
    RemovedMeta(PathBuf),
    /// Asset metadata at this path was renamed.
    RenamedMeta { old: PathBuf, new: PathBuf },
    /// A folder at the given path was added.
    AddedFolder(PathBuf),
    /// A folder at the given path was removed.
    RemovedFolder(PathBuf),
    /// A folder at the given path was renamed.
    RenamedFolder { old: PathBuf, new: PathBuf },
    /// Something of unknown type was removed.
    ///
    /// It is the job of the event handler to determine the type.
    /// This exists because notify-rs produces "untyped" rename events
    /// without destination paths for unwatched folders, so we can't
    /// determine the type of the rename.
    RemovedUnknown {
        /// The path of the removed asset or folder (undetermined).
        ///
        /// This could be an asset path or a folder. This will not be a "meta file" path.
        path: PathBuf,
        /// This field is only relevant if `path` is determined to be an asset path (and therefore not a folder).
        ///
        /// - If this field is `true`, then this event corresponds to a meta removal (not an asset removal) .
        /// - If `false`, then this event corresponds to an asset removal (not a meta removal).
        is_meta: bool,
    },
}
