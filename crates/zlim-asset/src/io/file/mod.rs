//! I/O implementation for the local filesystem.
//!
//! This asset I/O is fully featured, but it is unused on `android`
//! and not compiled on `wasm` targets.
//!
//! - On android, this module will be compiled but it's unused.
//! - On wasm32, this module will not be compiled, and cannot be used.

// -----------------------------------------------------------------------------
// Module Selection

zlim_task::cfg::single_thread! {
    mod sync_asset;
}

zlim_task::cfg::multi_thread! {
    mod async_asset;
}

// -----------------------------------------------------------------------------
// async_fs::File

mod async_file {
    use crate::io::future::ReadAllFuture;
    use crate::io::{Reader, ReaderNotSeekableError, SeekableReader};
    use async_fs::File;

    impl Reader for File {
        #[inline(always)]
        fn seekable(&mut self) -> Result<&mut dyn SeekableReader, ReaderNotSeekableError> {
            Ok(self)
        }

        #[inline(always)]
        fn read_all_bytes<'a>(&'a mut self, buf: &'a mut Vec<u8>) -> ReadAllFuture<'a> {
            ReadAllFuture::async_read::<File>(self, buf)
        }
    }
}

// -----------------------------------------------------------------------------
// base_path

use std::path::{Path, PathBuf};

/// Returns the asset root the file source resolves relative paths against.
///
/// In order of precedence the root is `ZLIM_ASSET_ROOT`, then `CARGO_MANIFEST_DIR` (the crate
/// cargo is building), then the directory containing the current executable — so under
/// `cargo run` the executable's directory is normally not involved.
pub fn base_path() -> PathBuf {
    if let Ok(manifest_dir) = std::env::var("ZLIM_ASSET_ROOT") {
        PathBuf::from(manifest_dir)
    } else if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        PathBuf::from(manifest_dir)
    } else {
        std::env::current_exe()
            .unwrap()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
    }
}

// -----------------------------------------------------------------------------
// FileAssetReader

/// A simple file reader.
///
/// Managed by asset plugin, users do not need to use this.
#[derive(Debug, Clone)]
pub struct FileAssetReader {
    root_path: PathBuf,
}

impl FileAssetReader {
    /// Creates a new [`FileAssetReader`] at `path`, relative to [`base_path`].
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let root_path = base_path().join(path.as_ref());
        zlim_log::debug!(
            "Asset Server using {} as its base path.",
            root_path.display()
        );
        Self { root_path }
    }

    /// Returns the base path of the assets directory.
    ///
    /// See [`base_path`] for the order in which it is chosen.
    #[inline]
    pub fn base_path() -> PathBuf {
        base_path()
    }

    /// Returns the root directory where assets are loaded from.
    #[inline]
    pub fn root_path(&self) -> &PathBuf {
        &self.root_path
    }
}

// -----------------------------------------------------------------------------
// FileAssetWriter

/// A file writer.
///
/// Managed by asset plugin, users do not need to use this.
#[derive(Debug, Clone)]
pub struct FileAssetWriter {
    root_path: PathBuf,
}

impl FileAssetWriter {
    /// Creates a new [`FileAssetWriter`] at `path`, relative to [`base_path`].
    ///
    /// When `create_root` is `true` the root directory is created up front; a failure to do so
    /// is logged and otherwise ignored.
    pub fn new<P: AsRef<Path>>(path: P, create_root: bool) -> Self {
        let root_path = base_path().join(path.as_ref());
        if create_root && let Err(e) = std::fs::create_dir_all(&root_path) {
            zlim_log::error!(
                "Failed to create root directory {} for file asset writer: {e}",
                root_path.display(),
            );
        }
        Self { root_path }
    }

    /// Returns the base path of the assets directory.
    ///
    /// See [`base_path`] for the order in which it is chosen.
    #[inline]
    pub fn base_path() -> PathBuf {
        base_path()
    }

    /// Returns the root directory where this writer stores assets.
    #[inline]
    pub fn root_path(&self) -> &PathBuf {
        &self.root_path
    }
}
