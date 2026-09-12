//! The reading half of the IO layer.
//!
//! Two levels live here:
//!
//! - [`Reader`] / [`SeekableReader`] are *byte streams*: `futures_lite`'s `AsyncRead` (plus
//!   `AsyncSeek`) with one addition — [`Reader::read_all_bytes`], the "append everything to a
//!   `Vec<u8>`" fast path. [`VecReader`] and [`SliceReader`] are ready-made implementations
//!   over memory that complete without an async state machine.
//!
//! - [`AssetReader`] is a *source*: it maps asset paths onto byte streams, meta sidecars and
//!   directory listings. It is written with RPITIT (`-> impl Future<…> + Send`), which is
//!   cheap for implementors but **not object safe**, so [`ErasedAssetReader`] is the boxed
//!   mirror the asset server stores sources as; it is implemented automatically for every
//!   `AssetReader`.
//!
//! # Examples
//!
//! Reading through the byte level:
//!
//! ```rust
//! use futures_lite::future::block_on;
//! use zlim_asset::io::{Reader, VecReader};
//!
//! let mut reader = VecReader::new(b"level.ron".to_vec());
//!
//! let mut bytes = Vec::new();
//! block_on(reader.read_all_bytes(&mut bytes)).unwrap();
//!
//! assert_eq!(bytes, b"level.ron");
//! ```
//!
//! Reading through the source level:
//!
//! ```rust
//! # use futures_lite::future::block_on;
//! # use std::path::Path;
//! # use zlim_asset::io::AssetReader;
//! use zlim_asset::io::memory::MemoryAssetReader;
//!
//! let source = MemoryAssetReader::default();
//! source.root.insert_asset_text(Path::new("models/level.ron"), "level");
//!
//! let bytes = block_on(source.read_bytes(Path::new("models/level.ron"))).unwrap();
//! assert_eq!(bytes, b"level");
//! ```

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::io::SeekFrom;
use std::path::{Path, PathBuf};

use zlim_core::derive::Error;

use super::future::ReadAllFuture;
use crate::utils::{BoxedFuture, PathStream};

// -----------------------------------------------------------------------------
// AssetReaderError

/// Errors that occur while loading assets.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum AssetReaderError {
    /// The requested path does not exist.
    #[error("Path not found: {}", _0.display())]
    NotFound(PathBuf),
    /// The underlying IO operation failed.
    #[error("Encountered an I/O error while loading asset: {_0}")]
    Io(std::io::Error),
    /// The remote source answered with an unexpected HTTP status.
    #[error("Encountered HTTP status {_0:?} when loading asset")]
    HttpError(u16),
}

impl Clone for AssetReaderError {
    fn clone(&self) -> Self {
        match self {
            Self::NotFound(arg) => Self::NotFound(arg.clone()),
            Self::Io(arg) => {
                // For IO errors, we only compare types,
                // so `Clone` guarantees equality invariance.
                let kind = arg.kind();
                Self::Io(std::io::Error::from(kind))
            }
            Self::HttpError(arg) => Self::HttpError(*arg),
        }
    }
}

impl PartialEq for AssetReaderError {
    /// Equality for `Io` is not full (only through the `ErrorKind` of the inner error).
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::NotFound(p1), Self::NotFound(p2)) => p1 == p2,
            (Self::Io(e1), Self::Io(e2)) => e1.kind() == e2.kind(),
            (Self::HttpError(c1), Self::HttpError(c2)) => c1 == c2,
            _ => false,
        }
    }
}

