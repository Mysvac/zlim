//! The processed-side index: what the importer knows about every asset it has an output for.
//!
//! [`ProcessorAssetInfos`] is the in-memory mirror of the processed side, built by reading the
//! `.meta` files an earlier run wrote. It exists for one question — *is this asset still up to
//! date?* — and it is what makes that question exact.
//!
//! Without it, the only way to check a recorded process dependency is to hash the dependency's
//! *source* again. That works for a dependency that is processed from its own bytes and nothing
//! else, because its recorded `full_hash` is then just that hash. It fails as soon as the dependency
//! has process dependencies of its own: its `full_hash` is folded from theirs, so it can never equal
//! a plain source hash — and every asset that depends on it would be processed again on every run.
//!
//! The index answers the same question with the value the dependency itself recorded
//! ([`ProcessedInfo::full_hash`]), so a chain of any depth is compared, not re-derived. What it
//! cannot answer is a dependency that has no processed output at all: there is nothing to compare
//! against, so the asset that needs it counts as out of date until that changes — that is the same
//! conservative direction as before, but now it only applies where it is really unknown.

use std::path::PathBuf;
use std::sync::Arc;

use async_broadcast::{Receiver, Sender};
use async_lock::RwLock;
use zlim_utils::hash::{HashMap, HashSet};

use crate::error::{AssetError, AssetProcessError};
use crate::ident::AssetSourceId;
use crate::meta::{AssetHash, ProcessedInfo};
use crate::path::AssetPath;
use crate::processor::ProcessStatus;

// -----------------------------------------------------------------------------
// Task

/// One unit of work for the importer: an asset of a source, named the way a source names files.
///
/// The source is carried along because a path alone does not say which source it belongs to — the
/// same relative path can exist in several of them.
pub(crate) type Task = (AssetSourceId, PathBuf);

/// Queues [`Task`]s for the importer.
pub(crate) type TaskSender = zlim_utils::mpmc::Sender<Task>;

/// The receiving end of the same queue.
pub(crate) type TaskReceiver = zlim_utils::mpmc::Receiver<Task>;

/// Turns an asset path into the task that processes it.
#[inline]
pub(crate) fn queued_task(path: &AssetPath<'static>) -> Task {
    (path.source_id(), path.path().to_path_buf())
}

// -----------------------------------------------------------------------------
// ProcessResult

/// How a single asset came out of the processing step, before it is recorded.
#[derive(Clone, Debug)]
pub(crate) enum ProcessResult {
    /// The asset was processed: this is the [`ProcessedInfo`] that was written.
    Processed(ProcessedInfo),
    /// The processed output is up to date, so nothing was written.
    SkippedNotChanged,
    /// The asset is deliberately not processed (an `.meta` that says `Ignore`).
    Ignored,
}

// -----------------------------------------------------------------------------
// ProcessorAssetInfo

/// What the importer knows about one asset.
#[derive(Debug)]
// NOTE: if you add fields to this struct, make sure they are propagated (when relevant) if an
// asset is ever moved or removed — that is what `ProcessorAssetInfos` would have to update.
pub(crate) struct ProcessorAssetInfo {
    /// The [`ProcessedInfo`] the processed `.meta` carries, when there is one.
    pub(crate) processed_info: Option<ProcessedInfo>,

    /// The assets that recorded this one as a process dependency.
    ///
    /// This is the reverse of [`ProcessedInfo::process_dependencies`], and it is what lets a run
    /// tell which assets have to be looked at again after this one was processed.
    pub(crate) dependents: HashSet<AssetPath<'static>>,

    /// How this asset came out of the run, once it has.
    pub(crate) status: Option<ProcessStatus>,

    /// A lock over this asset's processed files — the bytes and the `.meta` alike.
    ///
    /// The importer holds it for writing while it rewrites or deletes the pair, and the processed-side
    /// gate holds it for reading while it hands the files out, which is what keeps a reader from
    /// seeing new bytes next to an old `.meta` (or half of a written file). One lock for both files is
    /// the point: they are one revision.
    pub(crate) file_transaction_lock: Arc<RwLock<()>>,

    /// Broadcasts [`Self::status`] to whoever waits on this asset.
    ///
    /// One slot with overflow, so a late waiter picks up the latest status instead of parking, and
    /// the importer never blocks on a waiter that stopped listening.
    pub(crate) status_sender: Sender<ProcessStatus>,
    pub(crate) status_receiver: Receiver<ProcessStatus>,
}

