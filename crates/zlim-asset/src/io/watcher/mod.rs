//! Asset source watcher abstraction.
//!
//! The actual watchers are compiled with the `notify` feature (built on
//! `notify-debouncer-full`), and only on platforms that can watch files — wasm and Android
//! only get the [`AssetWatcher`] marker trait.

// use `cfg_select` to support rustfmt
cfg_select! {
    not(feature = "notify") => {}
    any(target_os = "windows", target_os = "linux", target_os = "macos") => {
        mod notifier;

        mod file;
        mod embed;

        pub use file::FileWatcher;
        pub use embed::EmbeddedWatcher;
    }
    _ => {}
}

/// A handle to an "asset watcher" process, that will listen for and emit [`AssetSourceEvent`]
/// values for as long as the watcher has not been dropped.
///
/// Implemented by `FileWatcher` and `EmbeddedWatcher` when the `notify` feature is enabled.
/// Dropping the handle stops the watcher and closes its event channel.
///
/// [`AssetSourceEvent`]: crate::event::AssetSourceEvent
pub trait AssetWatcher: Send + Sync + 'static {}