impl From<std::io::Error> for AssetReaderError {
    /// Wraps an IO error as [`AssetReaderError::Io`].
    #[inline]
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

// -----------------------------------------------------------------------------
// Reader

pub use futures_lite::AsyncReadExt;
pub use futures_lite::io::AsyncRead;

/// A readable byte stream used by the asset pipeline.
///
/// On top of [`AsyncRead`], an implementation only has to add two things:
///
/// - [`read_all_bytes`] — the whole remaining payload in one call, used by
///   [`AssetReader::read_bytes`] and by every loader that wants the complete file;
///
/// - [`seekable`] — an opt-in downcast to [`SeekableReader`], so loaders that
///   need random access (images, archives) can check before relying on it.
///
/// Implementations are expected to be `Unpin` (so they can be boxed into `dyn Reader`
/// and polled without pinning) and cheap to create.
///
/// [`read_all_bytes`]: Self::read_all_bytes
/// [`seekable`]: Self::seekable
///
/// # Examples
///
/// ```rust
/// use futures_lite::future::block_on;
/// use zlim_asset::io::{Reader, VecReader};
///
/// let mut reader = VecReader::new(b"meta".to_vec());
///
/// // `read_all_bytes` appends, and reports the *new* byte count.
/// let mut bytes = b"prefix:".to_vec();
/// assert_eq!(block_on(reader.read_all_bytes(&mut bytes)).unwrap(), 4);
/// assert_eq!(bytes, b"prefix:meta");
/// ```
pub trait Reader: AsyncRead + Unpin + Send + Sync {
    /// Returns a seekable view of this reader, if it supports seeking.
    ///
    /// # Errors
    ///
    /// Returns [`ReaderNotSeekableError`] when the storage cannot seek,
    /// e.g. an HTTP response body or a decompression stream.
    fn seekable(&mut self) -> Result<&mut dyn SeekableReader, ReaderNotSeekableError>;

    /// Reads this reader to EOF, appending the bytes to `buf`.
    ///
    /// Resolves to the number of bytes appended; the previous length of `buf` is not counted.
    /// Prefer this over `AsyncReadExt::read_to_end` on a `dyn Reader`: it returns a concrete
    /// future, so the trait object is not boxed a second time.
    fn read_all_bytes<'a>(&'a mut self, buf: &'a mut Vec<u8>) -> ReadAllFuture<'a>;

    /// Converts this reader into a boxed trait object.
    ///
    /// This is the usual way to return a reader from [`AssetReader`], whose methods are
    /// generic over `impl Reader`.
    #[inline]
    fn into_boxed<'a>(self) -> Box<dyn Reader + 'a>
    where
        Self: Sized + 'a,
    {
        Box::new(self)
    }
}

/// Forwards every [`Reader`] method to the boxed value.
impl Reader for Box<dyn Reader + '_> {
    #[inline(always)]
    fn read_all_bytes<'a>(&'a mut self, buf: &'a mut Vec<u8>) -> ReadAllFuture<'a> {
        (**self).read_all_bytes(buf)
    }

    #[inline(always)]
    fn seekable(&mut self) -> Result<&mut dyn SeekableReader, ReaderNotSeekableError> {
        (**self).seekable()
    }

    #[inline(always)]
    fn into_boxed<'a>(self) -> Box<dyn Reader + 'a>
    where
        Self: Sized + 'a,
    {
        self
    }
}

// -----------------------------------------------------------------------------
// SeekableReader

/// Re-export of the [`AsyncSeek`] trait.
pub use futures_lite::io::AsyncSeek;

/// A [`Reader`] that also supports seeking.
///
/// Obtained through [`Reader::seekable`]. The blanket implementation means
/// every reader that also implements [`AsyncSeek`] is seekable, so an
/// implementation only has to return `Ok(self)` (or forward to the wrapped
/// reader) from [`Reader::seekable`].
pub trait SeekableReader: Reader + AsyncSeek {}

impl<T: Reader + AsyncSeek> SeekableReader for T {}

/// Returned when a reader does not support seeking.
///
/// This describes the storage, it is not a failure: loaders that can work sequentially are
/// expected to fall back rather than propagate it.
#[derive(Error, Debug, Copy, Clone)]
#[error("The `Reader` returned by `AssetReader` does not support `AsyncSeek` behavior.")]
pub struct ReaderNotSeekableError;

// -----------------------------------------------------------------------------
// AssetReader

