use core::ffi::CStr;
use core::marker::PhantomData;

use crate::Client;

#[cfg(feature = "tracy")]
use tracy_client_sys as sys;

// -----------------------------------------------------------------------------
// FrameName

/// The name of a secondary or a non-continuous frame.
///
/// Normally constructed with the [`frame_name!`] macro, which is a compile-time
/// operation without memory allocation.
///
/// Use [`FrameName::new_leak`] only for names that are not known at compile
/// time. It leaks the string into the memory pool **without** deduplication, so
/// the name should be created once and reused.
///
/// If there is already a static [`CStr`], consider using [`FrameName::new`] to
/// a [`FrameName`] without any additional allocation.
///
/// # Without the profiler
///
/// When the `tracy` cargo feature is disabled, a name carries nothing: the type
/// is a zero-sized marker, and every value of it is equal to every other one, no
/// matter which name each of them was built from. An implementation that relies
/// on names being distinct therefore needs to be based on something else in that
/// case.
///
/// [`frame_name!`]: crate::frame_name
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FrameName {
    _seal: PhantomData<()>,
    #[cfg(feature = "tracy")]
    name: &'static str,
}

impl FrameName {
    /// Creates a `FrameName` from a static [`CStr`].
    ///
    /// The name is used as it is, so the caller keeps the string alive and nothing is allocated.
    ///
    /// # Panics
    ///
    /// Panics if the name is not valid UTF-8.
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn new(name: &'static CStr) -> Self {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = name;
            Self { _seal: PhantomData }
        }

        #[cfg(feature = "tracy")]
        {
            let name = name.to_str().expect("a frame name must be valid UTF-8");
            Self {
                _seal: PhantomData,
                name,
            }
        }
    }

    /// Constructs a `FrameName` from a runtime string.
    ///
    /// The name is copied into the global memory pool, so it stays valid for
    /// the rest of the program. Call this once per name and keep the result:
    /// calling it in a loop grows memory without bound.
    ///
    /// The [`frame_name!`](crate::frame_name) macro is preferable, because a
    /// literal name needs no allocation at all.
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    #[must_use]
    pub fn new_leak(name: &str) -> Self {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = name;
            Self { _seal: PhantomData }
        }

        #[cfg(feature = "tracy")]
        {
            // The profiler reads the name as a C string, so the terminator has to be part of the
            // copy whenever the caller did not provide one.
            let name = if name.bytes().last() == Some(b'\0') {
                zlim_utils::mem::Global::alloc_str(name)
            } else {
                let mut buf = String::with_capacity(name.len() + 1);
                buf.push_str(name);
                buf.push('\0');
                zlim_utils::mem::Global::alloc_str(&buf)
            };
            Self {
                _seal: PhantomData,
                name,
            }
        }
    }

    /// Constructs a `FrameName` from a null-terminated literal.
    ///
    /// Only [`crate::internal::create_frame_name`], which the [`frame_name!`](crate::frame_name)
    /// macro expands to, calls this function, and it checks that the literal is null-terminated.
    #[cfg(feature = "tracy")]
    #[inline(always)]
    pub(crate) const fn from_lit(name: &'static str) -> Self {
        Self {
            _seal: PhantomData,
            name,
        }
    }

    /// Creates the nameless frame name of a build without the profiler.
    #[doc(hidden)]
    #[inline(always)]
    #[cfg(not(feature = "tracy"))]
    pub const fn __no_tracy() -> Self {
        Self { _seal: PhantomData }
    }
}

// -----------------------------------------------------------------------------
// Frame

/// A non-continuous frame region.
///
/// Created with [`Client::non_continuous_frame`], and ended when the value is dropped.
#[must_use]
#[repr(transparent)]
pub struct Frame(FrameName);

#[cfg(feature = "tracy")]
impl Drop for Frame {
    fn drop(&mut self) {
        unsafe {
            // SAFETY: the name is null-terminated and outlives the call.
            let () = sys::___tracy_emit_frame_mark_end(self.0.name.as_ptr().cast());
        }
    }
}

impl Frame {
    /// Returns the name of the frame.
    #[inline]
    pub fn name(&self) -> FrameName {
        self.0
    }
}

// -----------------------------------------------------------------------------
// Client

/// Instrumentation for global frame indicators.
impl Client {
    /// Marks the end of a continuous frame.
    ///
    /// In a rendering application this belongs right after the buffer swap.
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn frame_mark() {
        #[cfg(feature = "tracy")]
        unsafe {
            // SAFETY: a null name asks the profiler for the primary frame mark.
            let () = sys::___tracy_emit_frame_mark(core::ptr::null());
        }
    }

