//! Shared helpers for the IO layer.

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::ffi::OsString;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};

use futures_lite::Stream;

// -----------------------------------------------------------------------------
// Alias

/// A boxed `Send` future, used by the type-erased IO traits.
pub type BoxedFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A stream of asset paths, relative to a source root.
pub type PathStream = dyn Stream<Item = PathBuf> + Unpin + Send;

/// A [`PathStream`] implementation that immediately returns nothing.
pub struct EmptyPathStream;

impl Stream for EmptyPathStream {
    type Item = PathBuf;

    #[inline(always)]
    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(None)
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(0))
    }
}

// -----------------------------------------------------------------------------
// Slice helpers

/// Performs a read from the `slice` into `buf`.
#[inline]
pub(crate) fn slice_read(slice: &[u8], bytes_read: &mut usize, buf: &mut [u8]) -> usize {
    if *bytes_read >= slice.len() {
        return 0;
    }

    let src = &slice[(*bytes_read)..];

    // See `std::io::Read for &[u8]`
    let amt = core::cmp::min(buf.len(), src.len());

    // The boundary check is automatically eliminated in O3 optimization.
    if amt == 1 {
        buf[0] = src[0];
    } else {
        buf[..amt].copy_from_slice(&src[..amt]);
    }

    *bytes_read += amt;

    amt
}

/// Calculates the position of a seek, returning an error when it is out of range.
#[inline]
pub(crate) fn slice_seek(
    slice: &[u8],
    bytes_read: &mut usize,
    pos: SeekFrom,
) -> std::io::Result<u64> {
    #[cold]
    #[inline(never)]
    fn make_overflow_error() -> std::io::Result<u64> {
        // TODO: Waiting unstable API `const_error!`.
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "seek position is out of range",
        ))
    }

    let (origin, offset) = match pos {
        SeekFrom::Current(offset) => (*bytes_read, Ok(offset)),
        SeekFrom::Start(offset) => (0, offset.try_into()),
        SeekFrom::End(offset) => (slice.len(), Ok(offset)),
    };

    if let Ok(offset_i64) = offset
        && let Ok(origin_i64) = i64::try_from(origin)
        && let Some(new_pos_i64) = origin_i64.checked_add(offset_i64)
        && let Ok(new_pos) = usize::try_from(new_pos_i64)
    {
        *bytes_read = new_pos;
        Ok(new_pos as u64)
    } else {
        make_overflow_error()
    }
}

// -----------------------------------------------------------------------------
// Meta path

/// Appends `.meta` to the given path (`foo` → `foo.meta`, `foo.bar` → `foo.bar.meta`).
pub(crate) fn append_meta_extension(path: &Path) -> PathBuf {
    let mut meta_path = path.to_path_buf();
    let extension_str = path.extension().unwrap_or_default();
    // Directly `to_os_string` will cause a additional reallocation.
    let mut extension = OsString::with_capacity(extension_str.len() + 5);
    extension.push(extension_str);
    if !extension.is_empty() {
        extension.push(".");
    }
    extension.push("meta");
    meta_path.set_extension(extension);
    meta_path
}

// -----------------------------------------------------------------------------
