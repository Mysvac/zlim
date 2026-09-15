//! Hand-written futures for the "give me everything at once" helpers.
//!
//! [`Reader::read_all_bytes`] and [`Writer::write_all_bytes`] are called on `dyn Reader` /
//! `dyn Writer` objects all the time, so they must not add another allocation on top of the
//! boxed trait object. This module therefore hand-writes the two futures instead of relying on
//! `async fn`, which would need boxing in a trait object.
//!
//! [`Reader::read_all_bytes`]: crate::io::Reader::read_all_bytes
//! [`Writer::write_all_bytes`]: crate::io::Writer::write_all_bytes
//!
//! # How one type covers several sources
//!
//! Each future stores a `fn(Pin<&mut Self>, &mut Context) -> Poll<_>` together with a
//! type-erased `NonNull` to the reader/writer. The constructor picks the function that
//! matches the erased type, and [`Future::poll`] simply forwards to it. That yields four
//! entry points sharing two types:
//!
//! | Constructor | Erased source | Behaviour |
//! |---|---|---|
//! | [`ReadAllFuture::async_read`] | `&mut R: AsyncRead` | loops `poll_read` until EOF |
//! | [`ReadAllFuture::slice_read`] | `&[u8]` | one `copy_nonoverlapping`, ready on first poll |
//! | [`WriteAllFuture::async_write`] | `&mut W: AsyncWrite` | loops `poll_write` until drained |
//! | [`WriteAllFuture::vec_write`] | `&mut Vec<u8>` | one `copy_nonoverlapping`, ready on first poll |
//!
//! The slice/`Vec` paths are what makes the in-memory sources
//! ([`crate::io::memory`]) and embedded assets cheap: no state machine, no per-byte loop.
//!
//! # Safety
//!
//! The type erasure is only sound because a future is created from a borrow that outlives it
//! (`&'a mut R` / `&'a [u8]`), and its `func` is always the instantiation matching that same
//! type — the two are set together and never changed afterwards. Additionally the read path
//! keeps a `Guard` so that a panic inside a user supplied `AsyncRead` implementation cannot
//! expose uninitialized bytes through the `Vec`.
#![expect(unsafe_code, reason = "pointer operation")]

use core::future::Future;
use core::pin::Pin;
use core::ptr::NonNull;
use core::task::{Context, Poll};

use futures_lite::io::{AsyncRead, AsyncWrite};
use futures_lite::ready;

// -----------------------------------------------------------------------------
// ReadAllFuture

/// A future that reads a reader to EOF, appending to a `Vec<u8>`.
///
/// Resolves to the number of bytes that were appended — the length of the buffer before the
/// read is not counted, so appending to a non-empty `Vec` reports only the new bytes.
///
/// Create one with [`ReadAllFuture::async_read`] (any [`AsyncRead`]) or
/// [`ReadAllFuture::slice_read`] (an in-memory slice). The future is `Unpin`, so it can be
/// polled without pinning, and it borrows both the source and the buffer for its whole life.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct ReadAllFuture<'a> {
    func: fn(Pin<&mut Self>, &mut Context<'_>) -> Poll<std::io::Result<usize>>,
    reader: NonNull<u8>,
    buffer: &'a mut Vec<u8>,
    data_size: usize,
    start_len: usize,
}

impl Unpin for ReadAllFuture<'_> {}
// SAFETY: the future only holds a borrow of the source plus a `&mut Vec<u8>`; both are `Send`
// / `Sync` for the constructors' bounds (`R: Send + Sync`), and the raw pointer never escapes.
unsafe impl Send for ReadAllFuture<'_> {}
unsafe impl Sync for ReadAllFuture<'_> {}

impl Future for ReadAllFuture<'_> {
    type Output = std::io::Result<usize>;

    #[inline(always)]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        (self.func)(self, cx)
    }
}

