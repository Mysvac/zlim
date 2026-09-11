//! The writing half of the IO layer.
//!
//! Mirrors [`crate::io::reader`]:
//!
//! - [`Writer`] is a *byte stream*: `futures_lite`'s `AsyncWrite` plus
//!   [`Writer::write_all_bytes`], the "hand over the whole payload at once" fast path.
//!   `Vec<u8>` implements it, which is what savers and tests write into.
//! - [`AssetWriter`] is a *source*: it creates those streams for asset paths and also owns the
//!   directory operations (create/remove/rename/clear). It is written with RPITIT
//!   (`-> impl Future<…> + Send`) and therefore **not object safe**; [`ErasedAssetWriter`] is
//!   the boxed mirror stored in [`AssetSource`](crate::io::AssetSource), implemented
//!   automatically for every `AssetWriter`.
//!
//! Two rules make sources interchangeable:
//!
//! - **No storage-specific suffix at this level.** [`AssetWriter::write`] takes the *asset*
//!   path; where the meta sidecar goes (`foo.meta`, a separate tree, …) is the source's
//!   business, and callers use [`AssetWriter::write_meta`] for it.
//! - **Flushing is the caller's job.** The returned [`Writer`] is only guaranteed complete
//!   after `AsyncWriteExt::flush`; [`AssetWriter::write_bytes`] does that for you.
//!
//! # Examples
//!
//! Writing into memory, then reading it back:
//!
//! ```rust
//! use futures_lite::future::block_on;
//! use std::path::Path;
//! use zlim_asset::io::{AssetWriter, memory::MemoryAssetWriter};
//!
//! let writer = MemoryAssetWriter::default();
//! block_on(writer.write_bytes(Path::new("models/level.ron"), b"level")).unwrap();
//!
//! let stored = writer.root.get_asset(Path::new("models/level.ron")).unwrap();
//! assert_eq!(stored.value(), b"level");
//! ```

use core::future::Future;
use std::path::{Path, PathBuf};

use zlim_core::derive::Error;

use super::future::WriteAllFuture;

// -----------------------------------------------------------------------------
// AssetWriterError

/// Errors that occur while writing assets.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum AssetWriterError {
    /// The requested path does not exist.
    #[error("Path not found: {}", _0.display())]
    NotFound(PathBuf),
    /// The filename is invalid or missing.
    #[error("Filename is invalid or missing: {}", _0.display())]
    InvalidFilename(PathBuf),
    /// The directory was expected to be empty.
    #[error("Expected an empty directory, but it's not empty: {}", _0.display())]
    DirectoryNotEmpty(PathBuf),
    /// The underlying IO operation failed.
    #[error("Encountered an I/O error while writing asset: {_0}")]
    Io(std::io::Error),
}

impl Clone for AssetWriterError {
    fn clone(&self) -> Self {
        match self {
            Self::NotFound(arg) => Self::NotFound(arg.clone()),
            Self::InvalidFilename(arg) => Self::InvalidFilename(arg.clone()),
            Self::DirectoryNotEmpty(arg) => Self::DirectoryNotEmpty(arg.clone()),
            Self::Io(arg) => {
                // For IO errors, we only compare types,
                // so `Clone` guarantees equality invariance.
                let kind = arg.kind();
                Self::Io(std::io::Error::from(kind))
            }
        }
    }
}

impl PartialEq for AssetWriterError {
    /// Equality for `Io` is not full (only through the `ErrorKind` of the inner error).
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::NotFound(p1), Self::NotFound(p2)) => p1 == p2,
            (Self::InvalidFilename(p1), Self::InvalidFilename(p2)) => p1 == p2,
            (Self::DirectoryNotEmpty(p1), Self::DirectoryNotEmpty(p2)) => p1 == p2,
            (Self::Io(e1), Self::Io(e2)) => e1.kind() == e2.kind(),
            _ => false,
        }
    }
}

