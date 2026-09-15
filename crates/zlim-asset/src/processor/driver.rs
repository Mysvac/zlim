//! The primitives the importer needs from the [`AssetServer`]: run one processor over one path, and
//! hash a source asset the way the importer does.
//!
//! These are deliberately *primitives*: they are given the processor to run, so nothing here knows
//! about the processor registry — that belongs to [`AssetProcessServer`], the only thing that
//! drives processing. What lives here is what needs the server's sources:
//!
//! - [`AssetServer::process_asset_with`]: run one processor over one source path, writing the
//!   processed bytes and the `.meta` that says how to load them, and report the
//!   [`ProcessedInfo`] it recorded;
//! - [`AssetServer::source_hash_of`]: hash a source asset the way the importer does, which is what
//!   a recorded hash is compared against.
//!
//! What the processed side *says* about such an output — and how one goes away — is `processed.rs`.
//!
//! [`AssetProcessServer`]: crate::processor::AssetProcessServer

use futures_lite::AsyncWriteExt;

use crate::error::{AssetError, AssetProcessError};
use crate::io::AssetReaderError;
use crate::io::SliceReader;
use crate::io::VecReader;
use crate::meta::{AssetHash, ProcessedInfo, Settings};
use crate::path::AssetPath;
use crate::processor::{ErasedAssetProcessor, ProcessContext};
use crate::server::AssetServer;

// -----------------------------------------------------------------------------
// AssetServer: processing one path

impl AssetServer {
    /// Runs `processor` over the source asset at `path`, writing the processed bytes and their
    /// `.meta` to the processed side of the source that owns `path`.
    ///
    /// `settings` and `meta_bytes` (the source's own `.meta`, empty when it has none) come from the
    /// caller, which is the one that resolved the processor — see [`AssetProcessServer`].
    ///
    /// The `.meta` written next to the processed bytes is the one `processor` returns (a `Load`
    /// config naming the loader that reads them back), plus the [`ProcessedInfo`] of this run —
    /// which is what a later run compares against to decide whether processing is needed at all.
    /// That same [`ProcessedInfo`] is returned, so the caller can record it without reading the
    /// file back.
    ///
    /// [`AssetProcessServer`]: crate::processor::AssetProcessServer
    pub(crate) async fn process_asset_with(
        &self,
        processor: &dyn ErasedAssetProcessor,
        settings: &dyn Settings,
        path: &AssetPath<'static>,
        meta_bytes: &[u8],
    ) -> Result<ProcessedInfo, AssetProcessError> {
        let source = self.0.sources.get(path.source_id())?;

        let reader = source.reader();
        let writer = source.processed_writer()?;

        // One revision of the bytes: they are hashed first and then handed to the loader,
        // so the hash always describes what was processed.
        let bytes = reader.read_bytes(path.path()).await?;

        let hash = hash_of(meta_bytes, &bytes).await?;

        let mut processed_info = ProcessedInfo {
            hash,
            full_hash: hash,
            process_dependencies: Vec::new(),
        };

        let mut out = writer.write(path.path()).await?;

        let ctx_rd = Box::new(VecReader::new(bytes));
        let context = ProcessContext::new(self, path, ctx_rd, &mut processed_info);

        let mut meta = processor.process(&mut out, context, settings).await?;

        out.flush().await.map_err(|error| {
            ::core::hint::cold_path();
            let error = crate::io::AssetWriterError::from(error);
            AssetProcessError::from(AssetError::from(error))
        })?;

        // The dependencies read during processing are part of "did anything change": folding their
        // hashes into `full_hash` is what makes a change to a side file reprocess this asset.
        let dep_hashes = processed_info
            .process_dependencies
            .iter()
            .map(|dep| &dep.full_hash);

        processed_info.full_hash = AssetHash::fold_hash(&processed_info.hash, dep_hashes);

        *meta.processed_info_mut() = Some(processed_info);

        writer
            .write_meta_bytes(path.path(), &meta.serialize())
            .await?;

        // The info was written into the meta just above, so taking it back out cannot be `None`.
        Ok(meta.processed_info_mut().take().unwrap())
    }

    /// Hashes the source asset at `path` the way the importer does: its `.meta` and its bytes
    /// together, so a change to either one is a change to the asset.
    ///
    /// This is the value a [`ProcessedInfo::hash`] is compared against by [`is_up_to_date`].
    ///
    /// [`is_up_to_date`]: crate::processor::infos::ProcessorAssetInfos::is_up_to_date
    pub(crate) async fn source_hash_of(
        &self,
        path: &AssetPath<'static>,
    ) -> Result<AssetHash, AssetProcessError> {
        let source = self.0.sources.get(path.source_id())?;

        let reader = source.reader();
        let meta_bytes = read_meta_or_empty(reader, path).await?;
        let bytes = reader.read_bytes(path.path()).await?;

        hash_of(&meta_bytes, &bytes).await
    }
}

// -----------------------------------------------------------------------------
// Hashing a source asset

/// Hashes a source `.meta` together with the asset bytes it belongs to.
async fn hash_of(meta_bytes: &[u8], bytes: &[u8]) -> Result<AssetHash, AssetProcessError> {
    AssetHash::async_hash(meta_bytes, &mut SliceReader::new(bytes))
        .await
        .map_err(|error| {
            ::core::hint::cold_path();
            AssetProcessError::from(AssetReaderError::from(error))
        })
}

/// Reads `path`'s `.meta` from the source side, treating a missing one as empty.
pub(crate) async fn read_meta_or_empty(
    reader: &dyn crate::io::ErasedAssetReader,
    path: &AssetPath<'static>,
) -> Result<Vec<u8>, AssetProcessError> {
    match reader.read_meta_bytes(path.path()).await {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.is_not_found() => Ok(Vec::new()),
        Err(error) => {
            ::core::hint::cold_path();
            let error = crate::error::AssetMetaReadError {
                path: path.clone(),
                error,
            };
            Err(AssetProcessError::from(AssetError::from(error)))
        }
    }
}