impl ReadAllFuture<'_> {
    /// Reads `reader` to EOF, appending everything to `output`.
    ///
    /// The buffer is grown geometrically; an error from the reader is returned as-is and
    /// leaves whatever was read so far in `output`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use futures_lite::future::block_on;
    /// use zlim_asset::io::VecReader;
    /// use zlim_asset::io::future::ReadAllFuture;
    ///
    /// let mut reader = VecReader::new(b"hello world".to_vec());
    /// let mut bytes = Vec::new();
    ///
    /// let appended = block_on(ReadAllFuture::async_read(&mut reader, &mut bytes)).unwrap();
    ///
    /// assert_eq!(appended, 11);
    /// assert_eq!(bytes, b"hello world");
    /// ```
    #[inline(always)]
    pub fn async_read<'a, R: AsyncRead + Send + Sync + Unpin>(
        reader: &'a mut R,
        output: &'a mut Vec<u8>,
    ) -> ReadAllFuture<'a> {
        let start_len = output.len();
        ReadAllFuture {
            func: async_read_internal::<R>,
            reader: NonNull::from_mut(reader).cast(),
            buffer: output,
            start_len,
            data_size: 0,
        }
    }

    /// Appends an in-memory slice to `output`, completing on the first poll.
    ///
    /// This is the fast path used by the in-memory and embedded sources: it reserves once and
    /// then does a single `copy_nonoverlapping`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use futures_lite::future::block_on;
    /// use zlim_asset::io::future::ReadAllFuture;
    ///
    /// let mut bytes = b"prefix: ".to_vec();
    /// let appended = block_on(ReadAllFuture::slice_read(b"data", &mut bytes)).unwrap();
    ///
    /// assert_eq!(appended, 4);
    /// assert_eq!(bytes, b"prefix: data");
    /// ```
    #[inline(always)]
    pub fn slice_read<'a>(reader: &'a [u8], output: &'a mut Vec<u8>) -> ReadAllFuture<'a> {
        let start_len = output.len();
        let data_size = reader.len();
        ReadAllFuture {
            func: slice_read_internal,
            reader: NonNull::from_ref(reader).cast(),
            buffer: output,
            start_len,
            data_size,
        }
    }
}

/// Loop `poll_read` until EOF, growing the buffer as needed.
fn async_read_internal<R: AsyncRead + Unpin>(
    this: Pin<&mut ReadAllFuture>,
    cx: &mut Context<'_>,
) -> Poll<std::io::Result<usize>> {
    let ReadAllFuture {
        reader,
        buffer,
        start_len,
        ..
    } = this.get_mut();

    let start_len = *start_len;
    // SAFETY: the future was constructed from a `&mut R` that outlives it (see the
    // constructor), and `func` is only ever `async_read_internal::<R>` for that `R`.
    let reader: &mut R = unsafe { reader.cast::<R>().as_mut() };
    let buf: &mut Vec<u8> = buffer;
    let mut rd: Pin<&mut R> = Pin::new(reader);

    /// Grows the buffer before each read and truncates it back to the readable length on
    /// drop, so that neither a `panic!` inside the reader nor an early return can leave
    /// uninitialized bytes visible through the `Vec`.
    struct Guard<'a> {
        buf: &'a mut Vec<u8>,
        len: usize,
    }

    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            self.buf.resize(self.len, 0);
        }
    }

    let mut guard = Guard {
        len: buf.len(),
        buf,
    };

    let result;

    loop {
        if guard.len == guard.buf.len() {
            guard.buf.reserve(32);
            let capacity = guard.buf.capacity();
            // Faster than `resize(capacity, 0)`, no need to reset memory.
            unsafe { guard.buf.set_len(capacity) };
        }

        match ready!(rd.as_mut().poll_read(cx, &mut guard.buf[guard.len..])) {
            Ok(0) => {
                result = Poll::Ready(Ok(guard.len - start_len));
                break;
            }
            Ok(n) => guard.len += n,
            Err(err) => {
                result = Poll::Ready(Err(err));
                break;
            }
        }
    }

    result
}