impl From<std::io::Error> for AssetWriterError {
    /// Wraps an IO error as [`AssetWriterError::Io`].
    #[inline]
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

// -----------------------------------------------------------------------------
// Writer

pub use futures_lite::AsyncWriteExt;
pub use futures_lite::io::AsyncWrite;

/// A writeable byte stream used by the asset pipeline.
///
/// On top of [`AsyncWrite`], the only addition is [`write_all_bytes`]: a concrete future
/// that keeps writing until the whole slice is accepted, used by [`AssetWriter::write_bytes`]
/// and by savers. It deliberately does **not** flush — callers that need durability call
/// `AsyncWriteExt::flush`, which is why the example below imports both traits.
///
/// [`write_all_bytes`]: Self::write_all_bytes
///
/// # Examples
///
/// ```rust
/// use futures_lite::future::block_on;
/// use futures_lite::io::AsyncWriteExt;
/// use std::path::Path;
/// // The concrete `Writer` implementations come from sources, so this example asks a
/// // `MemoryAssetWriter` for one.
/// use zlim_asset::io::AssetWriter;
/// use zlim_asset::io::Writer;
/// use zlim_asset::io::memory::MemoryAssetWriter;
///
/// let source = MemoryAssetWriter::default();
/// let mut writer = block_on(source.write(Path::new("a.txt"))).unwrap();
///
/// block_on(writer.write_all_bytes(b"level")).unwrap();
/// block_on(writer.write_all_bytes(b".ron")).unwrap();
/// block_on(writer.flush()).unwrap();
///
/// assert_eq!(
///     source.root.get_asset(Path::new("a.txt")).unwrap().value(),
///     b"level.ron"
/// );
/// ```
pub trait Writer: AsyncWrite + Unpin + Send + Sync {
    /// Writes the whole slice, looping until every byte is accepted.
    fn write_all_bytes<'a>(&'a mut self, buf: &'a [u8]) -> WriteAllFuture<'a>;

    /// Converts this writer into a boxed trait object.
    #[inline]
    fn into_boxed<'a>(self) -> Box<dyn Writer + 'a>
    where
        Self: Sized + 'a,
    {
        Box::new(self)
    }
}

/// Forwards every [`Writer`] method to the boxed value.
impl Writer for Box<dyn Writer + '_> {
    #[inline(always)]
    fn write_all_bytes<'a>(&'a mut self, buf: &'a [u8]) -> WriteAllFuture<'a> {
        (**self).write_all_bytes(buf)
    }

    #[inline(always)]
    fn into_boxed<'a>(self) -> Box<dyn Writer + 'a>
    where
        Self: Sized + 'a,
    {
        self
    }
}

// -----------------------------------------------------------------------------
// AssetWriter