impl Default for ProcessorAssetInfo {
    fn default() -> Self {
        let (mut status_sender, status_receiver) = async_broadcast::broadcast(1);
        status_sender.set_overflow(true);

        Self {
            processed_info: None,
            dependents: HashSet::new(),
            status: None,
            file_transaction_lock: Arc::new(RwLock::new(())),
            status_sender,
            status_receiver,
        }
    }
}

impl ProcessorAssetInfo {
    /// Records `status` and wakes everyone waiting on this asset.
    pub(crate) async fn update_status(&mut self, status: ProcessStatus) {
        self.status = Some(status);

        // The channel always has the receiver this struct holds, so it cannot be closed.
        let _ = self.status_sender.broadcast(status).await;
    }
}

// -----------------------------------------------------------------------------
// ProcessorAssetInfos

/// The index of every asset the importer has looked at, keyed by its source path.
#[derive(Debug, Default)]
pub(crate) struct ProcessorAssetInfos {
    infos: HashMap<AssetPath<'static>, ProcessorAssetInfo>,

    /// The dependents of assets that have no entry in `infos` (yet).
    ///
    /// A dependency can be recorded before the run knows about the dependency itself (it can live
    /// outside the sources being scanned), so its dependents are parked here and moved over as soon
    /// as it is inserted. This keeps the two consistent: a dependent is never lost.
    non_existent_dependents: HashMap<AssetPath<'static>, HashSet<AssetPath<'static>>>,
}

impl ProcessorAssetInfos {
    /// Returns the entry for `path`, creating it — and adopting any dependents that were parked
    /// while it did not exist.
    pub(crate) fn get_or_insert(&mut self, path: &AssetPath<'static>) -> &mut ProcessorAssetInfo {
        let parked = self.non_existent_dependents.remove(path);
        let info = self.infos.entry(path.clone()).or_default();

        if let Some(dependents) = parked {
            info.dependents.extend(dependents);
        }

        info
    }

    /// Returns the entry for `path`, if the importer knows about it.
    pub(crate) fn get(&self, path: &AssetPath<'static>) -> Option<&ProcessorAssetInfo> {
        self.infos.get(path)
    }

    /// Records the outcome of processing one asset, and requeues what it affects.
    ///
    /// This is the one place a result becomes state:
    ///
    /// - a processed asset is [`record`](Self::record)ed — its dependency edges are rebuilt and its
    ///   [`ProcessedInfo`] replaced — and only then is its status announced and every asset that
    ///   depends on it queued again, because the `full_hash` they recorded may have just changed;
    /// - an unchanged asset only has its status announced (its dependents are deliberately left
    ///   alone: they are, as far as anything recorded says, still up to date);
    /// - a failure is announced as `Failed`; one reported as an [`AssetLoaderError`] is parked as a
    ///   dangling dependent of the path that error names, with a zeroed [`ProcessedInfo`] so it can
    ///   never pass the up-to-date check, and it is queued again by that path's own success;
    /// - an asset nothing can process — an `.meta` that says `Ignore`, or a source with no extension
    ///   for a processor to be picked by — is announced as `Failed` too; nothing failed, so it is only
    ///   logged.
    ///
    /// [`AssetLoaderError`]: crate::error::AssetLoaderError
    pub(crate) async fn finish_processing(
        &mut self,
        path: AssetPath<'static>,
        result: Result<ProcessResult, AssetProcessError>,
        reprocess: Option<&TaskSender>,
    ) {
        match result {
            Ok(ProcessResult::Processed(processed_info)) => {
                // The record and its dependency edges are rebuilt the way the scan does it when it
                // reads the processed side back: `record` is the one place that keeps the index's
                // forward hashes and its reverse edges in step.
                let info = self.record(&path, Some(processed_info));

                // What a *finished* asset needs on top of that: its own status, and the assets that
                // recorded it as a dependency looked at again.
                info.update_status(ProcessStatus::Processed).await;

                if let Some(reprocess) = reprocess {
                    for dependent in self.dependents_of(&path) {
                        let _ = reprocess.send(queued_task(dependent));
                    }
                }
            }
            Ok(ProcessResult::SkippedNotChanged) => {
                zlim_log::trace!("Skipping processing (unchanged) '{path}'");

                self.get_or_insert(&path)
                    .update_status(ProcessStatus::Processed)
                    .await;
            }
            Ok(ProcessResult::Ignored) => {
                zlim_log::trace!("Skipping processing (ignored) '{path}'");

                // The asset is deliberately not processed, so there is no output to read: recording
                // `Failed` is what makes a read of it report "not found" instead of waiting for an
                // output that will never come. Nothing failed here, so there is no error to log
                // either — the asset has no processor and no loader.
                self.get_or_insert(&path)
                    .update_status(ProcessStatus::Failed)
                    .await;
            }
            Err(error) => {
                // An asset nothing can process — because it has no extension for a processor to be
                // picked by — is not a failure: it is simply not processed, and it is only logged.
                // (The reference implementation ignores this error; here the asset still gets a
                // status, so a read of it is answered with "no output" instead of waiting for one
                // that will never come.)
                if matches!(&*error.0, AssetError::ExtensionRequired(_)) {
                    zlim_log::trace!("Skipping processing (no extension) '{path}'");

                    self.get_or_insert(&path)
                        .update_status(ProcessStatus::Failed)
                        .await;
                } else {
                    self.record_failure(path, error).await;
                }
            }
        }
    }

