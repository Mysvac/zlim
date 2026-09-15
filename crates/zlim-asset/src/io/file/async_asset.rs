//! Asynchronous filesystem asset reader and writer implementations.

use core::pin::Pin;
use core::task::Poll;
use std::path::{Path, PathBuf};

use async_fs::File;
use futures_lite::StreamExt;
use futures_lite::io::{AsyncRead, AsyncWrite};

use super::{FileAssetReader, FileAssetWriter};
use crate::io::future::{ReadAllFuture, WriteAllFuture};
use crate::io::{AssetReader, AssetReaderError, Reader, ReaderNotSeekableError, SeekableReader};
use crate::io::{AssetWriter, AssetWriterError, Writer};
use crate::utils::{append_meta_extension, PathStream};

// -----------------------------------------------------------------------------
// Open File Limiter

#[cfg(windows)]
use core::marker::PhantomData;

#[cfg(not(windows))]
use async_lock::{Semaphore, SemaphoreGuard};

// Set to OS default limit / 2
// macos & ios: 256 -> 128
// the other non-Windows targets (linux, android, …): 1024 -> 512
// windows: none
//
// The permit is held as long as the reader/writer lives, so producers get back-pressure
// instead of an `EMFILE` from the OS.
#[cfg(any(target_os = "macos", target_os = "ios"))]
static OPEN_FILE_LIMITER: Semaphore = Semaphore::new(128);

#[cfg(not(any(target_os = "macos", target_os = "ios", windows)))]
static OPEN_FILE_LIMITER: Semaphore = Semaphore::new(512);

// -----------------------------------------------------------------------------
// FileReader

struct FileReader {
    file: File,
    /// Keeps the field set identical to the non-Windows branch; Windows needs no permit.
    #[cfg(windows)]
    _guard: PhantomData<()>,
    /// Reserves this file's descriptor slot; never read.
    #[cfg(not(windows))]
    _guard: SemaphoreGuard<'static>,
}

impl AsyncRead for FileReader {
    #[inline(always)]
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.file).poll_read(cx, buf)
    }
}

impl Reader for FileReader {
    #[inline(always)]
    fn seekable(&mut self) -> Result<&mut dyn SeekableReader, ReaderNotSeekableError> {
        Ok(&mut self.file)
    }

    #[inline(always)]
    fn read_all_bytes<'a>(&'a mut self, buf: &'a mut Vec<u8>) -> ReadAllFuture<'a> {
        ReadAllFuture::async_read::<File>(&mut self.file, buf)
    }
}

// -----------------------------------------------------------------------------
// AssetReader

#[cold]
fn map_reader_error(e: std::io::Error, path: PathBuf) -> AssetReaderError {
    use std::io::ErrorKind;
    match e.kind() {
        ErrorKind::NotFound => AssetReaderError::NotFound(path),
        _ => AssetReaderError::from(e),
    }
}

impl AssetReader for FileAssetReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        #[cfg(windows)]
        let _guard = PhantomData;
        #[cfg(not(windows))]
        let _guard = OPEN_FILE_LIMITER.acquire().await;

        let full_path = self.root_path.join(path);
        match File::open(&full_path).await {
            Ok(file) => Ok(FileReader { file, _guard }),
            Err(e) => Err(map_reader_error(e, full_path)),
        }
    }

    async fn read_meta<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        #[cfg(windows)]
        let _guard = PhantomData;
        #[cfg(not(windows))]
        let _guard = OPEN_FILE_LIMITER.acquire().await;

        let meta_path = append_meta_extension(path);
        let full_path = self.root_path.join(meta_path);
        match File::open(&full_path).await {
            Ok(file) => Ok(FileReader { file, _guard }),
            Err(e) => Err(map_reader_error(e, full_path)),
        }
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<PathStream>, AssetReaderError> {
        let full_path = self.root_path.join(path);

        let read_dir = match async_fs::read_dir(&full_path).await {
            Ok(read_dir) => read_dir,
            Err(e) => return Err(map_reader_error(e, full_path)),
        };

        let root_path = self.root_path.clone();

        let mapped_stream = read_dir.filter_map(move |f| {
            let dir_entry = f.ok()?;
            let path = dir_entry.path();
            // filter out meta files as they are not considered assets
            if let Some(ext) = path.extension().and_then(|e| e.to_str())
                && ext.eq_ignore_ascii_case("meta")
            {
                return None;
            }

            // filter out hidden files. they are not listed by default but are directly targetable
            if let Some(file_name) = path.file_name()
                && file_name.as_encoded_bytes().first() == Some(&b'.')
            {
                return None;
            }

            let relative_path = path.strip_prefix(&root_path).unwrap();
            Some(relative_path.to_path_buf())
        });

        Ok(Box::new(mapped_stream))
    }

    async fn is_directory<'a>(&'a self, path: &'a Path) -> Result<bool, AssetReaderError> {
        let full_path = self.root_path.join(path);
        match full_path.metadata() {
            Ok(metadata) => Ok(metadata.file_type().is_dir()),
            Err(e) => Err(map_reader_error(e, full_path)),
        }
    }
}

