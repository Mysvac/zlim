//! Shared helpers.

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::borrow::Cow;
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

// -----------------------------------------------------------------------------
// EmptyPathStream

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

/// Performs a read from the `slice` at the `bytes_read` cursor into `buf`.
///
/// At most `buf.len()` bytes are copied, the cursor is advanced by the number of bytes copied, and
/// that number is returned — `0` once the slice is exhausted.
#[inline]
pub(crate) fn slice_read(slice: &[u8], bytes_read: &mut usize, buf: &mut [u8]) -> usize {
    if *bytes_read >= slice.len() {
        return 0;
    }

    let src = &slice[(*bytes_read)..];

    // See `std::io::Read for &[u8]`
    let amt = core::cmp::min(buf.len(), src.len());

    // The boundary check is automatically eliminated in O3 optimization.
    buf[..amt].copy_from_slice(&src[..amt]);

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
// Loader extension

/// Normalizes a *static* loader extension as it is keyed in the loader registry.
///
/// Extensions are matched without the leading dot and case-insensitively, so `"png"`, `"PNG"`
/// and `".Png"` all have to end up as `"png"`. An extension that already is in that form is
/// returned as-is: its `&'static str` can be used as a key directly, so the interner is not
/// touched at all (the common case, since `AssetLoader::EXTENSIONS` is usually written in
/// lower case).
///
/// This is the side that *inserts* into the registry, whose keys have to be `&'static str`.
/// The look-up side takes a plain `&str` query and uses [`normalize_extension_ref`] instead,
/// which never interns.
///
/// [`AssetLoader::EXTENSIONS`]: crate::loader::AssetLoader::EXTENSIONS
#[inline]
pub(crate) fn normalize_extension(extension: &'static str) -> &'static str {
    let extension = extension.strip_prefix('.').unwrap_or(extension);

    match ensure_lowercase(extension) {
        Cow::Owned(lowercased) => zlim_utils::str::intern_str(&lowercased),
        Cow::Borrowed(extension) => extension,
    }
}

/// Returns a *runtime* extension in the form the loader registry keys on, without interning.
///
/// A `HashMap<&'static str, _>` accepts a `&str` query (`&'static str: Borrow<str>`), so a
/// look-up never has to produce a `&'static str`: the input is borrowed when it already is in
/// the registry's form, and rewritten into a temporary [`String`] otherwise (see
/// [`ensure_lowercase`]).
///
/// Interning would be wrong in this direction: the extension comes from an asset path, i.e.
/// from user data, so every distinct input would occupy the global string pool forever.
/// [`normalize_extension`] is the counterpart that *does* intern, and only the registry's keys
/// need it.
#[inline]
pub(crate) fn normalize_extension_ref(extension: &str) -> Cow<'_, str> {
    let extension = extension.strip_prefix('.').unwrap_or(extension);

    ensure_lowercase(extension)
}

/// Returns the lower-cased form of `extension`, borrowing it when it already is lower case.
///
/// Allocating only for the extensions that actually need it keeps the registry lookups
/// allocation-free for the usual lower-case asset names.
#[inline]
fn ensure_lowercase(extension: &str) -> Cow<'_, str> {
    if extension.bytes().all(|byte| !byte.is_ascii_uppercase()) {
        // ↑ cannot use `all(to_ascii_lowercase)`, which means all in `b'a'..=b'z'`.
        return Cow::Borrowed(extension);
    }

    ::core::hint::cold_path();
    Cow::Owned(extension.to_ascii_lowercase())
}

/// Normalizes a *runtime* extension and interns it, for a registry whose keys are `&'static str`.
///
/// This is the insert side of a registry that is configured explicitly — the extensions a processor
/// falls back to ([`AssetProcessors::register_extension`]), where the input is a plain `&str`
/// rather than a `&'static str` constant. Interning is what makes `HashMap<&'static str, _>` usable
/// there: the string pool lives until program exit, which is exactly as long as the mapping does
/// (it is only ever inserted into, never removed).
///
/// [`AssetProcessors::register_extension`]: crate::processor::AssetProcessors::register_extension
#[inline]
pub(crate) fn intern_extension(extension: &str) -> &'static str {
    zlim_utils::str::intern_str(&normalize_extension_ref(extension))
}

/// Returns an iterator over all *secondary* extensions of a full extension.
///
/// For a full extension `a.b.c` this yields `b.c` and `c`, so that a loader
/// registered for `c` can still be found for `foo.a.b.c` (the full extension
/// itself has to be tried by the caller first).
#[inline]
pub(crate) fn iter_secondary_extensions(full_extension: &str) -> impl Iterator<Item = &str> {
    // In UTF8, any ASCII byte is a valid character.
    full_extension.bytes().enumerate().filter_map(|(i, c)| {
        if c == b'.' {
            Some(&full_extension[i + 1..])
        } else {
            None
        }
    })
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
// Path normalization

/// Normalizes the path by collapsing all occurrences of '.' and '..' dot-segments where
/// possible as per [RFC 1808](https://datatracker.ietf.org/doc/html/rfc1808)
pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    let mut result_path = PathBuf::with_capacity(path.as_os_str().len());
    for elt in path.iter() {
        if elt == "." {
            // Skip
        } else if elt == ".." {
            // Note: If the result_path ends in `..`, `Path::file_name`
            // returns None, so we'll end up preserving it.
            if result_path.file_name().is_some() {
                result_path.pop();
            } else {
                // Preserve ".." if insufficient matches (per RFC 1808).
                result_path.push(elt);
            }
        } else {
            result_path.push(elt);
        }
    }
    result_path
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    #[track_caller]
    fn append_meta_extension(path: &Path) -> PathBuf {
        let mut meta_path = path.to_path_buf();
        let extension_str = path.extension().unwrap_or_default();
        // Directly `to_os_string` will cause a additional reallocation.
        let mut extension = OsString::with_capacity(extension_str.len() + 5);

        let cap = extension.capacity(); // <---------------

        extension.push(extension_str);
        if !extension.is_empty() {
            extension.push(".");
        }
        extension.push("meta");

        assert_eq!(cap, extension.capacity()); // <---------------

        meta_path.set_extension(extension);
        meta_path
    }

    /// The three path shapes the capacity arithmetic has to cover: one with no extension, where
    /// the buffer ends up holding only `meta`, and ones whose names already carry a dot or two.
    ///
    /// This test has no assertions of its own: the local copy above is what checks that appending
    /// `meta` never reallocates the `OsString`, which is the whole point of reserving `len + 5`.
    #[test]
    fn append_meta_capacity_fixed() {
        let _ = append_meta_extension(Path::new("123456"));
        let _ = append_meta_extension(Path::new("123456.txt"));
        let _ = append_meta_extension(Path::new("123456.txt.zip"));
    }

    /// The local copy above guards the `OsString` capacity; this pins the real function's output,
    /// so a regression in `super::append_meta_extension` cannot go unnoticed.
    #[test]
    fn append_meta_extension_appends_meta() {
        assert_eq!(
            super::append_meta_extension(Path::new("123456")),
            PathBuf::from("123456.meta")
        );
        assert_eq!(
            super::append_meta_extension(Path::new("123456.txt")),
            PathBuf::from("123456.txt.meta")
        );
    }
}
