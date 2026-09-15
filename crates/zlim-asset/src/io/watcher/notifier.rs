//! Debounced filesystem event plumbing shared by the file and embedded watchers.

use core::time::Duration;
use std::path::{Path, PathBuf};

use notify_debouncer_full::notify::event::{AccessKind, AccessMode, CreateKind};
use notify_debouncer_full::notify::event::{ModifyKind, RemoveKind, RenameMode};
use notify_debouncer_full::notify::{self, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use zlim_utils::vec::SmallVec;

use crate::event::AssetSourceEvent;

// -----------------------------------------------------------------------------
// EventNotifier

/// A path reported by a watcher, split into its asset path and whether it was a meta file.
pub struct EventPath {
    /// The asset path the raw path resolves to.
    pub path: PathBuf,
    /// Whether the raw path was a `.meta` sidecar.
    pub is_meta: bool,
}

/// Receives raw debounced filesystem events and turns them into [`AssetSourceEvent`]s.
pub trait EventNotifier: Send + Sync + 'static {
    /// Called once before each batch of events.
    fn begin(&mut self);

    /// Maps an absolute filesystem path onto an asset path.
    fn parse(&self, absolute_path: &Path) -> Option<EventPath>;

    /// Emits one [`AssetSourceEvent`] for `absolute_paths`.
    fn notify(&mut self, absolute_paths: &[PathBuf], event: AssetSourceEvent);
}

// -----------------------------------------------------------------------------
// make_absolute_path

/// Converts the provided path into an absolute one.
pub fn make_absolute_path(path: &Path) -> Result<PathBuf, std::io::Error> {
    // We use `normalize` + `absolute` instead of `canonicalize` to avoid reading the filesystem to
    // resolve the path. This also means that paths that no longer exist can still become absolute
    // (e.g., a file that was renamed will have the "old" path no longer exist).
    let absolute = std::path::absolute(path)?;
    Ok(crate::utils::normalize_path(&absolute))
}

// -----------------------------------------------------------------------------
// build_debouncer

