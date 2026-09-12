//! Source-file watcher that refreshes embedded assets in memory.

use core::time::Duration;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};

use notify_debouncer_full::notify::RecommendedWatcher;
use notify_debouncer_full::{Debouncer, RecommendedCache, notify};
use zlim_utils::hash::HashMap;
use zlim_utils::mpsc::Sender;

use super::AssetWatcher;
use super::notifier::build_debouncer;
use super::notifier::{EventNotifier, EventPath};
use crate::event::AssetSourceEvent;
use crate::io::memory::Dir;

// -----------------------------------------------------------------------------
// EmbeddedNotifier

struct EmbeddedNotifier {
    dir: Dir,
    root: PathBuf,
    sender: Sender<AssetSourceEvent>,
    last_event: Option<AssetSourceEvent>,
    root_paths: Arc<RwLock<HashMap<Box<Path>, PathBuf>>>,
}

impl EventNotifier for EmbeddedNotifier {
    fn begin(&mut self) {
        self.last_event = None;
    }

    fn parse(&self, absolute_path: &Path) -> Option<EventPath> {
        let root = &self.root;

        let Ok(relative_path) = absolute_path.strip_prefix(root) else {
            strip_prefix_faild(absolute_path, root);
            return None;
        };

        let is_meta = relative_path.extension().is_some_and(|e| e == "meta");

        let local_path = if is_meta {
            relative_path.with_extension("")
        } else {
            relative_path.to_path_buf()
        };

        let path = self
            .root_paths
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(local_path.as_path())?
            .clone();

        Some(EventPath { path, is_meta })
    }

    fn notify(&mut self, absolute_paths: &[PathBuf], event: AssetSourceEvent) {
        if self.last_event.as_ref() != Some(&event) {
            if let AssetSourceEvent::ModifiedAsset(path) = &event
                && let Ok(file) = File::open(&absolute_paths[0])
            {
                let mut reader = BufReader::new(file);
                let mut buffer = Vec::new();

                // Read file into vector.
                if reader.read_to_end(&mut buffer).is_ok() {
                    let value = Arc::<[u8]>::from(buffer.as_slice());
                    self.dir.insert_asset(path, value);
                }
            }

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
fn strip_prefix_faild<'a>(absolute_path: &'a Path, root: &'a Path) -> Option<&'a Path> {
    // Should not happen.
    zlim_log::error!(
        "EmbeddedNotifier::parse() failed to strip prefix: absolute_path={}, root={}.",
        absolute_path.display(),
        root.display(),
    );
    None
}

// -----------------------------------------------------------------------------
// EmbeddedWatcher

/// A watcher over the source files of embedded assets.
pub struct EmbeddedWatcher {
    _watcher: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl EmbeddedWatcher {
    /// Creates a watcher on the workspace root, refreshing `dir` from `root_paths`.
    pub fn build(
        dir: Dir,
        root_paths: Arc<RwLock<HashMap<Box<Path>, PathBuf>>>,
        sender: Sender<AssetSourceEvent>,
        debounce_wait_time: Duration,
    ) -> Option<Box<dyn AssetWatcher>> {
        let root = crate::io::file::base_path();

        let notifier = EmbeddedNotifier {
            dir,
            root: root.clone(),
            sender,
            root_paths,
            last_event: None,
        };

        match build_debouncer(root, debounce_wait_time, notifier) {
            Ok(watcher) => Some(Box::new(EmbeddedWatcher { _watcher: watcher })),
            Err(error) => watch_failed(error),
        }
    }
}

impl AssetWatcher for EmbeddedWatcher {}

#[cold]
fn watch_failed(error: notify::Error) -> Option<Box<dyn AssetWatcher>> {
    zlim_log::error!(
        "Create EmbeddedWatcher failed `{error}`, embedded assets hot-reload cannot work."
    );
    None
}
