use crate::io::{AssetReader, AssetReaderError, PathStream, Reader};
use crate::source::AssetSourceBuilder;
use std::path::{Path, PathBuf};

/// Asset reader that treats paths as URLs to load assets from.
enum WebAssetReader {
    /// Unencrypted connections.
    Http,
    /// Use TLS for setting up connections.
    Https,
}

impl WebAssetReader {
    #[inline]
    fn make_uri(&self, path: &Path) -> PathBuf {
        let prefix = match self {
            Self::Http => "http://",
            Self::Https => "https://",
        };
        Path::new(prefix).join(path)
    }

    /// See [`io::get_meta_path`](`crate::io::get_meta_path`)
    #[inline]
    fn make_meta_uri(&self, path: &Path) -> PathBuf {
        let meta_path = crate::utils::append_meta_extension(path);
        self.make_uri(&meta_path)
    }
}

#[cfg(target_arch = "wasm32")]
mod impls {
    use crate::io::platform::HttpWasmAssetReader;
    use crate::io::{AssetReaderError, Reader};
    use std::path::PathBuf;

    pub(super) async fn get(path: PathBuf) -> Result<Box<dyn Reader>, AssetReaderError> {
        HttpWasmAssetReader::new("")
            .fetch_bytes(path)
            .await
            .map(|r| Box::new(r) as Box<dyn Reader>)
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod impls {
    use blocking::unblock;
    use std::io::{BufReader, Read};
    use std::path::PathBuf;
    use std::sync::LazyLock;

    use crate::io::{AssetReaderError, Reader, VecReader};
    use ureq::Agent;

    #[cfg(feature = "https")]
    use ureq::tls::{RootCerts, TlsConfig};

    static AGENT: LazyLock<Agent> = LazyLock::new(|| {
        let builder = Agent::config_builder();

        #[cfg(feature = "https")]
        let builder = builder.tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        );

        builder.build().new_agent()
    });

    pub(super) async fn get(path: PathBuf) -> Result<Box<dyn Reader>, AssetReaderError> {
        let str_path: &str = path.to_str().ok_or_else(|| {
            ::core::hint::cold_path();
            let msg = format!("non-utf8 path: {}", path.display());
            AssetReaderError::Io(std::io::Error::other(msg))
        })?;

        #[cfg(not(target_os = "windows"))]
        let url = String::from(str_path);

        #[cfg(target_os = "windows")]
        let mut url = String::from(str_path);

        #[cfg(target_os = "windows")]
        crate::path::normalize_separators(&mut url);

        #[cfg(feature = "web_asset_cache")]
        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
        if let Some(data) = super::super::load_cache(str_path).await? {
            return Ok(Box::new(VecReader::new(data)));
        }

        #[cfg(feature = "web_asset_cache")]
        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
        let cache_uri = url.clone();

        let response = unblock(|| AGENT.get(url).call()).await;

        match response {
            Ok(mut response) => {
                let reader = response.body_mut().with_config().reader();

                let mut buffer = Vec::new();
                BufReader::new(reader).read_to_end(&mut buffer)?;

                #[cfg(feature = "web_asset_cache")]
                #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
                super::super::save_cache(&cache_uri, &buffer).await?;

                Ok(Box::new(VecReader::new(buffer)))
            }
            // ureq considers all >=400 status codes as errors
            Err(ureq::Error::StatusCode(code)) => {
                if code == 404 {
                    Err(AssetReaderError::NotFound(path))
                } else {
                    Err(AssetReaderError::HttpError(code))
                }
            }
            Err(err) => {
                let msg = format!(
                    "unexpected error while loading asset {}: {err}",
                    path.display(),
                );
                Err(AssetReaderError::Io(std::io::Error::other(msg)))
            }
        }
    }
}

impl AssetReader for WebAssetReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        impls::get(self.make_uri(path)).await
    }

    async fn read_meta<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        impls::get(self.make_meta_uri(path)).await
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<PathStream>, AssetReaderError> {
        Err(AssetReaderError::NotFound(self.make_uri(path)))
    }

    async fn is_directory<'a>(&'a self, _path: &'a Path) -> Result<bool, AssetReaderError> {
        Ok(false)
    }
}

#[cfg(feature = "http")]
pub(super) fn http_source_builder() -> AssetSourceBuilder {
    AssetSourceBuilder::new(move || Box::new(WebAssetReader::Http))
        .with_processed_reader(move || Box::new(WebAssetReader::Http))
}

#[cfg(feature = "https")]
pub(super) fn https_source_builder() -> AssetSourceBuilder {
    AssetSourceBuilder::new(move || Box::new(WebAssetReader::Https))
        .with_processed_reader(move || Box::new(WebAssetReader::Https))
}
