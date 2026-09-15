//! An in-memory asset source, used by tests, by `embedded` assets and for runtime injection.
//!
//! The storage is a tree of [`Dir`] nodes: each node holds three maps — assets, metadata and
//! sub-directories — and the path a node was created under. Assets and their metadata are
//! kept in *separate* maps (rather than as a `foo` / `foo.meta` pair like the filesystem
//! source), which is why [`MemoryAssetReader::read_meta`] needs no suffix logic.
//!
//! Everything is behind one `Arc<RwLock<…>>` per node, so cloning a [`Dir`] (or copying a
//! [`MemoryAssetReader::root`]) gives another handle to the *same* tree. That is the usual way
//! to connect a writer to a reader:
//!
//! ```rust
//! # use std::path::Path;
//! # use futures_lite::future::block_on;
//! use zlim_asset::io::{AssetReader, AssetWriter};
//! use zlim_asset::io::memory::{MemoryAssetReader, MemoryAssetWriter};
//!
//! let reader = MemoryAssetReader::default();
//! let writer = MemoryAssetWriter { root: reader.root.clone() };
//!
//! block_on(writer.write_bytes(Path::new("models/level.ron"), b"level")).unwrap();
//! block_on(writer.write_meta_bytes(Path::new("models/level.ron"), b"meta")).unwrap();
//!
//! assert_eq!(block_on(reader.read_bytes(Path::new("models/level.ron"))).unwrap(), b"level");
//! assert_eq!(block_on(reader.read_meta_bytes(Path::new("models/level.ron"))).unwrap(), b"meta");
//! ```
//!
//! Reads never copy the payload: [`MemoryAssetReader`] hands out a `DataReader` over the
//! stored bytes (an `Arc<[u8]>` or a `&'static [u8]`), and `read_all_bytes` copies them into
//! the destination buffer in one go (see [`crate::io::future`]). Writes go through a
//! `DataWriter`, which buffers until it is flushed, so the tree only changes when the caller
//! flushes.
//!
//! Paths are interpreted relative to the root that the reader/writer was created with: `.` is
//! skipped and `..` walks back as far as it can. A component that cannot be resolved — a `..`
//! above the root, or a `Prefix` component like `C:` — is reported with a warning and skipped
//! by [`Dir::get_or_init_dir`], while [`Dir::get_dir`] turns it into `None`. Intermediate
//! directories are created on demand.

use core::fmt::Debug;
use core::pin::Pin;
use core::task::Poll;
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};

use futures_lite::Stream;
use futures_lite::io::{AsyncRead, AsyncSeek, AsyncWrite};
use zlim_utils::hash::HashMap;
use zlim_utils::vec::FastVec;

use super::{AssetReader, AssetReaderError};
use super::{AssetWriter, AssetWriterError, Writer};
use super::{Reader, ReaderNotSeekableError, SeekableReader};
use crate::io::future::{ReadAllFuture, WriteAllFuture};
use crate::utils::PathStream;

// -----------------------------------------------------------------------------
// Value

/// The bytes stored at a single path: shared or compile-time static.
///
/// Cloning is cheap — an `Arc` bump for [`Borrow`](Self::Borrow), a pointer copy for
/// [`Static`](Self::Static) — which is what lets [`Data`] and the readers hand out payloads
/// without copying. The `From` implementations make `insert_asset` accept any of the three
/// common shapes (owned `Vec`, shared `Arc<[u8]>`, or `&'static [u8]`/`&'static [u8; N]`, the
/// last one being what `embedded_asset!` produces via `include_bytes!`).
///
/// `Debug` prints the pointer and the length rather than the bytes, so that logging a large
/// asset does not dump its contents.
#[derive(Clone)]
pub enum Value {
    /// Bytes shared through an `Arc`.
    Borrow(Arc<[u8]>),
    /// Bytes baked into the binary.
    Static(&'static [u8]),
}

impl Debug for Value {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Borrow(arg) => {
                let ptr = <[u8]>::as_ptr(arg);
                let size = <[u8]>::len(arg);
                write!(f, "Value({ptr:p}, {size}B)")
            }
            Self::Static(arg) => {
                let ptr = <[u8]>::as_ptr(arg);
                let size = <[u8]>::len(arg);
                write!(f, "Value({ptr:p}, {size}B)")
            }
        }
    }
}

