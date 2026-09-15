//! The scan a run starts with: every source asset is registered, and the processed side is brought in
//! line with it.
//!
//! Two questions are settled here, and both of them *before* the run processes anything — which is what
//! makes them exact:
//!
//! - *which assets are there*: every source file gets an entry, so a processor that reads a process
//!   dependency **waits** for it instead of being told the asset does not exist;
//! - *what the processed side holds*: the index is rebuilt from the `.meta` files the last run wrote,
//!   so "is this still up to date" is answered from what was actually recorded.
//!
//! The scan is also what prunes the processed side, by one rule: an output stays only when a source
//! backs it *and* its `.meta` can be read. See [`AssetProcessServer::sync_processed_side`].

use std::path::{Path, PathBuf};

use futures_lite::StreamExt;
use zlim_utils::hash::HashSet;

use crate::io::{AssetReaderError, ErasedAssetReader, ErasedAssetWriter};
use crate::path::AssetPath;
use crate::processor::processed::ProcessedOutput;
use crate::processor::server::{AssetProcessServer, InitializeError, ProcessorState};
use crate::source::AssetSource;

// -----------------------------------------------------------------------------
// AssetProcessServer: the scan

impl AssetProcessServer {
    /// Scans every source, and brings the processed side in line with it.
    ///
    /// This is what a pass knows before it starts working: every source file has an entry (so a
    /// dependency read *waits* for an asset instead of being told it does not exist), and the index
    /// describes what the processed side holds (so "is this still up to date" can be answered
    /// exactly).
    ///
    /// It is also what initializes the importer, which is why it ends by announcing
    /// [`ProcessorState::Processing`]: the assets are all registered by then, so a read that arrives
    /// while a run is still scanning waits for that scan instead of being told the asset does not
    /// exist.
    ///
    /// Returns every source path it registered, which is the work list of the pass that follows.
    ///
    /// # Errors
    ///
    /// Fails when the source side or the processed side of a source cannot be read; the phase that
    /// failed says which of the two it was.
    pub(super) async fn initialize(&self) -> Result<Vec<AssetPath<'static>>, InitializeError> {
        let mut paths = Vec::new();
        let mut source_files = Vec::new();

        for source in self.data.sources.iter() {
            if let Err(error) =
                collect_paths(source.reader(), PathBuf::new(), &mut source_files, None).await
            {
                ::core::hint::cold_path();
                return Err(InitializeError::FailedToReadSourcePaths(error));
            }

            paths.extend(
                source_files
                    .drain(..)
                    .map(|path| AssetPath::from(path).with_source_id(source.id())),
            );
        }

        self.register_paths(&paths).await;
        self.sync_processed_side(&paths).await?;

        self.data.state.set_state(ProcessorState::Processing).await;

        Ok(paths)
    }

    /// Adds every path as a known source asset, without a result yet.
    ///
    /// This is what makes [`ProcessStatus::NonExistent`](crate::processor::ProcessStatus::NonExistent)
    /// mean "not a source asset" rather than "not looked at yet": a pass registers every file it is
    /// about to work on *before* it starts, so a dependency read waits for an asset that is still
    /// queued instead of being told it is missing.
    pub(super) async fn register_paths(&self, paths: &[AssetPath<'static>]) {
        let mut infos = self.data.state.write_infos().await;

        for path in paths {
            infos.get_or_insert(path);
        }
    }
}

// -----------------------------------------------------------------------------
// AssetProcessServer: bringing the processed side in line