/// Reads raw asset bytes and directory listings from a source (filesystem, embedded, …).
///
/// This is the source-level trait: one implementation per storage backend
/// (`FileAssetReader`, `MemoryAssetReader`, `HttpWasmAssetReader`, …), registered
/// under an [`AssetSourceId`] in [`AssetSources`].
///
/// Contract:
///
/// - `path` is always **relative to the source root** and already sanitized by the asset
///   server; implementations only join it onto their own root.
/// - `read_meta` resolves the sidecar of the same path. Where it lives is up to the source:
///   the filesystem source appends `.meta`, the in-memory source keeps a separate metadata
///   tree.
/// - A missing path must be reported as [`AssetReaderError::NotFound`].
/// - Reads must be independent and `Send`: the asset server loads many assets concurrently.
///
/// The trait is not object safe; the type-erased [`ErasedAssetReader`] is created
/// automatically for any `AssetReader`.
///
/// [`AssetSourceId`]: crate::ident::AssetSourceId
/// [`AssetSources`]: crate::source::AssetSources
///
/// # Examples
///
/// ```rust
/// # use std::path::Path;
/// # use futures_lite::future::block_on;
/// # use zlim_asset::io::AssetReader;
/// use zlim_asset::io::memory::MemoryAssetReader;
///
/// let source = MemoryAssetReader::default();
/// source.root.insert_asset_text(Path::new("level.ron"), "level");
///
/// let bytes = block_on(source.read_bytes(Path::new("level.ron"))).unwrap();
/// assert_eq!(bytes, b"level");
///
/// assert!(block_on(source.read_bytes(Path::new("missing.ron"))).is_err());
/// ```
pub trait AssetReader: Sized + Sync + Send + 'static {
    /// Returns a future for the full file data at the provided path.
    fn read<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = Result<impl Reader + 'a, AssetReaderError>> + Send;

    /// Returns a future for the full meta data at the provided path.
    ///
    /// Sources normally store metadata next to the asset; the in-memory source keeps it in a
    /// separate map, the filesystem source appends `.meta`.
    fn read_meta<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = Result<impl Reader + 'a, AssetReaderError>> + Send;

    /// Returns a future for the stream of directory entry paths at the provided path.
    ///
    /// Entries are relative to the source root and non-recursive: one directory level,
    /// with sub-directories reported as entries. `.meta` sidecars and hidden files are
    /// filtered out by the storage implementations, and the ordering is unspecified.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use std::path::Path;
    /// # use futures_lite::future::block_on;
    /// # use futures_lite::StreamExt;
    /// # use zlim_asset::io::AssetReader;
    /// use zlim_asset::io::memory::MemoryAssetReader;
    ///
    /// let source = MemoryAssetReader::default();
    /// source.root.insert_asset_text(Path::new("models/a.ron"), "a");
    /// source.root.insert_asset_text(Path::new("models/b.ron"), "b");
    ///
    /// let mut entries = block_on(source.read_directory(Path::new("models"))).unwrap();
    /// let mut paths = Vec::new();
    ///
    /// while let Some(path) = block_on(entries.next()) {
    ///     paths.push(path);
    /// }
    ///
    /// assert_eq!(paths.len(), 2);
    /// ```
    fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = Result<Box<PathStream>, AssetReaderError>> + Send;

    /// Returns a future for whether the provided path points to a directory.
    ///
    /// The built-in sources report a path that does not exist as `false`, and an unreadable
    /// path as an error.
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = Result<bool, AssetReaderError>> + Send;

    /// Returns a future for the full file data at the provided path, as a `Vec<u8>`.
    ///
    /// Convenience provided by the trait: it calls [`read`](Self::read) and then drains the
    /// reader with [`Reader::read_all_bytes`], so a source only implements the streaming form.
    fn read_bytes<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = Result<Vec<u8>, AssetReaderError>> + Send {
        async {
            let mut data_reader = self.read(path).await?;
            let mut data_bytes = Vec::new();
            data_reader.read_all_bytes(&mut data_bytes).await?; // AsyncReadExt
            Ok(data_bytes)
        }
    }

    /// Returns a future for the full meta data at the provided path, as a `Vec<u8>`.
    ///
    /// The metadata counterpart of [`read_bytes`](Self::read_bytes).
    fn read_meta_bytes<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = Result<Vec<u8>, AssetReaderError>> + Send {
        async {
            let mut meta_reader = self.read_meta(path).await?;
            let mut meta_bytes = Vec::new();
            meta_reader.read_all_bytes(&mut meta_bytes).await?; // AsyncReadExt
            Ok(meta_bytes)
        }
    }
}

// -----------------------------------------------------------------------------
// ErasedAssetReader

type BoxedAssetReaderFuture<'a, T> = BoxedFuture<'a, Result<T, AssetReaderError>>;