impl From<Vec<u8>> for Value {
    fn from(value: Vec<u8>) -> Self {
        Self::Borrow(Arc::from(value))
    }
}

impl From<Arc<[u8]>> for Value {
    #[inline]
    fn from(value: Arc<[u8]>) -> Self {
        Self::Borrow(value)
    }
}

impl From<&'static [u8]> for Value {
    #[inline]
    fn from(value: &'static [u8]) -> Self {
        Self::Static(value)
    }
}

impl From<&'static str> for Value {
    #[inline]
    fn from(value: &'static str) -> Self {
        Self::Static(value.as_bytes())
    }
}

impl<const N: usize> From<&'static [u8; N]> for Value {
    #[inline]
    fn from(value: &'static [u8; N]) -> Self {
        Self::Static(value)
    }
}

// -----------------------------------------------------------------------------
// Data

/// Bytes stored at a path, together with that path.
///
/// Returned by the [`Dir`] accessors and by [`MemoryAssetWriter::remove`];
/// [`value`](Self::value) borrows the payload, and the path is kept so callers
/// can report *which* file was involved (e.g. the watcher and folder loaders).
///
/// [`MemoryAssetWriter::remove`]: AssetWriter::remove
#[derive(Clone, Debug)]
pub struct Data {
    path: PathBuf,
    value: Value,
}

impl Data {
    /// The path that this data was written to.
    #[inline]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The value in bytes that was written here.
    #[inline]
    pub fn value(&self) -> &[u8] {
        match &self.value {
            Value::Borrow(value) => value,
            Value::Static(value) => value,
        }
    }
}

// -----------------------------------------------------------------------------
// Dir

#[derive(Default, Debug)]
struct DirInternal {
    path: PathBuf,
    dirs: HashMap<Box<str>, Dir>,
    assets: HashMap<Box<str>, Data>,
    metadata: HashMap<Box<str>, Data>,
}

/// One directory of the in-memory filesystem.
///
/// Cloning a `Dir` shares the same node (`Arc`), and every node keeps a handle to its children
/// but never to its parent — that is why `..` is resolved while walking a path instead of by
/// following a parent link.
///
/// The three maps are what make the in-memory source behave like a real filesystem:
///
/// - `assets` — payloads, keyed by file name;
/// - `metadata` — `.meta` payloads for those assets, same key space, separate map;
/// - `dirs` — sub-directories, recursively.
///
/// The path recorded in a node is its path relative to the root the tree was created for, so
/// the directory stream and [`Data::path`] can report source-relative paths.
///
/// All accessors tolerate a poisoned lock by taking the inner value
/// (`PoisonError::into_inner`), because a panic in unrelated code must not make the whole test
/// tree unusable.
///
/// # Examples
///
/// ```rust
/// use std::path::Path;
/// use zlim_asset::io::memory::Dir;
///
/// let root = Dir::default();
/// root.insert_asset_text(Path::new("models/level.ron"), "level");
/// root.insert_meta_text(Path::new("models/level.ron"), "meta");
///
/// assert_eq!(root.get_asset(Path::new("models/level.ron")).unwrap().value(), b"level");
/// assert_eq!(root.get_meta(Path::new("models/level.ron")).unwrap().value(), b"meta");
/// assert_eq!(root.get_dir(Path::new("models")).unwrap().path(), Path::new("models"));
/// assert!(root.get_asset(Path::new("nope.ron")).is_none());
/// ```
#[derive(Default, Clone, Debug)]
#[repr(transparent)]
pub struct Dir(Arc<RwLock<DirInternal>>);

impl Dir {
    /// Creates a directory node at `path`.
    pub fn new(path: PathBuf) -> Self {
        Self(Arc::new(RwLock::new(DirInternal {
            path,
            assets: HashMap::new(),
            metadata: HashMap::new(),
            dirs: HashMap::new(),
        })))
    }