impl AssetProcessServer {
    /// Rebuilds the index from the processed side, and deletes what no source backs any more.
    ///
    /// The processed side is the *derived* side, and this is where that is enforced. One rule, said
    /// once: an output is kept only when a source asset still exists under the same path *and* the
    /// `.meta` that says how to load it can be read. Everything else is a ghost — the output of a
    /// source that is gone, half a pair an interrupted run left behind — and goes away, under the lock
    /// of the asset it belongs to.
    ///
    /// Two things follow from keeping the index and the files in step this way:
    ///
    /// - every source path starts with *no* recorded output, so an index left over from an earlier
    ///   pass cannot make an asset look up to date after its processed file was deleted;
    /// - an asset whose source is gone is *forgotten*, not merely emptied: its entry goes away with
    ///   its files, so a read of it is told it does not exist instead of waiting for a pass that will
    ///   never come.
    ///
    /// What a listing shows is the assets: `read_directory` never yields a `.meta`, on any backend, so
    /// a lone `.meta` whose bytes are gone is out of this scan's reach. Nothing the importer writes can
    /// leave one behind — a pair is written bytes first, and removed as a pair — so it would take
    /// something outside the importer to put one there.
    ///
    /// The *ungated* processed reader is used: this is the importer reading its own bookkeeping, not
    /// an asset load (waiting for the gate here would mean waiting for the state this pass builds).
    ///
    /// # Errors
    ///
    /// Fails when the processed side of a source cannot be listed.
    async fn sync_processed_side(
        &self,
        source_paths: &[AssetPath<'static>],
    ) -> Result<(), InitializeError> {
        // Cleared first; the processed side below fills it back in.
        {
            let mut infos = self.data.state.write_infos().await;

            for path in source_paths {
                infos.record(path, None);
            }
        }

        let sources: HashSet<&AssetPath<'static>> = source_paths.iter().collect();

        for source in self.data.sources.iter_processed() {
            let Some((processed_reader, processed_writer)) = processed_rw_of(source) else {
                continue;
            };

            let mut files = Vec::new();
            let mut empty_folders = Vec::new();

            if let Err(error) = collect_paths(
                processed_reader,
                PathBuf::new(),
                &mut files,
                Some(&mut empty_folders),
            )
            .await
            {
                ::core::hint::cold_path();
                return Err(InitializeError::FailedToReadDestinationPaths(error));
            }

            // Folders that held nothing: best effort, and children before parents. The folders the
            // deletions below empty are pruned as they happen.
            for folder in empty_folders {
                let _ = processed_writer.remove_empty_directory(&folder).await;
            }

            for file in files {
                let path = AssetPath::from(file).with_source_id(source.id());

                // The source is what an output belongs to: without it, no `.meta` in the pair can
                // change that the pair is a ghost.
                if !sources.contains(&path) {
                    ::core::hint::cold_path();
                    zlim_log::debug!(
                        "Removing the processed output of '{path}': its source is gone"
                    );
                    self.forget_processed_asset(source, &path).await;
                    continue;
                }

                // A source exists, so the only question left is whether the pair can be read at all.
                match self.server.processed_output_of(source, &path).await {
                    ProcessedOutput::Recorded(processed_info) => {
                        self.data
                            .state
                            .write_infos()
                            .await
                            .record(&path, processed_info);
                    }
                    // The pass that follows is about to write this asset again; until it does, a pair
                    // nothing can load has no business being on the processed side.
                    ProcessedOutput::Unusable(reason) => {
                        reason.log(&path);
                        self.discard_processed_pair(source, &path).await;
                    }
                }
            }
        }

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Listing a side of a source

/// Lists every asset under `path`, and the folders that hold none.
///
/// A directory stream lists one level, so this walks down (boxed, because an async recursive function
/// needs a concrete future size). `None` for `empty_folders` means "only the files matter": that is
/// what the source side asks for, since nothing is written there.
///
/// A `.meta` is not an asset: `read_directory` never lists one, on any backend — metadata is reachable
/// only through `read_meta` — and [`is_meta_file`] is what keeps one out of the listing if a reader
/// ever did list it.
///
/// Returns whether anything was found under `path`, so a parent can tell that it is not empty. A
/// folder is reported *after* its children, so removing them in order empties the deepest one first.
async fn collect_paths(
    reader: &dyn ErasedAssetReader,
    path: PathBuf,
    files: &mut Vec<PathBuf>,
    mut empty_folders: Option<&mut Vec<PathBuf>>,
) -> Result<bool, AssetReaderError> {
    if !reader.is_directory(&path).await? {
        if !is_meta_file(&path) {
            files.push(path);
        }

        return Ok(true);
    }

    let mut stream = reader.read_directory(&path).await?;
    let mut holds_files = false;

    while let Some(child) = stream.next().await {
        holds_files |= Box::pin(collect_paths(
            reader,
            child,
            files,
            empty_folders.as_deref_mut(),
        ))
        .await?;
    }

    if !holds_files
        && path.parent().is_some()
        && let Some(empty_folders) = empty_folders
    {
        empty_folders.push(path);
    }

    Ok(holds_files)
}

/// Whether `path` is a `.meta` file: the bookkeeping that lives next to the file it belongs to.
///
/// `.meta` is reserved on both sides of a source — a file with that extension is never an asset of
/// its own — and this is the one place that says so.
#[inline]
fn is_meta_file(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "meta")
}

/// The processed side of `source`: the reader the importer's own bookkeeping comes from, and the
/// writer it may be deleted with.
#[inline]
fn processed_rw_of(
    source: &AssetSource,
) -> Option<(&dyn ErasedAssetReader, &dyn ErasedAssetWriter)> {
    Some((
        source.ungated_processed_reader()?,
        source.processed_writer().ok()?,
    ))
}
