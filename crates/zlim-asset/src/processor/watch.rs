//! Following the source side: what a running importer does when a source changes.
//!
//! One listener per source, each turning events into work for the pass. Additions and modifications are
//! queued again (the task decides whether anything is out of date); a removal throws the processed
//! output away; a rename moves it; a folder event is re-enumerated, which is the conservative answer —
//! a directory stream is the only way to know what is in it.
//!
//! This starts only once the first pass is over: a change that arrives earlier is part of that scan
//! anyway.

use std::path::PathBuf;

use futures_lite::StreamExt;
use zlim_task::IoTaskPool;

use crate::event::AssetSourceEvent;
use crate::io::AssetReaderError;
use crate::path::AssetPath;
use crate::processor::infos::TaskSender;
use crate::processor::processed::report_remove_error;
use crate::processor::processed::report_write_error;
use crate::processor::server::AssetProcessServer;
use crate::source::AssetSource;

// -----------------------------------------------------------------------------
// AssetProcessServer: the listeners

impl AssetProcessServer {
    /// Follows the sources' watchers: every change to a source asset queues it again.
    ///
    /// The *unprocessed* side is what is watched here (the processed side is the app's business), and
    /// each source gets a task of its own that lives as long as the source does. This is only started
    /// once the first pass is over: a change that arrives earlier is part of that pass anyway.
    pub(super) fn spawn_source_change_event_listeners(&self, tasks: &TaskSender) {
        for source in self.data.sources.iter() {
            let Some(receiver) = source.event_receiver().cloned() else {
                continue;
            };

            let this = self.clone();
            let tasks = tasks.clone();
            let source_id = source.id();

            IoTaskPool::get()
                .spawn(async move {
                    let mut receiver = receiver;

                    // The sources are fixed once built, so an id taken from them still resolves.
                    let source = this.data.sources.get(source_id).unwrap();

                    while let Ok(event) = receiver.recv().await {
                        this.handle_asset_source_event(source, event, &tasks).await;
                    }
                })
                .detach();
        }
    }
}

// -----------------------------------------------------------------------------
// AssetProcessServer: one event

impl AssetProcessServer {
    /// Reacts to one source change.
    ///
    /// Additions and modifications are queued (the task decides whether anything is out of date); a
    /// removal throws the processed output away; a rename moves it. Folder events are re-enumerated,
    /// which is the conservative answer: a directory stream is the only way to know what is in it.
    async fn handle_asset_source_event(
        &self,
        source: &AssetSource,
        event: AssetSourceEvent,
        tasks: &TaskSender,
    ) {
        match event {
            AssetSourceEvent::AddedAsset(path)
            | AssetSourceEvent::AddedMeta(path)
            | AssetSourceEvent::ModifiedAsset(path)
            | AssetSourceEvent::ModifiedMeta(path) => {
                let _ = tasks.send((source.id(), path));
            }
            // A removed asset has no source to process any more: only its output goes away.
            AssetSourceEvent::RemovedAsset(path) => {
                self.handle_removed_asset(source, path).await;
            }
            // The asset itself may still be there, and its `.meta` may need to be regenerated.
            AssetSourceEvent::RemovedMeta(path) => {
                let _ = tasks.send((source.id(), path));
            }
            AssetSourceEvent::AddedFolder(path) => {
                self.handle_added_folder(source, path, tasks).await;
            }
            AssetSourceEvent::RemovedFolder(path) => {
                self.handle_removed_folder(source, path).await;
            }
            AssetSourceEvent::RenamedAsset { old, new } => {
                if old == new {
                    let _ = tasks.send((source.id(), new));
                } else {
                    self.handle_renamed_asset(source, old, new, tasks).await;
                }
            }
            // A renamed `.meta` says nothing about the asset it belongs to: both ends are queued.
            AssetSourceEvent::RenamedMeta { old, new } => {
                if old == new {
                    let _ = tasks.send((source.id(), new));
                } else {
                    let _ = tasks.send((source.id(), old));
                    let _ = tasks.send((source.id(), new));
                }
            }
            AssetSourceEvent::RenamedFolder { old, new } => {
                if old == new {
                    self.handle_added_folder(source, new, tasks).await;
                } else {
                    self.handle_removed_folder(source, old).await;
                    self.handle_added_folder(source, new, tasks).await;
                }
            }
            // The watcher could not tell whether what disappeared was a folder or a file.
            AssetSourceEvent::RemovedUnknown { path, is_meta } => {
                let Some(processed_reader) = source.ungated_processed_reader() else {
                    return;
                };

                match processed_reader.is_directory(&path).await {
                    Ok(true) => self.handle_removed_folder(source, path).await,
                    Ok(false) if is_meta => {
                        let _ = tasks.send((source.id(), path));
                    }
                    Ok(false) => self.handle_removed_asset(source, path).await,
                    Err(error) => {
                        if !error.is_not_found() {
                            ::core::hint::cold_path();
                            zlim_log::error!(
                                "Could not tell whether the removed path '{}' was a folder or a file: {error}",
                                path.display()
                            );
                        }
                        // Nothing on the processed side means there was nothing to remove,
                        // which is the normal case for a source that was never processed.
                    }
                }
            }
        }
    }
}

