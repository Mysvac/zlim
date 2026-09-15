use core::hash::BuildHasher;
use std::path::{Path, PathBuf};

use futures_lite::{AsyncReadExt, AsyncWriteExt};
use zlim_utils::{format_smol, hash::FixedState};

const CACHE_DIR: &str = ".web-asset-cache";

fn build_path(url: &str) -> PathBuf {
    let url = url.trim();

    let hash = FixedState.hash_one(url);
    let len = url.len();

    let name = format_smol!("{:016x}-{:x}", hash, len);
    Path::new(CACHE_DIR).join(name.as_str())
}

pub async fn save_cache(url: &str, data: &[u8]) -> Result<(), std::io::Error> {
    let filename = build_path(url);

    async_fs::create_dir_all(CACHE_DIR).await.ok();

    let mut cache_file = async_fs::File::create(&filename).await?;
    cache_file.write_all(data).await?;

    cache_file.close().await?;

    Ok(())
}

pub async fn load_cache(url: &str) -> Result<Option<Vec<u8>>, std::io::Error> {
    let filename = build_path(url);

    if filename.exists() {
        let mut file = async_fs::File::open(&filename).await?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await?;
        Ok(Some(buffer))
    } else {
        Ok(None)
    }
}
