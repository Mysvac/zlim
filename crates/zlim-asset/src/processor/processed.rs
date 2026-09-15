//! The processed side: what one of its outputs says, and how an output goes away.
//!
//! These are the same subject seen twice. *Reading* decides whether an asset's bytes can be loaded at
//! all: a pair whose `.meta` is missing, unreadable or unparsable is half an output, and is reported as
//! such rather than as "no output". *Deleting* is what happens when that is the case, when the source
//! behind the output is gone, or when a source was removed while the importer was running.
//!
//! Every deletion here happens under the **owning asset's** transaction lock. A reader that already
//! passed the processed-side gate finishes on the revision it started with, and no new read starts:
//! the gate's answer for an asset that is being forgotten is "it does not exist".

use std::path::Path;

use crate::io::{AssetReaderError, AssetWriterError, ErasedAssetWriter};
use crate::meta::{MetaParseError, ProcessedInfo, ProcessedInfoMinimal};
use crate::path::AssetPath;
use crate::processor::server::AssetProcessServer;
use crate::server::AssetServer;
use crate::source::AssetSource;

// -----------------------------------------------------------------------------
// What a processed output says

/// What the processed side says about one asset's output.
pub(crate) enum ProcessedOutput {
    /// The `.meta` next to the bytes was read and parsed: this is what it carries.
    ///
    /// `None` means the `.meta` records no output — it is not one the importer wrote. That is a
    /// *different* thing from an unusable one: the bytes can still be loaded with it.
    Recorded(Option<ProcessedInfo>),

    /// The bytes are there, but the `.meta` that says how to load them is not usable, so the pair is
    /// half an output: nothing can be read from it.
    Unusable(ProcessedMetaError),
}

/// Why the `.meta` of a processed output cannot be used.
pub(crate) enum ProcessedMetaError {
    /// There is no `.meta` next to the bytes.
    Missing,
    /// The `.meta` could not be read.
    Unreadable(AssetReaderError),
    /// The `.meta` was read, but it cannot be parsed.
    Malformed(MetaParseError),
}

impl ProcessedMetaError {
    /// Reports that a processed output is being thrown away, and why.
    ///
    /// The level says how surprising the reason is: a missing `.meta` is what an interrupted run
    /// leaves behind (the bytes are written before it), so it is a debug line; a `.meta` this crate
    /// wrote and can no longer parse is a warning; and one that cannot be read at all is an error.
    pub(crate) fn log(&self, path: &AssetPath<'_>) {
        match self {
            Self::Missing => {
                zlim_log::debug!("Removing the processed output of '{path}': it has no `.meta`")
            }
            Self::Malformed(error) => zlim_log::warn!(
                "Removing the processed output of '{path}': its `.meta` cannot be parsed: {error}"
            ),
            Self::Unreadable(error) => zlim_log::error!(
                "Removing the processed output of '{path}': its `.meta` cannot be read: {error}"
            ),
        }
    }
}

// -----------------------------------------------------------------------------
// AssetServer: reading what the processed side says

impl AssetServer {
    /// Returns what the processed side of `source` says about the output of `path`.
    ///
    /// A missing processed side, and a `.meta` that parses but records no [`ProcessedInfo`], are both
    /// [`ProcessedOutput::Recorded`]: there is a `.meta` to load the bytes with, nothing is broken. A
    /// pair whose `.meta` is missing, unreadable or unparsable is [`ProcessedOutput::Unusable`] — the
    /// bytes are there, but nothing says how to read them.
    ///
    /// That distinction is the whole of [`AssetProcessServer`]'s `sync_processed_side`: an unusable
    /// pair is not an output, and does not stay on the processed side.
    ///
    /// The *ungated* processed reader is used on purpose: this is the importer reading its own
    /// bookkeeping — what the processed side says went into each asset — not an asset load. Waiting
    /// on the gate here would mean waiting for the state this very pass is about to build, while the
    /// gate's answer for a path is still "unknown".
    ///
    /// [`AssetProcessServer`]: crate::processor::AssetProcessServer
    pub(crate) async fn processed_output_of(
        &self,
        source: &AssetSource,
        path: &AssetPath<'static>,
    ) -> ProcessedOutput {
        // No processed side: there is nothing there to read, usable or not.
        let Some(processed_reader) = source.ungated_processed_reader() else {
            return ProcessedOutput::Recorded(None);
        };

        let processed_meta = match processed_reader.read_meta_bytes(path.path()).await {
            Ok(bytes) => bytes,
            Err(error) if error.is_not_found() => {
                return ProcessedOutput::Unusable(ProcessedMetaError::Missing);
            }
            Err(error) => {
                return ProcessedOutput::Unusable(ProcessedMetaError::Unreadable(error));
            }
        };

        match ProcessedInfoMinimal::from_bytes(&processed_meta) {
            Ok(minimal) => ProcessedOutput::Recorded(minimal.processed_info),
            Err(error) => ProcessedOutput::Unusable(ProcessedMetaError::Malformed(error)),
        }
    }
}