/// Builds a recursive debouncer on `root`, forwarding normalized events to `notifier`.
///
/// A relative `root` is resolved against [`crate::io::file::base_path`], while an absolute one
/// is used as is (joining an absolute path replaces the base).
#[rustfmt::skip]
pub fn build_debouncer(
    root: PathBuf,
    debounce_wait_time: Duration,
    mut notifier: impl EventNotifier,
) -> Result<Debouncer<RecommendedWatcher, RecommendedCache>, notify::Error> {
    let root = crate::io::file::base_path().join(root);

    let event_handler = move |result: DebounceEventResult| {
        let events = match result {
            Ok(events) => events,
            Err(errors) => {
                // The iterator of slice is faster than that of elements.
                for error in errors.iter() {
                    zlim_log::error!("Encountered a filesystem watcher error: {error:?}");
                }
                return;
            }
        };

        notifier.begin();

        for event in events.iter() {
            // A single event usually contains no more than two paths.
            let mut paths = SmallVec::<PathBuf, 2>::with_capacity(event.paths.len());
            for path in event.paths.iter() {
                let absolute_path = make_absolute_path(path);
                paths.push(absolute_path.expect("paths from the debouncer are valid"));
            }

            match event.kind {
                notify::EventKind::Create(CreateKind::File) => {
                    if let Some(e) = notifier.parse(&paths[0]) {
                        if e.is_meta {
                            notifier.notify(&paths, AssetSourceEvent::AddedMeta(e.path));
                        } else {
                            notifier.notify(&paths, AssetSourceEvent::AddedAsset(e.path));
                        }
                    }
                }
                notify::EventKind::Create(CreateKind::Folder) => {
                    if let Some(e) = notifier.parse(&paths[0]) {
                        notifier.notify(&paths, AssetSourceEvent::AddedFolder(e.path));
                    }
                }
                notify::EventKind::Access(AccessKind::Close(AccessMode::Write)) => {
                    if let Some(e) = notifier.parse(&paths[0]) {
                        if e.is_meta {
                            notifier.notify(&paths, AssetSourceEvent::ModifiedMeta(e.path));
                        } else {
                            notifier.notify(&paths, AssetSourceEvent::ModifiedAsset(e.path));
                        }
                    }
                }
                // Because this is debounced over a reasonable period of time, Modify(ModifyKind::Name(RenameMode::From)
                // events are assumed to be "dangling" without a follow up "To" event. Without debouncing, "From" -> "To" -> "Both"
                // events are emitted for renames. If a From is dangling, it is assumed to be "removed" from the context of the asset
                // system.
                notify::EventKind::Modify(ModifyKind::Name(RenameMode::From))
                | notify::EventKind::Remove(RemoveKind::Any) => {
                    if let Some(e) = notifier.parse(&paths[0]) {
                        let path = e.path;
                        let is_meta = e.is_meta;
                        notifier.notify(&paths, AssetSourceEvent::RemovedUnknown { path, is_meta });
                    }
                }
                notify::EventKind::Modify(ModifyKind::Name(RenameMode::To))
                | notify::EventKind::Create(CreateKind::Any) => {
                    if let Some(e) = notifier.parse(&paths[0]) {
                        let asset_event = if paths[0].is_dir() {
                            AssetSourceEvent::AddedFolder(e.path)
                        } else if e.is_meta {
                            AssetSourceEvent::AddedMeta(e.path)
                        } else {
                            AssetSourceEvent::AddedAsset(e.path)
                        };
                        notifier.notify(&paths, asset_event);
                    }
                }
                notify::EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                    let Some(old_e) = notifier.parse(&paths[0]) else {
                        continue;
                    };
                    let Some(new_e) = notifier.parse(&paths[1]) else {
                        continue;
                    };
                    let old = old_e.path;
                    let new = new_e.path;
                    let old_is_meta = old_e.is_meta;
                    let new_is_meta = new_e.is_meta;

                    // only the new "real" path is considered a directory
                    if paths[1].is_dir() {
                        notifier.notify(&paths, AssetSourceEvent::RenamedFolder { old, new });
                    } else {
                        match (old_is_meta, new_is_meta) {
                            (true, true) => {
                                notifier.notify(&paths, AssetSourceEvent::RenamedMeta { old, new });
                            }
                            (false, false) => {
                                notifier
                                    .notify(&paths, AssetSourceEvent::RenamedAsset { old, new });
                            }
                            (true, false) => {
                                zlim_log::error!(
                                    "Asset metafile {old:?} was changed to asset file {new:?}, which is not supported. \
                                    Try restarting your app to see if configuration is still valid"
                                );
                            }
                            (false, true) => {
                                zlim_log::error!(
                                    "Asset file {old:?} was changed to meta file {new:?}, which is not supported. \
                                    Try restarting your app to see if configuration is still valid"
                                );
                            }
                        }
                    }
                }
                notify::EventKind::Modify(_) => {
                    let Some(e) = notifier.parse(&paths[0]) else {
                        continue;
                    };
                    if paths[0].is_dir() {
                        // modified folder means nothing in this case
                    } else if e.is_meta {
                        notifier.notify(&paths, AssetSourceEvent::ModifiedMeta(e.path));
                    } else {
                        notifier.notify(&paths, AssetSourceEvent::ModifiedAsset(e.path));
                    };
                }
                notify::EventKind::Remove(RemoveKind::File) => {
                    let Some(e) = notifier.parse(&paths[0]) else {
                        continue;
                    };
                    if e.is_meta {
                        notifier.notify(&paths, AssetSourceEvent::RemovedMeta(e.path));
                    } else {
                        notifier.notify(&paths, AssetSourceEvent::RemovedAsset(e.path));
                    }
                }
                notify::EventKind::Remove(RemoveKind::Folder) => {
                    let Some(e) = notifier.parse(&paths[0]) else {
                        continue;
                    };
                    notifier.notify(&paths, AssetSourceEvent::RemovedFolder(e.path));
                }
                _ => {}
            }
        }
    };

    let mut debouncer = new_debouncer(debounce_wait_time, None, event_handler)?;
    debouncer.watch(&root, RecursiveMode::Recursive)?;
    Ok(debouncer)
}