    /// Gets or creates the directory at `path`, which must be relative to the root.
    ///
    /// - `.` and `..` are applied where possible; a missing parent is ignored with a warning.
    /// - `Prefix` components (e.g. `C:`) are ignored with a warning.
    /// - A `RootDir` component (e.g. the leading `/` of `/a/b`) restarts the walk at the root.
    pub fn get_or_init_dir(&self, path: &Path) -> Dir {
        let mut dir = self.clone();

        let size_hint = path.as_os_str().len();

        let mut full_path = PathBuf::with_capacity(size_hint);
        let mut buffer = FastVec::<Dir, 6>::new();
        let data = buffer.data();

        for c in path.components() {
            match c {
                std::path::Component::CurDir => continue,
                std::path::Component::ParentDir => {
                    // We cannot add reverse edges, as this
                    // would cause circular references.
                    if let Some(parent) = data.pop() {
                        dir = parent;
                    } else {
                        core::hint::cold_path();
                        zlim_log::warn!(
                            "Parent directory is non-existent, ignoring '..' component: `{}`.",
                            path.display()
                        );
                    }
                    continue;
                }
                std::path::Component::Normal(osstr) => {
                    full_path.push(c);
                    data.push(dir.clone());
                    let name: Box<str> = osstr.to_string_lossy().into();
                    let next_dir = dir
                        .0
                        .write()
                        .unwrap_or_else(PoisonError::into_inner)
                        .dirs
                        .entry(name)
                        .or_insert_with(|| Dir::new(full_path.clone()))
                        .clone();
                    dir = next_dir;
                }
                std::path::Component::RootDir => {
                    core::hint::cold_path();
                    data.clear();
                    full_path.clear();
                    dir = self.clone();
                    continue;
                }
                std::path::Component::Prefix(prefix) => {
                    core::hint::cold_path();
                    zlim_log::warn!(
                        "Prefix is unsupport, ignoring '{:?}' component: `{}`.",
                        prefix.as_os_str(),
                        path.display()
                    )
                }
            }
        }

        dir
    }

    fn insert_asset_internal(&self, path: &Path, value: Value) {
        let mut dir = self.clone();

        if let Some(parent) = path.parent() {
            dir = self.get_or_init_dir(parent);
        }

        // Separate to reduce the lock occupation time.
        let name: Box<str> = path.file_name().unwrap().to_string_lossy().into();
        let path: PathBuf = path.to_owned();
        let data: Data = Data { value, path };

        dir.0
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .assets
            .insert(name, data);
    }

    fn insert_meta_internal(&self, path: &Path, value: Value) {
        let mut dir = self.clone();

        if let Some(parent) = path.parent() {
            dir = self.get_or_init_dir(parent);
        }

        let name: Box<str> = path.file_name().unwrap().to_string_lossy().into();
        let path: PathBuf = path.to_owned();
        let data: Data = Data { value, path };

        dir.0
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .metadata
            .insert(name, data);
    }

    /// Inserts an asset at `path`, creating intermediate directories as needed.
    ///
    /// The value accepts anything convertible into [`Value`], so `Vec<u8>`, `Arc<[u8]>` and
    /// `&'static [u8]` all work. `path` must have a final component; the caller is responsible
    /// for that (the `AssetWriter` layer reports [`AssetWriterError::InvalidFilename`] first).
    pub fn insert_asset(&self, path: &Path, value: impl Into<Value>) {
        self.insert_asset_internal(path, value.into());
    }

    /// Inserts metadata at `path`, creating intermediate directories as needed.
    ///
    /// Metadata lives in its own map keyed by the *asset* path, so a metadata entry for
    /// `models/level.ron` is written with that same path — no `.meta` suffix.
    pub fn insert_meta(&self, path: &Path, value: impl Into<Value>) {
        self.insert_meta_internal(path, value.into());
    }

    /// Inserts an asset from a string at `path`.
    ///
    /// For a `&'static` string use [`Dir::insert_asset`] instead: it needs no allocation and
    /// no copy.
    pub fn insert_asset_text(&self, path: &Path, asset: &str) {
        let value = Value::Borrow(Arc::<[u8]>::from(asset.as_bytes()));
        self.insert_asset_internal(path, value);
    }

    /// Inserts metadata from a string at `path`.
    ///
    /// For a `&'static` string use [`Dir::insert_meta`] instead: it needs no allocation and
    /// no copy.
    pub fn insert_meta_text(&self, path: &Path, asset: &str) {
        let value = Value::Borrow(Arc::<[u8]>::from(asset.as_bytes()));
        self.insert_meta_internal(path, value);
    }