    /// Records that processing `path` failed, and requeues it if a missing *dependency* is why.
    async fn record_failure(&mut self, path: AssetPath<'static>, error: AssetProcessError) {
        // A source that disappeared between the scan and this pass is not an error — there is
        // simply nothing to process — so it is only logged in debug, like the reference
        // implementation. Everything else is reported.
        if matches!(
            &*error.0,
            AssetError::AssetReaderError(reader_error) if reader_error.is_not_found()
        ) {
            ::core::hint::cold_path();
            zlim_log::trace!("No need to process '{path}' because it does not exist any more");
        } else {
            ::core::hint::cold_path();
            zlim_log::error!("Failed to process '{path}': {error}");
        }

        // A loader error carries the path it could not load, and for a processor reading a
        // *process dependency* that path is the dependency: the asset is parked on it and
        // queued again once that path is processed. The reference implementation reads the
        // same field rather than digging for the dependency that was actually missing.
        if let AssetError::AssetLoaderError(loader_error) = &*error.0 {
            let missing = loader_error.path.clone();

            let info = self.get_or_insert(&path);
            info.processed_info = Some(ProcessedInfo {
                hash: AssetHash::ZERO,
                full_hash: AssetHash::ZERO,
                process_dependencies: Vec::new(),
            });
            self.add_dependent(&missing, path.clone());
        }

        self.get_or_insert(&path)
            .update_status(ProcessStatus::Failed)
            .await;
    }

    /// Forgets `path`, because its source is gone.
    ///
    /// Everything waiting on it is told it does not exist, and its dependents are parked on the
    /// missing path (they are kept, not dropped: if the asset comes back, they are looked at again).
    /// The transaction lock is returned so the caller can wait until whoever reads the processed
    /// files is done before deleting them. `None` means the importer has no entry for `path` at all:
    /// nothing was recorded for it, so there is nothing to answer and no reader to wait for.
    pub(crate) async fn remove(&mut self, path: &AssetPath<'static>) -> Option<Arc<RwLock<()>>> {
        let info = self.infos.remove(path)?;

        if let Some(processed_info) = &info.processed_info {
            self.clear_dependencies(path, processed_info);
        }

        let _ = info
            .status_sender
            .broadcast(ProcessStatus::NonExistent)
            .await;

        if !info.dependents.is_empty() {
            ::core::hint::cold_path();
            zlim_log::error!(
                "The asset at '{path}' was removed, but other assets depend on it to \
                 be processed; check whether they should point somewhere else: {:?}",
                info.dependents
            );

            self.non_existent_dependents
                .insert(path.clone(), info.dependents);
        }

        Some(info.file_transaction_lock)
    }

    /// Moves `old` to `new`, because its source was renamed.
    ///
    /// The dependency edges of the moved asset are rewritten, waiters on either path are told what
    /// happened, and both the new path and its dependents are queued again — the moved asset needs
    /// its `.meta` to be re-read, and its dependents may have been waiting for it.
    ///
    /// Returns the transaction locks of the old and the new path, so the caller can wait for readers
    /// of both before moving the processed files. `None` means the importer has no entry for `old`:
    /// nothing was recorded for it, so nothing is moved and no reader has to be waited for.
    pub(crate) async fn rename(
        &mut self,
        old: &AssetPath<'static>,
        new: &AssetPath<'static>,
        requeue: &TaskSender,
    ) -> Option<(Arc<RwLock<()>>, Arc<RwLock<()>>)> {
        let mut info = self.infos.remove(old)?;

        if !info.dependents.is_empty() {
            ::core::hint::cold_path();
            zlim_log::error!(
                "The asset at '{old}' was renamed, but other assets depend on it to  \
                 be processed; check whether they should point somewhere else: {:?}",
                info.dependents
            );

            self.non_existent_dependents
                .insert(old.clone(), ::core::mem::take(&mut info.dependents));
        }

        if let Some(processed_info) = &info.processed_info {
            for dependency in &processed_info.process_dependencies {
                if let Some(dep_info) = self.infos.get_mut(&dependency.path) {
                    dep_info.dependents.remove(old);
                    dep_info.dependents.insert(new.clone());
                } else if let Some(dependents) =
                    self.non_existent_dependents.get_mut(&dependency.path)
                {
                    dependents.remove(old);
                    dependents.insert(new.clone());
                }
            }
        }

        let _ = info
            .status_sender
            .broadcast(ProcessStatus::NonExistent)
            .await;

        let new_info = self.get_or_insert(new);
        new_info.processed_info = info.processed_info;
        new_info.status = info.status;
        let new_lock = new_info.file_transaction_lock.clone();

        // Whoever waits on the new path has to learn the status the old path ended with.
        if let Some(status) = new_info.status {
            let _ = new_info.status_sender.broadcast(status).await;
        }

        let _ = requeue.send(queued_task(new));

        for dependent in self.dependents_of(new) {
            let _ = requeue.send(queued_task(dependent));
        }

        Some((info.file_transaction_lock, new_lock))
    }

