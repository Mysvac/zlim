//! The registry resource that stores embedded assets and registers their source.

use std::path::Path;

use zlim_core::derive::Resource;
use zlim_path::derive::TypePath;

use super::EMBEDDED;
use crate::io::ErasedAssetReader;
use crate::io::memory::{Data, Dir, MemoryAssetReader, Value};
use crate::source::{AssetSourceBuilder, AssetSourceBuilders};

crate::cfg::watch! {
    use std::path::PathBuf;
    use std::sync::{Arc, PoisonError, RwLock};
    use zlim_utils::hash::HashMap;
}

// -----------------------------------------------------------------------------
// EmbeddedAssetRegistry

crate::cfg::watch! {
    if {
        #[derive(TypePath, Resource, Default)]
        pub struct EmbeddedAssetRegistry {
            dir: Dir,
            root_paths: Arc<RwLock<HashMap<Box<Path>, PathBuf>>>,
        }
    } else {
        #[derive(TypePath, Resource, Default)]
        pub struct EmbeddedAssetRegistry {
            dir: Dir,
        }
    }
}

impl EmbeddedAssetRegistry {
    fn insert_asset_internal(&self, _full_path: &Path, asset_path: &Path, value: Value) {
        crate::cfg::watch! {
            self.root_paths
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(_full_path.into(), asset_path.to_path_buf());
        }

        self.dir.insert_asset(asset_path, value);
    }

    fn insert_meta_internal(&self, _full_path: &Path, asset_path: &Path, value: Value) {
        crate::cfg::watch! {
            self.root_paths
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(_full_path.into(), asset_path.to_path_buf());
        }

        self.dir.insert_meta(asset_path, value);
    }

    /// Inserts new asset with `full_path`, `asset_path` and `value`.
    ///
    /// `full_path` is the path [`file!`] would return in the source file that registers the
    /// asset. `asset_path` is the path that will be used to identify the asset in the
    /// `embedded` [`AssetSource`]. `value` is the bytes that will be returned for the asset:
    /// _either_ a `&'static [u8]`, a `&'static [u8; N]`, a `&'static str`, a `Vec<u8>` or an
    /// `Arc<[u8]>`.
    ///
    /// [`AssetSource`]: crate::source::AssetSource
    pub fn insert_asset(&self, full_path: &Path, asset_path: &Path, value: impl Into<Value>) {
        self.insert_asset_internal(full_path, asset_path, value.into());
    }

    /// Inserts new asset metadata with `full_path`, `asset_path` and `value`.
    ///
    /// `full_path` is the path [`file!`] would return in the source file that registers the
    /// metadata. `asset_path` is the path that will be used to identify the asset in the
    /// `embedded` [`AssetSource`]. `value` is the bytes that will be returned for the
    /// metadata: _either_ a `&'static [u8]`, a `&'static [u8; N]`, a `&'static str`, a
    /// `Vec<u8>` or an `Arc<[u8]>`.
    ///
    /// [`AssetSource`]: crate::source::AssetSource
    pub fn insert_meta(&self, full_path: &Path, asset_path: &Path, value: impl Into<Value>) {
        self.insert_meta_internal(full_path, asset_path, value.into());
    }

    /// Removes an asset stored using `full_path`.
    ///
    /// `full_path` is the path [`file!`] would return in the source file that registers the
    /// asset. The entry is removed from the in-memory tree by that same path, so the stored
    /// [`Data`] is only returned when the asset was inserted with an `asset_path` equal to
    /// `full_path`; otherwise this returns `None`.
    pub fn remove_asset(&self, full_path: &Path) -> Option<Data> {
        crate::cfg::watch! {
            self.root_paths
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(full_path);
        }

        self.dir.remove_asset(full_path)
    }
}

// -----------------------------------------------------------------------------
// register_source