    /// Removes the asset at `path`, returning `None` when the path is invalid.
    ///
    /// Missing intermediate directories are treated as an invalid path, not as an empty
    /// directory. The removed [`Data`] is returned so callers can reuse the payload.
    pub fn remove_asset(&self, path: &Path) -> Option<Data> {
        let mut dir = self.clone();

        if let Some(parent) = path.parent() {
            dir = self.get_dir(parent)?;
        }

        let name: &str = path.file_name()?.to_str()?;
        dir.0
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .assets
            .remove(name)
    }

    /// Removes the metadata at `path`, returning `None` when the path is invalid.
    pub fn remove_meta(&self, path: &Path) -> Option<Data> {
        let mut dir = self.clone();
        if let Some(parent) = path.parent() {
            dir = self.get_dir(parent)?;
        }

        let name: &str = path.file_name()?.to_str()?;
        dir.0
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .metadata
            .remove(name)
    }

    /// Removes the directory at `path`, returning `None` when the path is invalid.
    ///
    /// The whole subtree goes away with it; the returned [`Dir`] keeps the removed node (and
    /// therefore its contents) alive for as long as it is held.
    ///
    /// Unlike [`remove_asset`] and [`remove_meta`], the parent is resolved with
    /// [`get_or_init_dir`], so a missing intermediate directory is created
    /// as a side effect even though the removal then returns `None`.
    ///
    /// [`remove_asset`]: Self::remove_asset
    /// [`remove_meta`]: Self::remove_meta
    /// [`get_or_init_dir`]: Self::get_or_init_dir
    pub fn remove_dir(&self, path: &Path) -> Option<Dir> {
        let mut dir = self.clone();
        if let Some(parent) = path.parent() {
            dir = self.get_or_init_dir(parent);
        }

        let name: &str = path.file_name()?.to_str()?;
        dir.0
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .dirs
            .remove(name)
    }

    /// Returns the directory at `path`, or `None` when it does not exist.
    ///
    /// Unlike [`get_or_init_dir`] this never creates anything, and it resolves
    /// `..` strictly: a `..` that would walk above the root makes the lookup fail
    /// instead of being ignored.
    ///
    /// [`get_or_init_dir`]: Self::get_or_init_dir
    pub fn get_dir(&self, path: &Path) -> Option<Dir> {
        let mut dir = self.clone();

        let mut buffer = FastVec::<Dir, 6>::new();
        let data = buffer.data();

        for c in path.components() {
            match c {
                std::path::Component::CurDir => continue,
                std::path::Component::RootDir => {
                    data.clear();
                    dir = self.clone();
                    continue;
                }
                std::path::Component::ParentDir => {
                    dir = data.pop()?;
                    continue;
                }
                std::path::Component::Normal(osstr) => {
                    let name = osstr.to_str().unwrap();
                    let next_dir = dir
                        .0
                        .read()
                        .unwrap_or_else(PoisonError::into_inner)
                        .dirs
                        .get(name)?
                        .clone();
                    dir = next_dir;
                }
                _ => {
                    core::hint::cold_path();
                    return None;
                }
            }
        }

        Some(dir)
    }

    /// Returns the asset at `path`, or `None` when it does not exist.
    ///
    /// The returned [`Data`] shares the payload, so reading it out does not copy the bytes.
    pub fn get_asset(&self, path: &Path) -> Option<Data> {
        let mut dir = self.clone();

        if let Some(parent) = path.parent() {
            dir = dir.get_dir(parent)?;
        }

        let name: &str = path.file_name()?.to_str()?;
        dir.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .assets
            .get(name)
            .cloned()
    }

    /// Returns the metadata at `path`, or `None` when it does not exist.
    ///
    /// `path` is the asset path, not a `.meta` path; metadata is stored in its own map.
    pub fn get_meta(&self, path: &Path) -> Option<Data> {
        let mut dir = self.clone();

        if let Some(parent) = path.parent() {
            dir = dir.get_dir(parent)?;
        }

        let name: &str = path.file_name()?.to_str()?;
        dir.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .metadata
            .get(name)
            .cloned()
    }

    /// Returns this directory's path, relative to the root of the tree.
    pub fn path(&self) -> PathBuf {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .path
            .clone()
    }
}

// -----------------------------------------------------------------------------
// DirStream