/// Copy a slice into the buffer, completing immediately.
fn slice_read_internal(
    this: Pin<&mut ReadAllFuture>,
    _cx: &mut Context<'_>,
) -> Poll<std::io::Result<usize>> {
    // Cold overflow path: keeps the hot path branch-free.
    #[cold]
    #[inline(never)]
    fn overflow() -> std::io::Error {
        std::io::ErrorKind::FileTooLarge.into()
    }

    let ReadAllFuture {
        reader,
        buffer,
        data_size,
        start_len,
        ..
    } = this.get_mut();

    let start_len: usize = *start_len;
    let data_size: usize = *data_size;
    let buf: &mut Vec<u8> = buffer;

    let result_len = match data_size.checked_add(start_len) {
        Some(new) if new < isize::MAX as usize => new,
        _ => return Poll::Ready(Err(overflow())),
    };

    let old_length = buf.len();
    buf.reserve(result_len.saturating_sub(old_length));

    // SAFETY: `result_len` was checked to be below `isize::MAX`, the destination range was
    // just reserved, and `reader` points at `data_size` initialised bytes that outlive this
    // future. Both ranges are disjoint (`buf` is a distinct `&mut Vec<u8>`).
    unsafe {
        let src = reader.as_ptr();
        let dst = buf.as_mut_ptr().add(start_len);
        core::ptr::copy_nonoverlapping(src, dst, data_size);
        buf.set_len(result_len);
    }

    Poll::Ready(Ok(data_size))
}

// -----------------------------------------------------------------------------
// WriteAllFuture

/// A future that writes a whole slice, looping until every byte is accepted.
///
/// Create one with [`WriteAllFuture::async_write`] (any [`AsyncWrite`]) or
/// [`WriteAllFuture::vec_write`] (a `Vec<u8>`). It never flushes: callers that need the bytes
/// to reach the storage call `AsyncWriteExt::flush` afterwards, as [`AssetWriter::write_bytes`]
/// does.
///
/// [`AssetWriter::write_bytes`]: crate::io::AssetWriter::write_bytes
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct WriteAllFuture<'a> {
    func: fn(Pin<&mut Self>, &mut Context<'_>) -> Poll<std::io::Result<()>>,
    writer: NonNull<u8>,
    buffer: &'a [u8],
}

impl Unpin for WriteAllFuture<'_> {}
// SAFETY: the future only holds a borrow of the destination plus a `&[u8]`; both are `Send` /
// `Sync` for the constructors' bounds (`W: Send + Sync`), and the raw pointer never escapes.
unsafe impl Send for WriteAllFuture<'_> {}
unsafe impl Sync for WriteAllFuture<'_> {}

impl Future for WriteAllFuture<'_> {
    type Output = std::io::Result<()>;

    #[inline(always)]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        (self.func)(self, cx)
    }
}

impl WriteAllFuture<'_> {
    /// Writes all of `input` into `writer`.
    ///
    /// `poll_write` is called until the input is drained. A `poll_write` that reports `0`
    /// bytes is an error ([`WriteZero`]), and an implementation that reports more bytes
    /// than it was handed is rejected as [`InvalidData`] instead of panicking.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use futures_lite::future::block_on;
    /// use zlim_asset::io::future::WriteAllFuture;
    ///
    /// let mut output: Vec<u8> = Vec::new();
    /// block_on(WriteAllFuture::async_write(&mut output, b"asset")).unwrap();
    ///
    /// assert_eq!(output, b"asset");
    /// ```
    ///
    /// [`WriteZero`]: std::io::ErrorKind::WriteZero
    /// [`InvalidData`]: std::io::ErrorKind::InvalidData
    #[inline(always)]
    pub fn async_write<'a, W: AsyncWrite + Send + Sync + Unpin>(
        writer: &'a mut W,
        input: &'a [u8],
    ) -> WriteAllFuture<'a> {
        WriteAllFuture {
            func: async_write_internal::<W>,
            writer: NonNull::from_mut(writer).cast(),
            buffer: input,
        }
    }

    /// Appends `input` to a `Vec<u8>`, completing on the first poll.
    ///
    /// This is the fast path behind `DataWriter`, the in-memory writer in
    /// [`crate::io::memory`] that savers and tests write through.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use futures_lite::future::block_on;
    /// use zlim_asset::io::future::WriteAllFuture;
    ///
    /// let mut output: Vec<u8> = b"prefix: ".to_vec();
    /// block_on(WriteAllFuture::vec_write(&mut output, b"data")).unwrap();
    ///
    /// assert_eq!(output, b"prefix: data");
    /// ```
    #[inline(always)]
    pub fn vec_write<'a>(writer: &'a mut Vec<u8>, input: &'a [u8]) -> WriteAllFuture<'a> {
        WriteAllFuture {
            func: vec_write_internal,
            writer: NonNull::from_mut(writer).cast(),
            buffer: input,
        }
    }
}