// -----------------------------------------------------------------------------
// AssetProcessServer: folders

impl AssetProcessServer {
    /// Queues every asset under `path`, walking into directories.
    ///
    /// The *unprocessed* reader lists the source side: that is where the assets to process are. A
    /// directory stream lists one level, so this recurses (boxed, because an async recursive
    /// function needs a concrete future size).
    async fn queue_processing_tasks_for_folder(
        &self,
        source: &AssetSource,
        path: PathBuf,
        tasks: &TaskSender,
    ) -> Result<(), AssetReaderError> {
        if source.reader().is_directory(&path).await? {
            let mut stream = source.reader().read_directory(&path).await?;

            while let Some(child) = stream.next().await {
                Box::pin(self.queue_processing_tasks_for_folder(source, child, tasks)).await?;
            }
        } else {
            let _ = tasks.send((source.id(), path));
        }

        Ok(())
    }

    /// Queues everything under `path`, because a folder appeared.
    async fn handle_added_folder(&self, source: &AssetSource, path: PathBuf, tasks: &TaskSender) {
        if let Err(error) = self
            .queue_processing_tasks_for_folder(source, path, tasks)
            .await
        {
            ::core::hint::cold_path();
            zlim_log::error!("Failed to list the added folder '{}': {error}", source.id());
        }
    }

    /// Throws away everything the processed side holds under `path`, because a folder is gone.
    ///
    /// A processed side that cannot be listed is logged and marks the run unrecoverable, so the next
    /// run processes every asset again rather than trusting what it could not read.
    async fn handle_removed_folder(&self, source: &AssetSource, path: PathBuf) {
        let Some(processed_reader) = source.ungated_processed_reader() else {
            return;
        };

        match processed_reader.read_directory(&path).await {
            Ok(mut stream) => {
                while let Some(child) = stream.next().await {
                    self.handle_removed_asset(source, child).await;
                }
            }
            Err(AssetReaderError::NotFound(_)) => {}
            Err(error) => {
                ::core::hint::cold_path();
                zlim_log::error!(
                    "Failed to list the processed side of the removed folder '{}': {error}",
                    path.display()
                );
                self.log_unrecoverable().await;
            }
        }

        let Ok(processed_writer) = source.processed_writer() else {
            return;
        };

        if let Err(error) = processed_writer.remove_directory(&path).await {
            report_remove_error("remove the folder", &path, error);
        }
    }
}

// -----------------------------------------------------------------------------
// AssetProcessServer: assets

impl AssetProcessServer {
    /// Throws away the processed output of one asset, because its source is gone.
    ///
    /// The watcher's form of [`forget_processed_asset`](Self::forget_processed_asset): a removal event
    /// names a path the way the source side does.
    async fn handle_removed_asset(&self, source: &AssetSource, path: PathBuf) {
        let asset_path = AssetPath::from(path).with_source_id(source.id());
        self.forget_processed_asset(source, &asset_path).await;
    }

    /// Moves the processed output of a renamed source asset, and its `.meta` with it.
    async fn handle_renamed_asset(
        &self,
        source: &AssetSource,
        old: PathBuf,
        new: PathBuf,
        tasks: &TaskSender,
    ) {
        let old = AssetPath::from(old).with_source_id(source.id());
        let new = AssetPath::from(new).with_source_id(source.id());

        let locks = {
            let mut infos = self.data.state.write_infos().await;
            infos.rename(&old, &new, tasks).await
        };
        let Some((old_lock, new_lock)) = locks else {
            return;
        };

        // SAFETY: `rename` ensure that the locks are not the same.
        let _old_write_lock = old_lock.write_arc().await;
        let _new_write_lock = new_lock.write_arc().await;

        let Ok(processed_writer) = source.processed_writer() else {
            return;
        };

        if let Err(error) = processed_writer.rename(old.path(), new.path()).await {
            report_write_error("rename", old.path(), error);
        }

        if let Err(error) = processed_writer.rename_meta(old.path(), new.path()).await {
            report_write_error("rename the meta of", old.path(), error);
        }
    }
}