/// A [`Stream`] over the subdirectory and asset paths of a [`Dir`].
///
/// Produced by [`MemoryAssetReader::read_directory`]. Two properties matter to callers:
///
/// - paths are **relative to the root** of the tree (a child of `models/` yields
///   `models/level.ron`), matching the filesystem source;
/// - sub-directories are yielded before assets, each group in map order, and metadata files
///   are never listed (they are reachable only through `read_meta`).
///
/// The node is locked for the duration of each `poll_next` only, so it is safe to poll this
/// stream while writing to the tree from elsewhere.
struct DirStream {
    dir: Dir,
    index: usize,
    dir_index: usize,
}

impl DirStream {
    fn new(dir: Dir) -> Self {
        Self {
            dir,
            index: 0,
            dir_index: 0,
        }
    }
}

impl Stream for DirStream {
    type Item = PathBuf;

    fn poll_next(
        self: Pin<&mut Self>,
        _ctx: &mut core::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let dir = this.dir.0.read().unwrap_or_else(PoisonError::into_inner);

        let dir_index = this.dir_index;
        let dir_path = dir
            .dirs
            .keys()
            .nth(dir_index)
            .map(|d| dir.path.join(d.as_ref()));

        if let Some(dir_path) = dir_path {
            this.dir_index += 1;
            Poll::Ready(Some(dir_path))
        } else {
            let index = this.index;
            this.index += 1;
            let data = dir.assets.values().nth(index);
            Poll::Ready(data.map(|d| d.path().to_owned()))
        }
    }
}

// -----------------------------------------------------------------------------
// DataReader

struct DataReader {
    data: Data,
    bytes_read: usize,
}

impl AsyncRead for DataReader {
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut core::task::Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        use crate::utils::slice_read;
        let this = self.get_mut();
        let slice = this.data.value();
        let bytes_read = &mut this.bytes_read;
        Poll::Ready(Ok(slice_read(slice, bytes_read, buf)))
    }
}

impl AsyncSeek for DataReader {
    #[inline]
    fn poll_seek(
        self: Pin<&mut Self>,
        _cx: &mut core::task::Context<'_>,
        pos: std::io::SeekFrom,
    ) -> Poll<std::io::Result<u64>> {
        use crate::utils::slice_seek;
        let this = self.get_mut();
        let slice = this.data.value();
        let bytes_read = &mut this.bytes_read;
        Poll::Ready(slice_seek(slice, bytes_read, pos))
    }
}

impl Reader for DataReader {
    #[inline(always)]
    fn seekable(&mut self) -> Result<&mut dyn SeekableReader, ReaderNotSeekableError> {
        Ok(self)
    }

    #[inline]
    fn read_all_bytes<'a>(&'a mut self, buf: &'a mut Vec<u8>) -> ReadAllFuture<'a> {
        let start = self.bytes_read;
        let bytes = self.data.value();
        if start >= bytes.len() {
            ReadAllFuture::slice_read(&[], buf)
        } else {
            ReadAllFuture::slice_read(&bytes[start..], buf)
        }
    }
}

// -----------------------------------------------------------------------------
// MemoryAssetReader

/// An in-memory [`AssetReader`], primarily used by unit tests and embedded assets.
///
/// Holds a [`Dir`] root and answers reads from it. Cloning the reader (or copying
/// [`root`](Self::root) into a [`MemoryAssetWriter`]) shares the same tree, which is how a
/// writer is paired with a reader in tests; [`EmbeddedAssetRegistry`] likewise hands the same
/// `Dir` to every reader it builds from its own inserts.
///
/// [`EmbeddedAssetRegistry`]: crate::io::embedded::EmbeddedAssetRegistry
///
/// # Examples
///
/// ```rust
/// # use std::path::Path;
/// # use futures_lite::future::block_on;
/// use zlim_asset::io::{AssetReader, memory::MemoryAssetReader};
///
/// let reader = MemoryAssetReader::default();
/// reader.root.insert_asset_text(Path::new("tex/icon.png"), "png bytes");
///
/// assert!(block_on(reader.is_directory(Path::new("tex"))).unwrap());
/// assert_eq!(block_on(reader.read_bytes(Path::new("tex/icon.png"))).unwrap(), b"png bytes");
/// assert!(block_on(reader.read_bytes(Path::new("tex/missing.png"))).is_err());
/// ```
#[derive(Default, Clone)]
pub struct MemoryAssetReader {
    /// The root of the in-memory filesystem backing this reader.
    pub root: Dir,
}