/// Loop `poll_write` until the whole buffer is consumed.
fn async_write_internal<W: AsyncWrite + Unpin>(
    this: Pin<&mut WriteAllFuture>,
    cx: &mut Context<'_>,
) -> Poll<std::io::Result<()>> {
    let WriteAllFuture { writer, buffer, .. } = this.get_mut();

    while !buffer.is_empty() {
        // SAFETY: the future was constructed from a `&mut W` that outlives it, and `func`
        // is only ever `async_write_internal::<W>` for that `W`.
        let writer: &mut W = unsafe { writer.cast::<W>().as_mut() };
        let writer: Pin<&mut W> = Pin::new(writer);

        let n = ready!(writer.poll_write(cx, buffer))?;

        // `core::mem::take` detaches the buffer from `*buffer`, so that the `rest` handed
        // back by `split_at_checked` does not borrow it while it is reassigned below. A
        // too-large `n` — a broken `AsyncWrite` implementation — leaves the buffer empty
        // and is reported as `InvalidData`.
        let taken = core::mem::take(buffer);
        let Some((_, rest)) = taken.split_at_checked(n) else {
            ::core::hint::cold_path();
            return Poll::Ready(Err(std::io::ErrorKind::InvalidData.into()));
        };

        *buffer = rest;

        if n == 0 {
            ::core::hint::cold_path();
            return Poll::Ready(Err(std::io::ErrorKind::WriteZero.into()));
        }
    }

    Poll::Ready(Ok(()))
}

/// Append directly into a `Vec<u8>`, completing immediately.
fn vec_write_internal(
    this: Pin<&mut WriteAllFuture>,
    _cx: &mut Context<'_>,
) -> Poll<std::io::Result<()>> {
    // Cold overflow path: keeps the hot path branch-free.
    #[cold]
    #[inline(never)]
    fn overflow() -> std::io::Error {
        std::io::ErrorKind::FileTooLarge.into()
    }

    let WriteAllFuture { writer, buffer, .. } = this.get_mut();

    if !buffer.is_empty() {
        // SAFETY: the future was constructed from a `&mut Vec<u8>` that outlives it.
        let writer: &mut Vec<u8> = unsafe { writer.cast::<Vec<u8>>().as_mut() };

        let start_len = writer.len();
        let data_size = buffer.len();

        let result_len = match data_size.checked_add(start_len) {
            Some(new) if new < isize::MAX as usize => new,
            _ => return Poll::Ready(Err(overflow())),
        };

        writer.reserve(data_size);

        // SAFETY: `result_len` is below `isize::MAX`, `reserve` guarantees the capacity, and
        // the source range is a distinct borrowed slice of `data_size` bytes.
        unsafe {
            let taken = core::mem::take(buffer);
            let src = taken.as_ptr();
            let dst = writer.as_mut_ptr().add(start_len);
            core::ptr::copy_nonoverlapping(src, dst, data_size);
            writer.set_len(result_len);
        }
    }

    Poll::Ready(Ok(()))
}
