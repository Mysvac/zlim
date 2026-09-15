//! Waiting for the importer before reading the processed side.
//!
//! A running app reads the *processed* assets an importer writes. Without a gate it would happily
//! read a file the importer is in the middle of rewriting, so the processed reader of a source is
//! wrapped in [`ProcessorGatedReader`]: a read of `path` waits until the importer has an answer for
//! that path — written it, found it up to date, or failed — and only then touches the file.
//!
//! Waiting for the *answer* is not enough on its own: the importer rewrites assets when their
//! sources change, so it can be writing one while the app reads the previous revision. That is what
//! the asset's transaction lock is for: the importer holds it for writing while it rewrites the
//! bytes and the `.meta` together, and every read here holds it for reading — so a reader sees one
//! whole revision, never new bytes next to an old `.meta`.

use core::pin::Pin;
use core::task::{Context, Poll};
use std::path::Path;
use std::sync::Arc;

use async_lock::RwLockReadGuardArc;

use crate::ident::AssetSourceId;
use crate::io::future::ReadAllFuture;
use crate::io::{AssetReader, AssetReaderError, ErasedAssetReader, Reader, ReaderNotSeekableError};
use crate::path::AssetPath;
use crate::processor::ProcessStatus;
use crate::processor::ProcessingState;
use crate::utils::PathStream;

// -----------------------------------------------------------------------------
// ProcessorGatedReader

/// An [`AssetReader`] that holds a read back until the importer is done with that path.
pub(crate) struct ProcessorGatedReader {
    source: AssetSourceId,
    reader: Arc<dyn ErasedAssetReader>,
    state: Arc<ProcessingState>,
}

impl ProcessorGatedReader {
    /// Wraps `reader`, which belongs to `source`, gating it on `state`.
    pub(crate) fn new(
        source: AssetSourceId,
        reader: Arc<dyn ErasedAssetReader>,
        state: Arc<ProcessingState>,
    ) -> Self {
        Self {
            source,
            reader,
            state,
        }
    }

    /// Waits until `path` has been processed, or reports it as missing.
    ///
    /// A failed or never-seen path reads as [`AssetReaderError::NotFound`]: from the reader's point
    /// of view there simply is no such file.
    async fn wait_for(&self, path: &Path) -> Result<AssetPath<'static>, AssetReaderError> {
        let asset_path: AssetPath<'static> =
            AssetPath::from(path.to_path_buf()).with_source_id(self.source.clone());

        #[cfg(any(debug_assertions, feature = "debug"))]
        zlim_log::trace!("Waiting for processing to finish before reading {asset_path}");

        let status = self.state.wait_until_processed(asset_path.clone()).await;

        #[cfg(any(debug_assertions, feature = "debug"))]
        zlim_log::trace!(
            "Processing finished with {asset_path}, reading {}",
            status.status()
        );

        match status {
            ProcessStatus::Processed => Ok(asset_path),
            ProcessStatus::Failed | ProcessStatus::NonExistent => {
                Err(AssetReaderError::NotFound(path.to_path_buf()))
            }
        }
    }
}

impl AssetReader for ProcessorGatedReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        let asset_path = self.wait_for(path).await?;
        let lock = self.state.transaction_lock(&asset_path).await?;
        let reader = self.reader.read(path).await?;

        Ok(TransactionLockedReader::new(reader, lock))
    }

    async fn read_meta<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        let asset_path = self.wait_for(path).await?;
        let lock = self.state.transaction_lock(&asset_path).await?;
        let reader = self.reader.read_meta(path).await?;

        Ok(TransactionLockedReader::new(reader, lock))
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<PathStream>, AssetReaderError> {
        // A directory is not one asset: what has to be waited for is the run as a whole.
        self.state.wait_until_finished().await;
        self.reader.read_directory(path).await
    }

    async fn is_directory<'a>(&'a self, path: &'a Path) -> Result<bool, AssetReaderError> {
        self.state.wait_until_finished().await;
        self.reader.is_directory(path).await
    }
}

// -----------------------------------------------------------------------------
// TransactionLockedReader

/// A [`Reader`] that holds its asset's transaction lock until it is dropped.
///
/// The guard is not read: it is there so the importer cannot rewrite the bytes (or the `.meta`)
/// while this reader is still handing them out.
struct TransactionLockedReader<'a> {
    reader: Box<dyn Reader + 'a>,
    _transaction_lock: RwLockReadGuardArc<()>,
}

impl<'a> TransactionLockedReader<'a> {
    fn new(reader: Box<dyn Reader + 'a>, file_lock: RwLockReadGuardArc<()>) -> Self {
        Self {
            reader,
            _transaction_lock: file_lock,
        }
    }
}

impl futures_lite::AsyncRead for TransactionLockedReader<'_> {
    #[inline]
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

impl Reader for TransactionLockedReader<'_> {
    #[inline]
    fn seekable(&mut self) -> Result<&mut dyn crate::io::SeekableReader, ReaderNotSeekableError> {
        self.reader.seekable()
    }

    #[inline]
    fn read_all_bytes<'a>(&'a mut self, buf: &'a mut Vec<u8>) -> ReadAllFuture<'a> {
        self.reader.read_all_bytes(buf)
    }
}