/// A type-erased [`AssetReader`] with boxed futures, used internally by the asset server.
///
/// Every `AssetReader` implements this automatically, and [`AssetSource`] stores sources
/// as `Box<dyn ErasedAssetReader>` / `Arc<dyn ErasedAssetReader>`. The cost is one
/// allocation per future; the benefit is that sources with different concrete reader types
/// can live in one table and be shared behind an `Arc`.
///
/// Prefer implementing [`AssetReader`] and let the blanket implementation do the boxing.
///
/// # Examples
///
/// ```rust
/// # use std::path::Path;
/// # use futures_lite::future::block_on;
/// use zlim_asset::io::ErasedAssetReader;
/// use zlim_asset::io::memory::MemoryAssetReader;
///
/// let memory = MemoryAssetReader::default();
/// memory.root.insert_asset_text(Path::new("a.txt"), "asset");
/// let source: Box<dyn ErasedAssetReader> = Box::new(memory);
///
/// let bytes = block_on(source.read_bytes(Path::new("a.txt"))).unwrap();
/// assert_eq!(bytes, b"asset");
/// ```
///
/// [`AssetSource`]: crate::source::AssetSource
pub trait ErasedAssetReader: Send + Sync + 'static {
    /// Returns a future for the full file data at the provided path.
    fn read<'a>(&'a self, path: &'a Path) -> BoxedAssetReaderFuture<'a, Box<dyn Reader + 'a>>;

    /// Returns a future for the full meta data at the provided path.
    fn read_meta<'a>(&'a self, path: &'a Path) -> BoxedAssetReaderFuture<'a, Box<dyn Reader + 'a>>;

    /// Returns a future for the stream of directory entry paths at the provided path.
    fn read_directory<'a>(&'a self, path: &'a Path) -> BoxedAssetReaderFuture<'a, Box<PathStream>>;

    /// Returns a future for whether the provided path points to a directory.
    fn is_directory<'a>(&'a self, path: &'a Path) -> BoxedAssetReaderFuture<'a, bool>;

    /// Returns a future for the full file data at the provided path, as a `Vec<u8>`.
    fn read_bytes<'a>(&'a self, path: &'a Path) -> BoxedAssetReaderFuture<'a, Vec<u8>>;

    /// Returns a future for the full meta data at the provided path, as a `Vec<u8>`.
    fn read_meta_bytes<'a>(&'a self, path: &'a Path) -> BoxedAssetReaderFuture<'a, Vec<u8>>;
}

impl<T: AssetReader> ErasedAssetReader for T {
    fn read<'a>(&'a self, path: &'a Path) -> BoxedAssetReaderFuture<'a, Box<dyn Reader + 'a>> {
        Box::pin(async move { Ok(<T as AssetReader>::read(self, path).await?.into_boxed()) })
    }

    fn read_meta<'a>(&'a self, path: &'a Path) -> BoxedAssetReaderFuture<'a, Box<dyn Reader + 'a>> {
        Box::pin(async move {
            Ok(<T as AssetReader>::read_meta(self, path)
                .await?
                .into_boxed())
        })
    }

    fn read_directory<'a>(&'a self, path: &'a Path) -> BoxedAssetReaderFuture<'a, Box<PathStream>> {
        Box::pin(<T as AssetReader>::read_directory(self, path))
    }

    fn is_directory<'a>(&'a self, path: &'a Path) -> BoxedAssetReaderFuture<'a, bool> {
        Box::pin(<T as AssetReader>::is_directory(self, path))
    }

    fn read_bytes<'a>(&'a self, path: &'a Path) -> BoxedAssetReaderFuture<'a, Vec<u8>> {
        Box::pin(<T as AssetReader>::read_bytes(self, path))
    }

    fn read_meta_bytes<'a>(&'a self, path: &'a Path) -> BoxedAssetReaderFuture<'a, Vec<u8>> {
        Box::pin(<T as AssetReader>::read_meta_bytes(self, path))
    }
}

// -----------------------------------------------------------------------------
// VecReader

/// An [`AsyncRead`] implementation capable of reading a [`Vec<u8>`].
///
/// The reader owns the bytes, tracks how far it has been read, and is seekable, so
/// it is the natural return type for sources that materialize an asset (in-memory
/// source, wasm `fetch`, Android `AAssets`). Both [`Reader::read_all_bytes`] and
/// [`AsyncRead::poll_read`] run straight on the byte slice — no async state machine,
/// no copy beyond the destination.
///
/// # Examples
///
/// ```rust
/// # use std::io::SeekFrom;
/// # use futures_lite::future::block_on;
/// # use futures_lite::io::AsyncSeekExt;
/// use zlim_asset::io::{Reader, VecReader};
///
/// let mut reader = VecReader::new(b"0123456789".to_vec());
///
/// // Skip the first four bytes, then take the rest.
/// block_on(reader.seekable().unwrap().seek(SeekFrom::Start(4))).unwrap();
///
/// let mut bytes = Vec::new();
/// assert_eq!(block_on(reader.read_all_bytes(&mut bytes)).unwrap(), 6);
/// assert_eq!(bytes, b"456789");
/// ```
pub struct VecReader {
    bytes: Vec<u8>,
    bytes_read: usize,
}

