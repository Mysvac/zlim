//! Android asset reader backed by the platform `AssetManager`.

use std::ffi::CString;
use std::path::Path;

use futures_lite::stream;
use zlim_os::sys::android_activity;

use crate::io::{AssetReader, AssetReaderError, Reader, VecReader};
use crate::utils::{PathStream, append_meta_extension};

// -----------------------------------------------------------------------------
// AndroidAssetReader

/// [`AssetReader`] implementation for Android devices, built on top of Android's `AssetManager`.
pub struct AndroidAssetReader;

impl AndroidAssetReader {
    /// Returns Android's `AssetManager`, panicking when the app was not set up.
    #[inline]
    fn asset_manager() -> android_activity::ndk::asset::AssetManager {
        zlim_app::sys::ANDROID_APP
            .get()
            .expect("Zlim must be setup with the `#[zlim_main]` macro on Android")
            .asset_manager()
    }
}

impl AssetReader for AndroidAssetReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        let asset_manager = Self::asset_manager();

        let mut opened_asset = asset_manager
            .open(&CString::new(path.to_str().unwrap()).unwrap())
            .ok_or(AssetReaderError::NotFound(path.to_path_buf()))?;

        let bytes = opened_asset.buffer()?;
        let reader = VecReader::new(bytes.to_vec());
        Ok(reader)
    }

    async fn read_meta<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        let meta_path = append_meta_extension(path);

        let asset_manager = Self::asset_manager();

        let mut opened_asset = asset_manager
            .open(&CString::new(meta_path.to_str().unwrap()).unwrap())
            .ok_or(AssetReaderError::NotFound(meta_path))?;

        let bytes = opened_asset.buffer()?;
        let reader = VecReader::new(bytes.to_vec());
        Ok(reader)
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<PathStream>, AssetReaderError> {
        let asset_manager = Self::asset_manager();

        let opened_assets_dir = asset_manager
            .open_dir(&CString::new(path.to_str().unwrap()).unwrap())
            .ok_or(AssetReaderError::NotFound(path.to_path_buf()))?;

        let mapped_stream = opened_assets_dir
            .filter_map(move |f| {
                let file_path = path.join(Path::new(f.to_str().unwrap()));
                // filter out meta files as they are not considered assets
                if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                    if ext.eq_ignore_ascii_case("meta") {
                        return None;
                    }
                }
                Some(file_path.to_owned())
            })
            .collect::<Vec<_>>();

        let read_dir: Box<PathStream> = Box::new(stream::iter(mapped_stream));
        Ok(read_dir)
    }

    async fn is_directory<'a>(&'a self, path: &'a Path) -> Result<bool, AssetReaderError> {
        let asset_manager = Self::asset_manager();

        // HACK: `AssetManager` does not provide a way to check if path
        // points to a directory or a file
        // `open_dir` succeeds for both files and directories and will only
        // fail if the path does not exist at all
        // `open` will fail for directories, but it will work for files
        // The solution here was to first use `open_dir` to eliminate the case
        // when the path does not exist at all, and then to use `open` to
        // see if that path is a file or a directory
        let cpath = CString::new(path.to_str().unwrap()).unwrap();
        let _ = asset_manager
            .open_dir(&cpath)
            .ok_or(AssetReaderError::NotFound(path.to_path_buf()))?;
        Ok(asset_manager.open(&cpath).is_none())
    }
}