impl Debug for MemoryAssetReader {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.pad("MemoryAssetReader")
    }
}

impl AssetReader for MemoryAssetReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        match self.root.get_asset(path) {
            Some(data) => Ok(DataReader {
                data,
                bytes_read: 0,
            }),
            None => Err(AssetReaderError::NotFound(path.to_path_buf())),
        }
    }

    async fn read_meta<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        match self.root.get_meta(path) {
            Some(data) => Ok(DataReader {
                data,
                bytes_read: 0,
            }),
            None => Err(AssetReaderError::NotFound(path.to_path_buf())),
        }
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<PathStream>, AssetReaderError> {
        match self.root.get_dir(path) {
            Some(dir) => Ok(Box::new(DirStream::new(dir))),
            None => Err(AssetReaderError::NotFound(path.to_path_buf())),
        }
    }

    async fn is_directory<'a>(&'a self, path: &'a Path) -> Result<bool, AssetReaderError> {
        Ok(self.root.get_dir(path).is_some())
    }
}

// -----------------------------------------------------------------------------
// DataWriter

struct DataWriter {
    /// The dir to write to.
    dir: Dir,
    /// The path to write to.
    path: PathBuf,
    /// The current buffer of data.
    ///
    /// This will include data that has been flushed already.
    current_data: Vec<u8>,
    /// Whether to write to the data or to the meta.
    is_meta_writer: bool,
}

impl AsyncWrite for DataWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut core::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        this.current_data.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _: &mut core::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        let data = self.current_data.as_slice();
        let value = Arc::<[u8]>::from(data);
        let path = &self.path;
        if self.is_meta_writer {
            self.dir.insert_meta(path, value);
        } else {
            self.dir.insert_asset(path, value);
        }
        Poll::Ready(Ok(()))
    }

    fn poll_close(
        self: Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        self.poll_flush(cx)
    }
}

impl Writer for DataWriter {
    #[inline]
    fn write_all_bytes<'a>(&'a mut self, buf: &'a [u8]) -> WriteAllFuture<'a> {
        WriteAllFuture::vec_write(&mut self.current_data, buf)
    }
}

// -----------------------------------------------------------------------------
// MemoryAssetWriter

/// An in-memory [`AssetWriter`], primarily used by unit tests and embedded assets.
///
/// Pair it with a [`MemoryAssetReader`] by sharing the root:
///
/// ```rust
/// # use std::path::Path;
/// # use futures_lite::future::block_on;
/// # use zlim_asset::io::{AssetReader, AssetWriter};
/// use zlim_asset::io::memory::{MemoryAssetReader, MemoryAssetWriter};
///
/// let reader = MemoryAssetReader::default();
/// let writer = MemoryAssetWriter { root: reader.root.clone() };
///
/// block_on(writer.write_bytes(Path::new("a.txt"), b"asset")).unwrap();
/// assert_eq!(block_on(reader.read_bytes(Path::new("a.txt"))).unwrap(), b"asset");
///
/// block_on(writer.rename(Path::new("a.txt"), Path::new("nested/b.txt"))).unwrap();
/// assert!(block_on(reader.read_bytes(Path::new("a.txt"))).is_err());
/// assert_eq!(block_on(reader.read_bytes(Path::new("nested/b.txt"))).unwrap(), b"asset");
/// ```
///
/// Writes are buffered by the returned stream and only appear in the tree once it is flushed;
/// [`AssetWriter::write_bytes`] does that for the caller.
#[derive(Default, Clone)]
pub struct MemoryAssetWriter {
    /// The root of the in-memory filesystem backing this writer.
    pub root: Dir,
}

impl Debug for MemoryAssetWriter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.pad("MemoryAssetWriter")
    }
}

impl AssetWriter for MemoryAssetWriter {
    async fn write<'a>(&'a self, path: &'a Path) -> Result<impl Writer + 'a, AssetWriterError> {
        if path.file_name().is_none() {
            return Err(AssetWriterError::InvalidFilename(path.to_path_buf()));
        }
        Ok(DataWriter {
            dir: self.root.clone(),
            path: path.to_owned(),
            current_data: Vec::new(),
            is_meta_writer: false,
        })
    }