impl EmbeddedAssetRegistry {
    /// Registers the [`EMBEDDED`] [`AssetSource`] to the given [`AssetSourceBuilders`].
    ///
    /// This is called by `AssetPlugin`, calling it twice replaces the source.
    ///
    /// [`AssetSource`]: crate::source::AssetSource
    #[rustfmt::skip]
    pub fn register_source(&self, sources: &mut AssetSourceBuilders) {
        let dir = self.dir.clone();
        let p_dir = self.dir.clone();

        let reader_builder = move || {
            Box::new(MemoryAssetReader { root: dir.clone() }) as Box<dyn ErasedAssetReader>
        };
        let p_reader_builder = move || {
            Box::new(MemoryAssetReader { root: p_dir.clone() }) as Box<dyn ErasedAssetReader>
        };

        // Note that we only add a processed watch warning because we don't want to warn
        // noisily about embedded watching (which is niche) when users enable file watching.

        let source = crate::cfg::watch! {
            if {{
                use crate::io::watcher::EmbeddedWatcher;
                use core::time::Duration;

                const DEBOUNCE: Duration = Duration::from_millis(300);

                let dir = self.dir.clone();
                let p_dir = self.dir.clone();
                let root_paths = self.root_paths.clone();
                let p_root_paths = self.root_paths.clone();

                let watcher_builder = move |sender| {
                    EmbeddedWatcher::build(dir.clone(), root_paths.clone(), sender, DEBOUNCE)
                };

                let p_watcher_builder = move |sender| {
                    EmbeddedWatcher::build(p_dir.clone(), p_root_paths.clone(), sender, DEBOUNCE)
                };

                AssetSourceBuilder::new(reader_builder)
                    .with_processed_reader(p_reader_builder)
                    .with_watcher(watcher_builder)
                    .with_processed_watcher(p_watcher_builder)
                    .with_processed_watch_warning("Platform not support EmbeddedWatcher.")
            }} else {{
                AssetSourceBuilder::new(reader_builder)
                    .with_processed_reader(p_reader_builder)
                    .with_processed_watch_warning("Consider enabling the `watch` cargo feature.")
            }}
        };

        sources.insert(EMBEDDED, source);
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use futures_lite::future::block_on;

    use super::*;
    use crate::ident::AssetSourceId;
    use crate::io::memory::MemoryAssetWriter;

    /// Creates a builder for a plain in-memory default source.
    fn memory_builder() -> AssetSourceBuilder {
        AssetSourceBuilder::new(|| {
            Box::new(MemoryAssetReader::default()) as Box<dyn ErasedAssetReader>
        })
        .with_writer(|| {
            Some(Box::new(MemoryAssetWriter::default()) as Box<dyn crate::io::ErasedAssetWriter>)
        })
    }

    /// The registered `embedded` source serves what the registry holds, and its processed reader
    /// answers with the same bytes: an embedded asset is already in its final form, so there is
    /// nothing left to import.
    #[test]
    fn registered_source_reads_inserted_bytes() {
        let registry = EmbeddedAssetRegistry::default();
        registry.insert_asset(
            Path::new("src/shader.wgsl"),
            Path::new("my_crate/shader.wgsl"),
            &b"fn main() {}"[..],
        );
        registry.insert_meta(
            Path::new("src/shader.wgsl.meta"),
            Path::new("my_crate/shader.wgsl"),
            &b"meta"[..],
        );

        let mut builders = AssetSourceBuilders::default();
        builders.insert(AssetSourceId::Default, memory_builder());
        registry.register_source(&mut builders);

        let sources = builders.build_sources(false, false);
        let Ok(source) = sources.get(EMBEDDED) else {
            panic!("the embedded source must be registered");
        };

        assert_eq!(
            block_on(
                source
                    .reader()
                    .read_bytes(Path::new("my_crate/shader.wgsl"))
            )
            .unwrap(),
            b"fn main() {}"
        );
        assert_eq!(
            block_on(
                source
                    .reader()
                    .read_meta_bytes(Path::new("my_crate/shader.wgsl"))
            )
            .unwrap(),
            b"meta"
        );
        assert_eq!(
            block_on(
                source
                    .processed_reader()
                    .unwrap()
                    .read_bytes(Path::new("my_crate/shader.wgsl"))
            )
            .unwrap(),
            b"fn main() {}"
        );
    }

    #[test]
    fn removing_an_asset_is_a_no_op_when_missing() {
        let registry = EmbeddedAssetRegistry::default();
        assert!(registry.remove_asset(Path::new("nope.txt")).is_none());
    }
}