impl VecReader {
    /// Creates a new [`VecReader`] for `bytes`.
    #[inline(always)]
    pub const fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            bytes_read: 0,
        }
    }
}

impl AsyncRead for VecReader {
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        use crate::utils::slice_read;
        let this = self.get_mut();
        Poll::Ready(Ok(slice_read(&this.bytes, &mut this.bytes_read, buf)))
    }
}

impl AsyncSeek for VecReader {
    #[inline]
    fn poll_seek(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        pos: SeekFrom,
    ) -> Poll<std::io::Result<u64>> {
        use crate::utils::slice_seek;
        // Get the mut borrow to avoid trying to borrow the pin itself multiple times.
        let this = self.get_mut();
        Poll::Ready(slice_seek(&this.bytes, &mut this.bytes_read, pos))
    }
}

impl Reader for VecReader {
    #[inline(always)]
    fn seekable(&mut self) -> Result<&mut dyn SeekableReader, ReaderNotSeekableError> {
        Ok(self)
    }

    #[inline]
    fn read_all_bytes<'a>(&'a mut self, buf: &'a mut Vec<u8>) -> ReadAllFuture<'a> {
        let start = self.bytes_read;
        let bytes = self.bytes.as_slice();
        if start >= bytes.len() {
            ReadAllFuture::slice_read(&[], buf)
        } else {
            ReadAllFuture::slice_read(&bytes[start..], buf)
        }
    }
}

// -----------------------------------------------------------------------------
// SliceReader

/// An [`AsyncRead`] implementation capable of reading a `&[u8]`.
///
/// Like [`VecReader`], but borrowing instead of owning: this is what
/// [`ReadAllFuture::slice_read`] is built for, and what `MemoryAssetReader`
/// and `EmbeddedAssetRegistry` hand out. The reader is [`AsyncSeek`] as well,
/// so loaders can rewind and parse a payload twice.
///
/// # Examples
///
/// ```rust
/// use futures_lite::future::block_on;
/// use zlim_asset::io::{Reader, SliceReader};
///
/// // The bytes stay where they are: the reader only borrows them.
/// let payload: &[u8] = b"embedded bytes";
/// let mut reader = SliceReader::new(payload);
///
/// let mut bytes = Vec::new();
/// block_on(reader.read_all_bytes(&mut bytes)).unwrap();
///
/// assert_eq!(bytes, payload);
/// ```
///
/// [`ReadAllFuture::slice_read`]: crate::io::future::ReadAllFuture::slice_read
pub struct SliceReader<'a> {
    bytes: &'a [u8],
    bytes_read: usize,
}

impl<'a> SliceReader<'a> {
    /// Creates a new [`SliceReader`] for `bytes`.
    #[inline(always)]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            bytes_read: 0,
        }
    }
}

impl AsyncRead for SliceReader<'_> {
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        use crate::utils::slice_read;
        let this = self.get_mut();
        Poll::Ready(Ok(slice_read(this.bytes, &mut this.bytes_read, buf)))
    }
}

impl AsyncSeek for SliceReader<'_> {
    #[inline]
    fn poll_seek(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        pos: SeekFrom,
    ) -> Poll<std::io::Result<u64>> {
        use crate::utils::slice_seek;
        let this = self.get_mut();
        Poll::Ready(slice_seek(this.bytes, &mut this.bytes_read, pos))
    }
}

impl Reader for SliceReader<'_> {
    #[inline(always)]
    fn seekable(&mut self) -> Result<&mut dyn SeekableReader, ReaderNotSeekableError> {
        Ok(self)
    }

    #[inline]
    fn read_all_bytes<'a>(&'a mut self, buf: &'a mut Vec<u8>) -> ReadAllFuture<'a> {
        let start = self.bytes_read;
        let bytes = self.bytes;
        if start >= bytes.len() {
            ReadAllFuture::slice_read(&[], buf)
        } else {
            ReadAllFuture::slice_read(&bytes[start..], buf)
        }
    }
}

// -----------------------------------------------------------------------------
