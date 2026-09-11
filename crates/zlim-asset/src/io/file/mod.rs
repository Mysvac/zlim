//! I/O implementation for the local filesystem.
//!
//! This asset I/O is fully featured but it's not available on `android` and `wasm` targets.
//!
//! - On android, this module will be compiled but it's unused.
//! - On wasm32, this module will not be compiled, and cannot be used.

zlim_task::cfg::single_thread! {
    mod sync_asset;
}

zlim_task::cfg::multi_thread! {
    mod async_asset;
}

// -----------------------------------------------------------------------------
// base_path

use std::path::{Path, PathBuf};

/// Returns the asset root the file source resolves relative paths against.
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
    /// Creates a new [`FileAssetReader`] at a path relative to the executable's directory.
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
    /// Which is normally the executable's parent directory.
    /// (e.g. for `foo/main.exe`, this is `foo`).
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
    /// Creates a new [`FileAssetWriter`] at a path relative to the executable's directory.
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
    /// Which is normally the executable's parent directory.
    /// (e.g. for `foo/main.exe`, this is `foo`).
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