/// Writes raw asset bytes and directory operations back to a source.
///
/// The write-side counterpart of [`AssetReader`]: one implementation per storage backend
/// that supports writing (the filesystem source, the in-memory source used by tests and
/// embedded assets, …). Sources that cannot be written to simply do not provide a writer,
/// which is why [`default_writer`] returns `None` on wasm and Android.
///
/// Contract:
///
/// - `path` is relative to the source root, exactly as for reads.
/// - `write` / `write_meta` return a stream; the bytes are only guaranteed to have landed
///   after it is flushed. [`write_bytes`] and [`write_meta_bytes`] do that for you.
/// - Sources add their own layout: the filesystem source appends `.meta` for metadata, the
///   in-memory source keeps a separate metadata map.
/// - Removals other than [`remove_empty_directory`] are recursive, and removing something
///   that does not exist is a [`NotFound`] error (except for metadata, see [`remove_meta`]).
///
/// The trait is not object safe; the type-erased [`ErasedAssetWriter`] is created
/// automatically for any `AssetWriter`.
///
/// [`write_bytes`]: Self::write_bytes
/// [`write_meta_bytes`]: Self::write_meta_bytes
/// [`remove_empty_directory`]: Self::remove_empty_directory
/// [`remove_meta`]: Self::remove_meta
/// [`NotFound`]: AssetWriterError::NotFound
/// [`AssetReader`]: crate::io::AssetReader
/// [`default_writer`]: crate::io::AssetSource::default_writer
///
/// # Examples
///
/// ```rust
/// use futures_lite::future::block_on;
/// use futures_lite::io::AsyncWriteExt;
/// use std::path::Path;
/// use zlim_asset::io::AssetWriter;
/// use zlim_asset::io::Writer;
/// use zlim_asset::io::memory::MemoryAssetWriter;
///
/// let writer = MemoryAssetWriter::default();
///
/// // Stream the payload yourself …
/// let mut file = block_on(writer.write(Path::new("a.txt"))).unwrap();
/// block_on(file.write_all_bytes(b"hello")).unwrap();
/// block_on(file.flush()).unwrap();
///
/// // … or hand over a slice.
/// block_on(writer.write_bytes(Path::new("b.txt"), b"world")).unwrap();
///
/// assert_eq!(writer.root.get_asset(Path::new("a.txt")).unwrap().value(), b"hello");
/// assert_eq!(writer.root.get_asset(Path::new("b.txt")).unwrap().value(), b"world");
/// ```
pub trait AssetWriter: Send + Sync + 'static {
    /// Returns a future for the writer of the full asset bytes at the provided path.
    ///
    /// The returned stream must be flushed before the asset is complete.
    fn write<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = Result<impl Writer + 'a, AssetWriterError>> + Send;

    /// Returns a future for the writer of the full asset meta bytes at the provided path.
    ///
    /// This _should not_ include storage specific extensions like `.meta`.
    fn write_meta<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = Result<impl Writer + 'a, AssetWriterError>> + Send;

    /// Removes the asset stored at the given path.
    ///
    /// # Errors
    ///
    /// [`NotFound`](AssetWriterError::NotFound) when there is no asset at `path`.
    fn remove<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = Result<(), AssetWriterError>> + Send;

    /// Removes the asset meta stored at the given path.
    ///
    /// This _should not_ include storage specific extensions like `.meta`.
    fn remove_meta<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = Result<(), AssetWriterError>> + Send;

    /// Renames the asset at `old_path` to `new_path`.
    ///
    /// Missing parent directories of `new_path` are created.
    fn rename<'a>(
        &'a self,
        old_path: &'a Path,
        new_path: &'a Path,
    ) -> impl Future<Output = Result<(), AssetWriterError>> + Send;

    /// Renames the asset meta for the asset at `old_path` to `new_path`.
    ///
    /// This _should not_ include storage specific extensions like `.meta`.
    fn rename_meta<'a>(
        &'a self,
        old_path: &'a Path,
        new_path: &'a Path,
    ) -> impl Future<Output = Result<(), AssetWriterError>> + Send;

    /// Creates a directory at the given path, including all missing parent directories.
    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = Result<(), AssetWriterError>> + Send;

    /// Removes the directory at the given path, including all assets and directories in it.
    fn remove_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = Result<(), AssetWriterError>> + Send;

    /// Removes the directory at the given path, but only if it is completely empty.
    ///
    /// # Errors
    ///
    /// [`DirectoryNotEmpty`](AssetWriterError::DirectoryNotEmpty) when it still has entries.
    fn remove_empty_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = Result<(), AssetWriterError>> + Send;

    /// Removes all assets (and directories) in this directory, resulting in an empty directory.
    ///
    /// Unlike [`remove_directory`](Self::remove_directory) the directory itself is kept.
    fn remove_assets_in_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = Result<(), AssetWriterError>> + Send;

    /// Writes the asset `bytes` to the given `path`.
    ///
    /// Convenience provided by the trait: it opens [`write`](Self::write), hands the slice to
    /// [`Writer::write_all_bytes`], and flushes, so a source only implements the streaming
    /// form.
    fn write_bytes<'a>(
        &'a self,
        path: &'a Path,
        bytes: &'a [u8],
    ) -> impl Future<Output = Result<(), AssetWriterError>> + Send {
        async {
            let mut writer = self.write(path).await?;
            writer.write_all_bytes(bytes).await?;
            writer.flush().await?;
            Ok(())
        }
    }

    /// Writes the asset meta `bytes` to the given `path`.
    ///
    /// The metadata counterpart of [`write_bytes`](Self::write_bytes).
    fn write_meta_bytes<'a>(
        &'a self,
        path: &'a Path,
        bytes: &'a [u8],
    ) -> impl Future<Output = Result<(), AssetWriterError>> + Send {
        async {
            let mut meta_writer = self.write_meta(path).await?;
            meta_writer.write_all_bytes(bytes).await?;
            meta_writer.flush().await?;
            Ok(())
        }
    }
}