    /// Records what the processed side says about `path`.
    ///
    /// The dependency edges of the previous record are dropped first, so a re-processed asset that
    /// no longer reads a side file stops being a dependent of it. `None` records "there is no
    /// processed output for this asset" — the state a failed or never-processed asset is in.
    ///
    /// The entry is returned, so a caller that has more to do with the asset — the pass, which
    /// announces its status — does not have to look it up again.
    pub(crate) fn record(
        &mut self,
        path: &AssetPath<'static>,
        processed_info: Option<ProcessedInfo>,
    ) -> &mut ProcessorAssetInfo {
        // Detach the old edges before the new ones are attached.
        let old = self.get_or_insert(path).processed_info.take();

        if let Some(old) = &old {
            for dependency in &old.process_dependencies {
                self.remove_dependent(&dependency.path, path);
            }
        }

        if let Some(processed_info) = &processed_info {
            for dependency in &processed_info.process_dependencies {
                self.add_dependent(&dependency.path, path.clone());
            }
        }

        let info = self.get_or_insert(path);
        info.processed_info = processed_info;
        info
    }

    /// Returns the assets that recorded `path` as a process dependency.
    pub(crate) fn dependents_of<'a>(
        &'a self,
        path: &AssetPath<'static>,
    ) -> impl Iterator<Item = &'a AssetPath<'static>> {
        self.infos
            .get(path)
            .map(|info| info.dependents.iter())
            .unwrap_or_default()
    }

    /// Returns whether the output recorded for `path` still matches its source.
    ///
    /// That is: the source hash is the one the record was made from, and every recorded process
    /// dependency still has the `full_hash` the index knows for it. A dependency with no entry, or
    /// with no processed output, makes this `false`: nothing proves it is unchanged.
    pub(crate) fn is_up_to_date(&self, path: &AssetPath<'static>, source_hash: AssetHash) -> bool {
        let Some(processed_info) = self
            .infos
            .get(path)
            .and_then(|info| info.processed_info.as_ref())
        else {
            return false;
        };

        if processed_info.hash != source_hash {
            return false;
        }

        processed_info
            .process_dependencies
            .iter()
            .all(|dependency| {
                self.infos
                    .get(&dependency.path)
                    .and_then(|info| info.processed_info.as_ref())
                    .map(|info| &info.full_hash)
                    == Some(&dependency.full_hash)
            })
    }

    /// Marks `dependent` as an asset that reads `dependency`.
    fn add_dependent(&mut self, dependency: &AssetPath<'static>, dependent: AssetPath<'static>) {
        if let Some(info) = self.infos.get_mut(dependency) {
            info.dependents.insert(dependent);
        } else {
            self.non_existent_dependents
                .entry(dependency.clone())
                .or_default()
                .insert(dependent);
        }
    }

    /// Drops every `asset` → dependency edge recorded in `processed_info`.
    fn clear_dependencies(&mut self, asset: &AssetPath<'static>, processed_info: &ProcessedInfo) {
        for dependency in &processed_info.process_dependencies {
            self.remove_dependent(&dependency.path, asset);
        }
    }

    /// Removes the `dependent` → `dependency` edge recorded earlier.
    fn remove_dependent(
        &mut self,
        dependency: &AssetPath<'static>,
        dependent: &AssetPath<'static>,
    ) {
        if let Some(info) = self.infos.get_mut(dependency) {
            info.dependents.remove(dependent);
        } else if let Some(dependents) = self.non_existent_dependents.get_mut(dependency) {
            dependents.remove(dependent);
        }
    }
}