// -----------------------------------------------------------------------------
// FileWriter

impl Writer for File {
    #[inline(always)]
    fn write_all_bytes<'a>(&'a mut self, buf: &'a [u8]) -> WriteAllFuture<'a> {
        WriteAllFuture::async_write::<File>(self, buf)
    }
}

struct FileWriter {
    file: File,
    /// Keeps the field set identical to the non-Windows branch; Windows needs no permit.
    #[cfg(windows)]
    _guard: PhantomData<()>,
    /// Reserves this file's descriptor slot; never read.
    #[cfg(not(windows))]
    _guard: SemaphoreGuard<'static>,
}

impl AsyncWrite for FileWriter {
    #[inline(always)]
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.file).poll_write(cx, buf)
    }

    #[inline(always)]
    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.file).poll_flush(cx)
    }

    #[inline(always)]
    fn poll_close(
        mut self: Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.file).poll_close(cx)
    }
}

impl Writer for FileWriter {
    #[inline(always)]
    fn write_all_bytes<'a>(&'a mut self, buf: &'a [u8]) -> WriteAllFuture<'a> {
        WriteAllFuture::async_write::<File>(&mut self.file, buf)
    }
}

// -----------------------------------------------------------------------------
// AssetWriter

#[cold]
fn map_write_error(e: std::io::Error, path: PathBuf) -> AssetWriterError {
    use std::io::ErrorKind;
    match e.kind() {
        ErrorKind::NotFound => AssetWriterError::NotFound(path),
        ErrorKind::InvalidFilename => AssetWriterError::InvalidFilename(path),
        ErrorKind::DirectoryNotEmpty => AssetWriterError::DirectoryNotEmpty(path),
        _ => AssetWriterError::from(e),
    }
}

impl AssetWriter for FileAssetWriter {
    async fn write<'a>(&'a self, path: &'a Path) -> Result<impl Writer + 'a, AssetWriterError> {
        #[cfg(windows)]
        let _guard = PhantomData;
        #[cfg(not(windows))]
        let _guard = OPEN_FILE_LIMITER.acquire().await;

        let full_path = self.root_path.join(path);
        if let Some(parent) = full_path.parent() {
            async_fs::create_dir_all(parent).await?;
        }

        match File::create(&full_path).await {
            Ok(file) => Ok(FileWriter { file, _guard }),
            Err(e) => Err(map_write_error(e, full_path)),
        }
    }

    async fn write_meta<'a>(&'a self, path: &'a Path) -> Result<impl Writer + 'a, AssetWriterError> {
        #[cfg(windows)]
        let _guard = PhantomData;
        #[cfg(not(windows))]
        let _guard = OPEN_FILE_LIMITER.acquire().await;

        let meta_path = append_meta_extension(path);
        let full_path = self.root_path.join(meta_path);

        if let Some(parent) = full_path.parent() {
            async_fs::create_dir_all(parent).await?;
        }

        match File::create(&full_path).await {
            Ok(file) => Ok(FileWriter { file, _guard }),
            Err(e) => Err(map_write_error(e, full_path)),
        }
    }