// -----------------------------------------------------------------------------
// AssetProcessServer: losing an output

impl AssetProcessServer {
    /// Deletes the processed pair of an asset that is still a source asset, under its write lock.
    ///
    /// The entry stays, and so does its status channel: the pass that is about to rewrite the pair
    /// answers the same waiters on it. Used where the pair cannot be loaded at all — the files have no
    /// business being on the processed side, and the asset is about to be processed again.
    pub(super) async fn discard_processed_pair(
        &self,
        source: &AssetSource,
        path: &AssetPath<'static>,
    ) {
        let _write_lock = self.data.state.transaction_lock_for_write(path).await;
        self.remove_processed_asset_and_meta(source, path).await;
    }

    /// Forgets an asset whose source is gone: its entry, its processed pair, and the folders the
    /// deletion empties.
    ///
    /// A live removal does the same thing, in the same order: the entry goes first — which is what
    /// answers its waiters `NonExistent`, and parks the assets that depended on it — and its lock is
    /// then held for writing, so a reader of the previous revision finishes before the files go away.
    /// A path the index never knew has no reader to wait for; its files are only there if something
    /// wrote them outside the importer.
    pub(super) async fn forget_processed_asset(
        &self,
        source: &AssetSource,
        path: &AssetPath<'static>,
    ) {
        let lock = {
            let mut infos = self.data.state.write_infos().await;
            infos.remove(path).await
        };

        let _write_lock = match lock {
            Some(lock) => Some(lock.write_arc().await),
            None => None,
        };

        self.remove_processed_asset_and_meta(source, path).await;
    }

    /// Deletes the processed bytes and `.meta` of `path`, and any folder that became empty.
    ///
    /// The lock belongs to the caller: this is the file part alone, so that a removal and a discard
    /// differ only in what they do with the index.
    async fn remove_processed_asset_and_meta(
        &self,
        source: &AssetSource,
        path: &AssetPath<'static>,
    ) {
        let Ok(processed_writer) = source.processed_writer() else {
            return;
        };

        if let Err(error) = processed_writer.remove(path.path()).await {
            report_remove_error("remove", path.path(), error);
        }

        if let Err(error) = processed_writer.remove_meta(path.path()).await {
            report_remove_error("remove the meta of", path.path(), error);
        }

        prune_empty_parents(processed_writer, path.path()).await;
    }
}

// -----------------------------------------------------------------------------
// The files behind it

/// Asks the writer to remove every ancestor folder of `path`, deepest first, stopping at the root of
/// the processed side (an empty relative path).
///
/// Best effort: the writer decides — it refuses a folder that still holds something, and the refusal
/// is reported — and a folder that stays is a cosmetic problem, never a correctness one.
async fn prune_empty_parents(writer: &dyn ErasedAssetWriter, path: &Path) {
    let mut parent = path.parent();

    while let Some(directory) = parent {
        if directory.as_os_str().is_empty() {
            break;
        }

        if let Err(error) = writer.remove_empty_directory(directory).await {
            report_remove_error("remove the empty folder", directory, error);
        }

        parent = directory.parent();
    }
}

/// Logs a failed write on the processed side.
///
/// Every failure is logged, a missing file included; unlike a removal (see [`report_remove_error`]),
/// nothing here is filtered by the kind of error.
#[cold]
pub(super) fn report_write_error(action: &str, path: &Path, error: AssetWriterError) {
    zlim_log::error!(
        "Failed to {action} the processed asset '{}': {error}",
        path.display()
    );
}

/// Logs a failed remove on the processed side.
///
/// A missing file is not a failure: removing the output of an asset that never had one is exactly
/// what "its source is gone" means.
#[cold]
pub(super) fn report_remove_error(action: &str, path: &Path, error: AssetWriterError) {
    if !error.is_not_find() {
        zlim_log::error!(
            "Failed to {action} the processed asset '{}': {error}",
            path.display()
        );
    }
}