// -----------------------------------------------------------------------------
// ErasedAssetWriter

type BoxedAssetWriterFuture<'a, T> = crate::BoxedFuture<'a, Result<T, AssetWriterError>>;

/// A type-erased [`AssetWriter`] with boxed futures, used internally by the asset server.
///
/// Every `AssetWriter` implements this automatically and sources store writers as
/// `Box<dyn ErasedAssetWriter>`, so savers and the asset processor can work with any backend.
/// Prefer implementing [`AssetWriter`] and let the blanket implementation do the boxing.
///
/// # Examples
///
/// ```rust
/// use futures_lite::future::block_on;
/// use std::path::Path;
/// use zlim_asset::io::{ErasedAssetWriter, memory::MemoryAssetWriter};
///
/// let memory = MemoryAssetWriter::default();
/// let writer: Box<dyn ErasedAssetWriter> = Box::new(memory);
/// block_on(writer.write_bytes(Path::new("a.txt"), b"asset")).unwrap();
/// ```
pub trait ErasedAssetWriter: Send + Sync + 'static {
    /// Returns a future for the writer of the full asset bytes at the provided path.
    fn write<'a>(&'a self, path: &'a Path) -> BoxedAssetWriterFuture<'a, Box<dyn Writer + 'a>>;

    /// Returns a future for the writer of the full asset meta bytes at the provided path.
    ///
    /// This _should not_ include storage specific extensions like `.meta`.
    fn write_meta<'a>(&'a self, path: &'a Path)
    -> BoxedAssetWriterFuture<'a, Box<dyn Writer + 'a>>;

    /// Removes the asset stored at the given path.
    fn remove<'a>(&'a self, path: &'a Path) -> BoxedAssetWriterFuture<'a, ()>;

    /// Removes the asset meta stored at the given path.
    ///
    /// This _should not_ include storage specific extensions like `.meta`.
    fn remove_meta<'a>(&'a self, path: &'a Path) -> BoxedAssetWriterFuture<'a, ()>;

    /// Renames the asset at `old_path` to `new_path`.
    fn rename<'a>(
        &'a self,
        old_path: &'a Path,
        new_path: &'a Path,
    ) -> BoxedAssetWriterFuture<'a, ()>;

    /// Renames the asset meta for the asset at `old_path` to `new_path`.
    ///
    /// This _should not_ include storage specific extensions like `.meta`.
    fn rename_meta<'a>(
        &'a self,
        old_path: &'a Path,
        new_path: &'a Path,
    ) -> BoxedAssetWriterFuture<'a, ()>;

    /// Creates a directory at the given path, including all missing parent directories.
    fn create_directory<'a>(&'a self, path: &'a Path) -> BoxedAssetWriterFuture<'a, ()>;

    /// Removes the directory at the given path, including all assets and directories in it.
    fn remove_directory<'a>(&'a self, path: &'a Path) -> BoxedAssetWriterFuture<'a, ()>;

    /// Removes the directory at the given path, but only if it is completely empty.
    fn remove_empty_directory<'a>(&'a self, path: &'a Path) -> BoxedAssetWriterFuture<'a, ()>;

    /// Removes all assets (and directories) in this directory, resulting in an empty directory.
    fn remove_assets_in_directory<'a>(&'a self, path: &'a Path) -> BoxedAssetWriterFuture<'a, ()>;

    /// Writes the asset `bytes` to the given `path`.
    fn write_bytes<'a>(&'a self, path: &'a Path, bytes: &'a [u8])
    -> BoxedAssetWriterFuture<'a, ()>;

    /// Writes the asset meta `bytes` to the given `path`.
    fn write_meta_bytes<'a>(
        &'a self,
        path: &'a Path,
        bytes: &'a [u8],
    ) -> BoxedAssetWriterFuture<'a, ()>;
}