    /// Marks the end of a named, continuously repeating frame.
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn secondary_frame_mark(name: FrameName) {
        #[cfg(not(feature = "tracy"))]
        let _ = name;

        #[cfg(feature = "tracy")]
        unsafe {
            // SAFETY: `name` is null-terminated and outlives the call.
            let () = sys::___tracy_emit_frame_mark(name.name.as_ptr().cast());
        }
    }

    /// Marks the beginning of a non-continuous frame.
    ///
    /// The frame ends when the returned [`Frame`] is dropped.
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn non_continuous_frame(name: FrameName) -> Frame {
        #[cfg(feature = "tracy")]
        unsafe {
            // SAFETY: `name` is null-terminated and outlives the call.
            let () = sys::___tracy_emit_frame_mark_start(name.name.as_ptr().cast());
        }

        Frame(name)
    }

    /// Emits an image of a frame.
    ///
    /// The image must be in RGBA format, with a width and a height divisible by four,
    /// and it should be smaller than 256 KB. `offset` is the number of frames in the
    /// past the image was captured in, so an offset of 1 associates the image with the
    /// frame that ended at the previous [`Client::frame_mark`]. `flip` mirrors the image
    /// vertically, which is what a texture captured by an API with a bottom-left origin needs.
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn frame_image(image: &[u8], width: u16, height: u16, offset: u8, flip: bool) {
        #[cfg(not(feature = "tracy"))]
        let _ = (image, width, height, offset, flip);

        #[cfg(feature = "tracy")]
        unsafe {
            // SAFETY: the profiler copies the image before returning.
            let () = sys::___tracy_emit_frame_image(
                image.as_ptr().cast(),
                width,
                height,
                offset,
                flip as i32,
            );
        }
    }
}

// -----------------------------------------------------------------------------
// frame_name

/// Constructs a [`FrameName`] from a literal name.
///
/// The macro can be used in a `const` context, and the resulting
/// name can be passed to [`Client::secondary_frame_mark`] and
/// [`Client::non_continuous_frame`].
#[macro_export]
#[cfg(feature = "tracy")]
macro_rules! frame_name {
    ($name: literal) => {{ $crate::internal::create_frame_name(concat!($name, "\0")) }};
}

/// Constructs a [`FrameName`] from a literal name.
///
/// The macro can be used in a `const` context, and the resulting
/// name can be passed to [`Client::secondary_frame_mark`] and
/// [`Client::non_continuous_frame`].
#[macro_export]
#[cfg(not(feature = "tracy"))]
macro_rules! frame_name {
    ($name: literal) => {{
        /* let _ = $name; */
        $crate::FrameName::__no_tracy()
    }};
}

// -----------------------------------------------------------------------------
// secondary_frame_mark

/// Marks the end of a named, continuously repeating frame.
///
/// Equivalent to calling [`Client::secondary_frame_mark`]
/// with the [`frame_name!`](crate::frame_name) of the name.
#[macro_export]
#[cfg(feature = "tracy")]
macro_rules! secondary_frame_mark {
    ($name: literal) => {{
        $crate::Client::secondary_frame_mark($crate::frame_name!($name));
    }};
}

/// Marks the end of a named, continuously repeating frame.
///
/// Equivalent to calling [`Client::secondary_frame_mark`]
/// with the [`frame_name!`](crate::frame_name) of the name.
#[macro_export]
#[cfg(not(feature = "tracy"))]
macro_rules! secondary_frame_mark {
    ($name: literal) => {{ /* let _ = $name; */ }};
}

// -----------------------------------------------------------------------------
// non_continuous_frame

/// Marks the beginning of a non-continuous frame.
///
/// Equivalent to calling [`Client::non_continuous_frame`] with the
/// [`frame_name!`](crate::frame_name) of the name. The frame ends
/// when the returned [`Frame`] is dropped.
#[macro_export]
#[cfg(feature = "tracy")]
macro_rules! non_continuous_frame {
    ($name: literal) => {{
        $crate::Client::non_continuous_frame($crate::frame_name!($name));
    }};
}

/// Marks the beginning of a non-continuous frame.
///
/// Equivalent to calling [`Client::non_continuous_frame`] with the
/// [`frame_name!`](crate::frame_name) of the name. The frame ends
/// when the returned [`Frame`] is dropped.
#[macro_export]
#[cfg(not(feature = "tracy"))]
macro_rules! non_continuous_frame {
    ($name: literal) => {{ /* let _ = $name; */ }};
}

// -----------------------------------------------------------------------------