    async fn remove<'a>(&'a self, path: &'a Path) -> Result<(), AssetWriterError> {
        let full_path = self.root_path.join(path);
        async_fs::remove_file(&full_path)
            .await
            .map_err(|e| map_write_error(e, full_path))
    }

    async fn remove_meta<'a>(&'a self, path: &'a Path) -> Result<(), AssetWriterError> {
        let meta_path = append_meta_extension(path);
        let full_path = self.root_path.join(meta_path);
        async_fs::remove_file(&full_path)
            .await
            .map_err(|e| map_write_error(e, full_path))
    }

    async fn rename<'a>(
        &'a self,
        old_path: &'a Path,
        new_path: &'a Path,
    ) -> Result<(), AssetWriterError> {
        let full_old_path = self.root_path.join(old_path);
        let full_new_path = self.root_path.join(new_path);
        if let Some(parent) = full_new_path.parent() {
            async_fs::create_dir_all(parent).await?;
        }
        async_fs::rename(full_old_path, full_new_path)
            .await
            .map_err(AssetWriterError::from)
    }

    async fn rename_meta<'a>(
        &'a self,
        old_path: &'a Path,
        new_path: &'a Path,
    ) -> Result<(), AssetWriterError> {
        let old_meta_path = append_meta_extension(old_path);
        let new_meta_path = append_meta_extension(new_path);
        let full_old_path = self.root_path.join(old_meta_path);
        let full_new_path = self.root_path.join(new_meta_path);
        if let Some(parent) = full_new_path.parent() {
            async_fs::create_dir_all(parent).await?;
        }
        async_fs::rename(full_old_path, full_new_path)
            .await
            .map_err(AssetWriterError::from)
    }

    async fn create_directory<'a>(&'a self, path: &'a Path) -> Result<(), AssetWriterError> {
        let full_path = self.root_path.join(path);
        async_fs::create_dir_all(&full_path)
            .await
            .map_err(|e| map_write_error(e, full_path))
    }

    async fn remove_directory<'a>(&'a self, path: &'a Path) -> Result<(), AssetWriterError> {
        let full_path = self.root_path.join(path);
        async_fs::remove_dir_all(&full_path)
            .await
            .map_err(|e| map_write_error(e, full_path))
    }

    async fn remove_empty_directory<'a>(&'a self, path: &'a Path) -> Result<(), AssetWriterError> {
        let full_path = self.root_path.join(path);
        async_fs::remove_dir(&full_path)
            .await
            .map_err(|e| map_write_error(e, full_path))
    }

    async fn remove_assets_in_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<(), AssetWriterError> {
        let full_path = self.root_path.join(path);
        async_fs::remove_dir_all(&full_path).await?;
        async_fs::create_dir_all(&full_path).await?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use futures_lite::StreamExt;
    use futures_lite::future::block_on;

    use super::*;

    /// Creates a fresh temporary directory for one test.
    fn temp_dir(name: &str) -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "zlim-asset-async-file-{}-{}-{name}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn reader_at(dir: &Path) -> FileAssetReader {
        FileAssetReader {
            root_path: dir.to_path_buf(),
        }
    }

    fn writer_at(dir: &Path) -> FileAssetWriter {
        FileAssetWriter {
            root_path: dir.to_path_buf(),
        }
    }

    #[test]
    #[ignore = "manual trigger"]
    fn write_read_roundtrip() {
        let dir = temp_dir("roundtrip");
        let reader = reader_at(&dir);
        let writer = writer_at(&dir);

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

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A listing holds the asset files only: the `.meta` siblings written next to them, and files
    /// whose name starts with a dot, are both filtered out.
    #[test]
    #[ignore = "manual trigger"]
    fn directories_are_listed_without_meta_or_hidden_files() {
        let dir = temp_dir("list");
        let reader = reader_at(&dir);
        let writer = writer_at(&dir);

        block_on(writer.write_bytes(Path::new("models/a.ron"), b"a")).unwrap();
        block_on(writer.write_bytes(Path::new("models/b.ron"), b"b")).unwrap();
        block_on(writer.write_meta_bytes(Path::new("models/a.ron"), b"a")).unwrap();
        std::fs::write(dir.join("models/.hidden"), b"hidden").unwrap();

        assert!(block_on(reader.is_directory(Path::new("models"))).unwrap());
        assert!(!block_on(reader.is_directory(Path::new("models/a.ron"))).unwrap());

        let mut directory = block_on(reader.read_directory(Path::new("models"))).unwrap();
        let mut entries = Vec::new();
        while let Some(entry) = block_on(directory.next()) {
            entries.push(entry);
        }
        entries.sort();
        assert_eq!(
            entries,
            vec![
                PathBuf::from("models/a.ron"),
                PathBuf::from("models/b.ron")
            ]
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    #[ignore = "manual trigger"]
    fn missing_paths_report_not_found() {
        let dir = temp_dir("missing");
        let reader = reader_at(&dir);

        let error = block_on(reader.read_bytes(Path::new("nope.png"))).unwrap_err();
        assert!(matches!(error, AssetReaderError::NotFound(_)));

        std::fs::remove_dir_all(&dir).ok();
    }
}