    async fn write_meta<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<impl Writer + 'a, AssetWriterError> {
        if path.file_name().is_none() {
            return Err(AssetWriterError::InvalidFilename(path.to_path_buf()));
        }
        Ok(DataWriter {
            dir: self.root.clone(),
            path: path.to_owned(),
            current_data: Vec::new(),
            is_meta_writer: true,
        })
    }

    async fn remove<'a>(&'a self, path: &'a Path) -> Result<(), AssetWriterError> {
        if self.root.remove_asset(path).is_none() {
            return Err(AssetWriterError::NotFound(path.to_path_buf()));
        }
        Ok(())
    }

    async fn remove_meta<'a>(&'a self, path: &'a Path) -> Result<(), AssetWriterError> {
        if self.root.remove_meta(path).is_none() {
            return Err(AssetWriterError::NotFound(path.to_path_buf()));
        }
        Ok(())
    }

    async fn rename<'a>(
        &'a self,
        old_path: &'a Path,
        new_path: &'a Path,
    ) -> Result<(), AssetWriterError> {
        if new_path.file_name().is_none() {
            return Err(AssetWriterError::InvalidFilename(new_path.to_path_buf()));
        }
        let Some(old_asset) = self.root.get_asset(old_path) else {
            return Err(AssetWriterError::NotFound(old_path.to_path_buf()));
        };
        if old_path != new_path {
            // Remove the asset after instead of before since otherwise there'd be a
            // moment where the Dir is unlocked and missing both the old and new paths.
            self.root.insert_asset(new_path, old_asset.value);
            self.root.remove_asset(old_path);
        }
        Ok(())
    }

    async fn rename_meta<'a>(
        &'a self,
        old_path: &'a Path,
        new_path: &'a Path,
    ) -> Result<(), AssetWriterError> {
        if new_path.file_name().is_none() {
            return Err(AssetWriterError::InvalidFilename(new_path.to_path_buf()));
        }
        let Some(old_asset) = self.root.get_meta(old_path) else {
            return Err(AssetWriterError::NotFound(old_path.to_path_buf()));
        };

        if old_path != new_path {
            // Remove the meta after instead of before since otherwise there'd be a
            // moment where the Dir is unlocked and missing both the old and new paths.
            self.root.insert_meta(new_path, old_asset.value);
            self.root.remove_meta(old_path);
        }

        Ok(())
    }

    async fn create_directory<'a>(&'a self, path: &'a Path) -> Result<(), AssetWriterError> {
        self.root.get_or_init_dir(path);
        Ok(())
    }

    async fn remove_directory<'a>(&'a self, path: &'a Path) -> Result<(), AssetWriterError> {
        if self.root.remove_dir(path).is_none() {
            return Err(AssetWriterError::NotFound(path.to_path_buf()));
        }
        Ok(())
    }

    async fn remove_empty_directory<'a>(&'a self, path: &'a Path) -> Result<(), AssetWriterError> {
        let Some(dir) = self.root.get_dir(path) else {
            return Err(AssetWriterError::NotFound(path.to_path_buf()));
        };

        let dir = dir.0.read().unwrap();

        if !dir.assets.is_empty() || !dir.metadata.is_empty() || !dir.dirs.is_empty() {
            return Err(AssetWriterError::DirectoryNotEmpty(path.to_path_buf()));
        }

        self.root.remove_dir(path);
        Ok(())
    }

    async fn remove_assets_in_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<(), AssetWriterError> {
        let Some(dir) = self.root.get_dir(path) else {
            return Err(AssetWriterError::NotFound(path.to_path_buf()));
        };

        let mut dir = dir.0.write().unwrap();

        dir.assets.clear();
        dir.dirs.clear();
        dir.metadata.clear();
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use futures_lite::StreamExt;
    use futures_lite::future::block_on;

    use super::*;

    #[test]
    fn memory_dir() {
        let dir = Dir::default();
        let a_path = Path::new("a.txt");
        let a_data = "a".as_bytes().to_vec();
        let a_meta = "ameta".as_bytes().to_vec();

        dir.insert_asset(a_path, Arc::from(a_data.as_slice()));
        let asset = dir.get_asset(a_path).unwrap();
        assert_eq!(asset.path(), a_path);
        assert_eq!(asset.value(), a_data);

        dir.insert_meta(a_path, Arc::from(a_meta.as_slice()));
        let meta = dir.get_meta(a_path).unwrap();
        assert_eq!(meta.path(), a_path);
        assert_eq!(meta.value(), a_meta);

        let b_path = Path::new("x/y/b.txt");
        let b_data = "b".as_bytes().to_vec();
        let b_meta = "meta".as_bytes().to_vec();
        dir.insert_asset(b_path, Arc::from(b_data.as_slice()));
        dir.insert_meta(b_path, Arc::from(b_meta.as_slice()));

        let asset = dir.get_asset(b_path).unwrap();
        assert_eq!(asset.path(), b_path);
        assert_eq!(asset.value(), b_data);

        let meta = dir.get_meta(b_path).unwrap();
        assert_eq!(meta.path(), b_path);
        assert_eq!(meta.value(), b_meta);
    }

    /// A writer built over the reader's own root round-trips: what it writes comes back through
    /// the reader, a written path's parent counts as a directory while the file itself does not,
    /// and an unknown path is `NotFound` rather than some other failure.
    #[test]
    fn memory_source_roundtrip() {
        let reader = MemoryAssetReader::default();
        let writer = MemoryAssetWriter {
            root: reader.root.clone(),
        };

        block_on(writer.write_bytes(Path::new("models/level.ron"), b"level")).unwrap();
        block_on(writer.write_meta_bytes(Path::new("models/level.ron"), b"meta")).unwrap();

        assert_eq!(
            block_on(reader.read_bytes(Path::new("models/level.ron"))).unwrap(),
            b"level"
        );
        assert_eq!(
            block_on(reader.read_meta_bytes(Path::new("models/level.ron"))).unwrap(),
            b"meta"
        );
        assert!(block_on(reader.is_directory(Path::new("models"))).unwrap());
        assert!(!block_on(reader.is_directory(Path::new("models/level.ron"))).unwrap());
        assert!(matches!(
            block_on(reader.read_bytes(Path::new("nope.ron"))),
            Err(AssetReaderError::NotFound(_))
        ));
    }

    /// Listing and mutating through the in-memory tree: a listing yields the immediate children
    /// only, a rename moves the bytes, and a directory that still holds something is refused by
    /// the empty-only removal but taken — with its subtree — by the plain one.
    #[test]
    fn memory_directory_stream_and_mutation() {
        let reader = MemoryAssetReader::default();
        let writer = MemoryAssetWriter {
            root: reader.root.clone(),
        };

        block_on(writer.write_bytes(Path::new("dir/a.txt"), b"a")).unwrap();
        block_on(writer.write_bytes(Path::new("dir/sub/b.txt"), b"b")).unwrap();
        block_on(writer.write_meta_bytes(Path::new("dir/a.txt"), b"ameta")).unwrap();

        let mut directory = block_on(reader.read_directory(Path::new("dir"))).unwrap();
        let mut entries = Vec::new();
        while let Some(entry) = block_on(directory.next()) {
            entries.push(entry);
        }
        // The listing is one level deep and hides the metadata, and its order is unspecified.
        entries.sort();
        assert_eq!(
            entries,
            vec![PathBuf::from("dir/a.txt"), PathBuf::from("dir/sub")]
        );

        // A rename moves the bytes rather than copying them: the old path stops resolving.
        block_on(writer.rename(Path::new("dir/a.txt"), Path::new("dir/b.txt"))).unwrap();
        assert!(block_on(reader.read_bytes(Path::new("dir/a.txt"))).is_err());
        assert_eq!(
            block_on(reader.read_bytes(Path::new("dir/b.txt"))).unwrap(),
            b"a"
        );

        // The directory still holds `dir/sub`, so the removal that insists on empty refuses it.
        assert!(matches!(
            block_on(writer.remove_empty_directory(Path::new("dir"))),
            Err(AssetWriterError::DirectoryNotEmpty(_))
        ));
        // The plain removal takes the whole subtree with it.
        block_on(writer.remove_directory(Path::new("dir"))).unwrap();
        assert!(!block_on(reader.is_directory(Path::new("dir"))).unwrap());
    }
}