impl<T: AssetWriter> ErasedAssetWriter for T {
    fn write<'a>(&'a self, path: &'a Path) -> BoxedAssetWriterFuture<'a, Box<dyn Writer + 'a>> {
        Box::pin(async move { Ok(<T as AssetWriter>::write(self, path).await?.into_boxed()) })
    }

    fn write_meta<'a>(
        &'a self,
        path: &'a Path,
    ) -> BoxedAssetWriterFuture<'a, Box<dyn Writer + 'a>> {
        Box::pin(async move {
            Ok(<T as AssetWriter>::write_meta(self, path)
                .await?
                .into_boxed())
        })
    }

    fn remove<'a>(&'a self, path: &'a Path) -> BoxedAssetWriterFuture<'a, ()> {
        Box::pin(<T as AssetWriter>::remove(self, path))
    }

    fn remove_meta<'a>(&'a self, path: &'a Path) -> BoxedAssetWriterFuture<'a, ()> {
        Box::pin(<T as AssetWriter>::remove_meta(self, path))
    }

    fn rename<'a>(
        &'a self,
        old_path: &'a Path,
        new_path: &'a Path,
    ) -> BoxedAssetWriterFuture<'a, ()> {
        Box::pin(<T as AssetWriter>::rename(self, old_path, new_path))
    }

    fn rename_meta<'a>(
        &'a self,
        old_path: &'a Path,
        new_path: &'a Path,
    ) -> BoxedAssetWriterFuture<'a, ()> {
        Box::pin(<T as AssetWriter>::rename_meta(self, old_path, new_path))
    }

    fn create_directory<'a>(&'a self, path: &'a Path) -> BoxedAssetWriterFuture<'a, ()> {
        Box::pin(<T as AssetWriter>::create_directory(self, path))
    }

    fn remove_directory<'a>(&'a self, path: &'a Path) -> BoxedAssetWriterFuture<'a, ()> {
        Box::pin(<T as AssetWriter>::remove_directory(self, path))
    }

    fn remove_empty_directory<'a>(&'a self, path: &'a Path) -> BoxedAssetWriterFuture<'a, ()> {
        Box::pin(<T as AssetWriter>::remove_empty_directory(self, path))
    }

    fn remove_assets_in_directory<'a>(&'a self, path: &'a Path) -> BoxedAssetWriterFuture<'a, ()> {
        Box::pin(<T as AssetWriter>::remove_assets_in_directory(self, path))
    }

    fn write_bytes<'a>(
        &'a self,
        path: &'a Path,
        bytes: &'a [u8],
    ) -> BoxedAssetWriterFuture<'a, ()> {
        Box::pin(<T as AssetWriter>::write_bytes(self, path, bytes))
    }

    fn write_meta_bytes<'a>(
        &'a self,
        path: &'a Path,
        bytes: &'a [u8],
    ) -> BoxedAssetWriterFuture<'a, ()> {
        Box::pin(<T as AssetWriter>::write_meta_bytes(self, path, bytes))
    }
}

// -----------------------------------------------------------------------------
