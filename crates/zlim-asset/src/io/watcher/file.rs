//! Filesystem watcher for file assets and its event notifier.

use core::time::Duration;
use std::path::{Path, PathBuf};

use notify_debouncer_full::notify::RecommendedWatcher;
use notify_debouncer_full::{Debouncer, RecommendedCache};
use zlim_utils::mpmc::Sender;

use super::AssetWatcher;
use super::notifier::{EventNotifier, EventPath};
use super::notifier::{build_debouncer, make_absolute_path};
use crate::event::AssetSourceEvent;

// -----------------------------------------------------------------------------
// FileEventNotifier

struct FileEventNotifier {
    root: PathBuf,
    sender: Sender<AssetSourceEvent>,
    last_event: Option<AssetSourceEvent>,
}

impl EventNotifier for FileEventNotifier {
    fn begin(&mut self) {
        self.last_event = None;
    }

    fn parse(&self, absolute_path: &Path) -> Option<EventPath> {
        let root = &self.root;

        let Ok(relative_path) = absolute_path.strip_prefix(root) else {
            strip_prefix_failed(absolute_path, root);
            return None;
        };

        let is_meta = relative_path.extension().is_some_and(|e| e == "meta");

        let path = if is_meta {
            relative_path.with_extension("")
        } else {
            relative_path.to_path_buf()
        };

        Some(EventPath { path, is_meta })
    }

    fn notify(&mut self, _absolute_paths: &[PathBuf], event: AssetSourceEvent) {
        if self.last_event.as_ref() != Some(&event) {
            self.last_event = Some(event.clone());
            if self.sender.send(event).is_err() {
                ::core::hint::cold_path();
                zlim_log::error!("the watcher receiver is dropped but notifier is alive");
            }
        }
    }
}

#[cold]
#[inline(never)]
fn strip_prefix_failed(absolute_path: &Path, root: &Path) {
    // Should not happen.
    zlim_log::error!(
        "FileEventNotifier::parse() failed to strip prefix: absolute_path={}, root={}",
        absolute_path.display(),
        root.display(),
    );
}

// -----------------------------------------------------------------------------
// FileWatcher

/// A watcher over the local filesystem.
pub struct FileWatcher {
    _watcher: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl FileWatcher {
    /// Creates a watcher on `path`, emitting into `sender`.
    ///
    /// Events are debounced by `debounce_wait_time`. Returns `None` when the watcher cannot be
    /// created — the reason is logged, and file assets then have no hot-reload.
    pub fn build(
        path: PathBuf,
        sender: Sender<AssetSourceEvent>,
        debounce_wait_time: Duration,
    ) -> Option<Box<dyn AssetWatcher>> {
        let root = match make_absolute_path(&path) {
            Ok(r) => r,
            Err(err) => return watch_failed(err),
        };

        let notifier = FileEventNotifier {
            root: root.clone(),
            sender,
            last_event: None,
        };

        match build_debouncer(root, debounce_wait_time, notifier) {
            Ok(watcher) => Some(Box::new(FileWatcher { _watcher: watcher })),
            Err(error) => watch_failed(error),
        }
    }
}

impl AssetWatcher for FileWatcher {}

#[cold]
fn watch_failed(err: impl core::error::Error) -> Option<Box<dyn AssetWatcher>> {
    zlim_log::error!("Create FileWatcher failed, file assets hot-reload cannot work: {err}.");
    None
}
