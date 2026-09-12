//! The `embedded://` asset source: compile-time bytes registered into an in-memory tree.

// -----------------------------------------------------------------------------
// EmbeddedAssetRegistry

mod registry;

/// A [`Resource`] that manages embedded assets in a virtual in memory [`Dir`].
///
/// Generally this should not be interacted with directly: the [`embedded_asset!`]
/// macro populates it, and [`register_source`] turns it into the `embedded` asset source.
/// Loading by name (`load_embedded_asset!`) needs `AssetServer` and is still pending.
///
/// [`Dir`]: crate::io::memory::Dir
/// [`embedded_asset!`]: crate::embedded_asset
/// [`Resource`]: trait@zlim_core::resource::Resource
/// [`register_source`]: EmbeddedAssetRegistry::register_source
#[doc(inline)]
pub use registry::EmbeddedAssetRegistry;

// -----------------------------------------------------------------------------
// Constant Value

/// The name of the `embedded` [`AssetSource`].
///
/// [`AssetSource`]: crate::source::AssetSource
pub const EMBEDDED: &str = "embedded";

// -----------------------------------------------------------------------------
// Macros

/// Returns the [`Path`] of an `embedded` asset, following the rules of
/// [`embedded_asset!`](crate::embedded_asset).
#[macro_export]
macro_rules! embedded_path {
    ($path_str: expr) => {{ $crate::embedded_path!("src", $path_str) }};
    ($source_path: expr, $path_str: expr) => {{
        $crate::io::embedded::__embedded_asset_path(
            ::core::file!(),
            ::core::env!("CARGO_CRATE_NAME"),
            $source_path.as_ref(),
            $path_str.as_ref(),
        )
    }};
}

/// Embeds the bytes of `$path` into the binary and registers them with the `embedded` source.
///
/// The generated asset path is `$crate_name/` + the path past the first `$source_path/`
/// (=`src` by default) component of the calling file.
///
/// TODO(server): `load_embedded_asset!`-style loading needs `AssetServer`, which lands in M2.
#[macro_export]
macro_rules! embedded_asset {
    ($app: expr, $path: expr) => {
        $crate::embedded_asset!($app, "src", $path)
    };
    ($app: expr, $source_path: expr, $path: expr) => {{
        let mut embedded = $app
            .world_mut()
            .get_resource_mut::<$crate::io::embedded::EmbeddedAssetRegistry>()
            .expect("`EmbeddedAssetRegistry` must be inserted before using `embedded_asset!`");
        let path = $crate::embedded_path!($source_path, $path);
        let watched_path = $crate::io::embedded::watched_path(file!(), $path);
        embedded.insert_asset(watched_path, &path, include_bytes!($path));
    }};
}

use std::path::{Path, PathBuf};

/// Returns the path used by the watcher.
#[doc(hidden)]
pub fn watched_path(source_file_path: &'static str, asset_path: &'static str) -> PathBuf {
    crate::cfg::notify! {
        if {
            PathBuf::from(source_file_path)
                .parent()
                .unwrap()
                .join(asset_path)
        } else {
            let _ = source_file_path;
            let _ = asset_path;
            PathBuf::new()
        }
    }
}

/// Maps a source file path onto its `embedded` asset path.
#[doc(hidden)]
pub fn __embedded_asset_path(
    file_path: &str,
    crate_name: &str,
    src_prefix: &Path,
    asset_path: &Path,
) -> PathBuf {
    #[cfg(target_family = "windows")]
    let file_path: PathBuf = PathBuf::from(file_path);

    // ↓ `file_path` is complition string, need to handle cross compilation.
    #[cfg(not(target_family = "windows"))]
    let file_path: PathBuf = if file_path.as_bytes().contains(&b'\\') {
        let mut buffer = String::from(file_path);
        #[expect(unsafe_code, reason = "raw bytes modification")]
        unsafe {
            let iter = buffer.as_bytes_mut().iter_mut();
            iter.filter(|c| **c == b'\\').for_each(|c| *c = b'/');
        }
        PathBuf::from(buffer)
    } else {
        PathBuf::from(file_path)
    };

    let mut maybe_parent = file_path.parent();
    let after_src = loop {
        let Some(parent) = maybe_parent else {
            panic!("Failed to find src_prefix {src_prefix:?} in {file_path:?}")
        };
        if parent.ends_with(src_prefix) {
            break file_path.strip_prefix(parent).unwrap();
        }
        maybe_parent = parent.parent();
    };
    let asset_path = after_src.parent().unwrap().join(asset_path);

    Path::new(crate_name).join(asset_path)
}
