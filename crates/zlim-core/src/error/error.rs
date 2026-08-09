#![expect(clippy::module_inception, reason = "better structure")]

use core::error::Error;
use core::fmt::{Debug, Display};
use core::marker::PhantomData;
use core::ptr::NonNull;

// -----------------------------------------------------------------------------
// ZlimError

/// An error type that combines an underlying error with a severity level.
///
/// `ZlimError` wraps any `Error + Send + Sync + 'static` type and attaches a
/// [`Severity`], allowing error handling systems to categorize and respond to
/// errors appropriately.
///
/// # Examples
///
/// ```
/// use zlim_core::error::ZlimError;
///
/// fn validate_value(val: i64) -> Result<(), ZlimError> {
///     if val < 0 {
///         let msg = format!("Value cannot be negative: {val}");
///         return Err(ZlimError::info(msg));
///     }
///     Ok(())
/// }
/// ```
#[repr(transparent)]
pub struct ZlimError(NonNull<()>, PhantomData<Box<InnerError>>);

/// A specialized [`Result`] type alias for [`ZlimError`].
///
/// This is the recommended return type for fallible functions throughout the
/// engine. Prefer this over `Result<T, ZlimError>` for brevity and consistency.
pub type ZlimResult<T> = Result<T, ZlimError>;

// -----------------------------------------------------------------------------
// Severity

/// Indicates how severe a [`ZlimError`] is.
#[derive(Clone, Copy, Hash)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Nothing has gone wrong, but the error should be reported.
    Info = 0,
    /// Something unexpected but recoverable happened.
    Warning = 1,
    /// A real error occurred, but the program may continue.
    Error = 2,
    /// A fatal error; the program cannot continue.
    Panic = 3,
}

// -----------------------------------------------------------------------------
// Internal pointer-tagging helpers
//
// `ZlimError` packs a `Severity` (0–3) into the low 2 bits of an aligned
// heap pointer.  `InnerError` is `#[repr(align(4))]`, so any pointer to it has
// its low 2 bits zero.  The level is added as a byte offset before the pointer
// is stored; reading it back masks off the tag bits.
//
// Layout:
//   raw ptr:  [  ... upper bits ... | 0 0 ]  (aligned to ≥ 4)
//   stored:   raw ptr | level               (level ∈ {0,1,2,3})

/// Type-erased boxed error stored on the heap.
type BoxedError = Box<dyn Error + Send + Sync + 'static>;

/// The heap-allocated backing storage for a [`ZlimError`].
///
/// `#[repr(align(4))]` guarantees the low 2 bits of any pointer to this
/// struct are zero — the invariant that makes pointer-tagging safe.
#[repr(align(4))]
struct InnerError {
    content: BoxedError,
}

/// Compile-time guard: alignment must be at least 4.
const _: () = const {
    assert!(align_of::<InnerError>() >= 4);
};

/// Alignment of [`InnerError`] (≥ 4).
const ALIGN: usize = align_of::<InnerError>();
/// Bitmask for the tag bits (i.e. `ALIGN - 1`).
const MASKS: usize = ALIGN - 1;
/// Bitmask for the pointer bits (i.e. `!MASKS`).
const UPPER: usize = !MASKS;

impl ZlimError {
    /// Extracts the raw [`InnerError`] pointer by masking off the tag bits.
    #[inline(always)]
    fn get_ptr(&self) -> *mut InnerError {
        (self.0.as_ptr() as usize & UPPER) as *mut InnerError
    }
}

// -----------------------------------------------------------------------------
// Implementation

impl ZlimError {
    /// Internal constructor that takes an already-boxed error.
    ///
    /// Encodes `level` into the low bits of the heap pointer via pointer-tagging.
    #[inline(never)]
    fn new_boxed(level: Severity, error: BoxedError) -> Self {
        let ptr: *mut InnerError = Box::leak(Box::new(InnerError { content: error }));
        debug_assert!(
            (ptr as usize & MASKS) == 0,
            "InnerError should be align of `8`"
        );
        unsafe {
            let p: *mut () = (ptr as *mut ()).byte_add(level as usize);
            Self(NonNull::new_unchecked(p), PhantomData)
        }
    }

    /// Creates a new `ZlimError` with the given [`Severity`] and error value.
    ///
    /// The error is automatically boxed via [`Into::into`], and the severity
    /// level is encoded into the pointer's low bits via pointer-tagging.
    ///
    /// This function is cold and never inlined to keep the success path fast.
    ///
    /// For common severity levels, prefer the convenience constructors:
    /// [`ZlimError::info`], [`ZlimError::warning`], [`ZlimError::error`],
    /// [`ZlimError::panic`].
    #[cold]
    #[inline(never)]
    pub fn new(level: Severity, error: impl Into<BoxedError>) -> Self {
        Self::new_boxed(level, error.into())
    }

    /// Creates a new `ZlimError` with [`Severity::Info`].
    #[cold]
    #[inline(never)]
    pub fn info(error: impl Into<BoxedError>) -> Self {
        Self::new_boxed(Severity::Info, error.into())
    }

    /// Creates a new `ZlimError` with [`Severity::Warning`].
    #[cold]
    #[inline(never)]
    pub fn warning(error: impl Into<BoxedError>) -> Self {
        Self::new_boxed(Severity::Warning, error.into())
    }

    /// Creates a new `ZlimError` with [`Severity::Error`].
    #[cold]
    #[inline(never)]
    pub fn error(error: impl Into<BoxedError>) -> Self {
        Self::new_boxed(Severity::Error, error.into())
    }

    /// Creates a new `ZlimError` with [`Severity::Panic`].
    #[cold]
    #[inline(never)]
    pub fn panic(error: impl Into<BoxedError>) -> Self {
        Self::new_boxed(Severity::Panic, error.into())
    }

    /// Overrides the severity level of this error.
    ///
    /// This only changes the metadata; the underlying error value remains
    /// unchanged.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_core::error::{ZlimError, Severity};
    ///
    /// let err = ZlimError::panic("something broke").with_severity(Severity::Warning);
    /// assert_eq!(err.severity(), Severity::Warning);
    /// ```
    #[inline]
    pub fn with_severity(self, level: Severity) -> Self {
        unsafe {
            let p: *mut InnerError = self.get_ptr();
            let ptr: *mut () = (p as *mut ()).byte_add(level as usize);
            ::core::mem::forget(self);
            Self(NonNull::new_unchecked(ptr), PhantomData)
        }
    }

    /// Returns the [`Severity`] stored in the pointer's low bits.
    #[inline]
    pub fn severity(&self) -> Severity {
        match self.0.as_ptr() as usize & MASKS {
            0 => Severity::Info,
            1 => Severity::Warning,
            2 => Severity::Error,
            _ => Severity::Panic,
        }
    }

    /// Returns a reference to the underlying boxed error.
    #[inline]
    pub fn get(&self) -> &(dyn Error + Send + Sync + 'static) {
        unsafe { (*self.get_ptr()).content.as_ref() }
    }

    /// Consumes the `ZlimError` and returns the inner boxed error.
    ///
    /// The severity metadata is discarded. This is useful when you need to
    /// hand off the error to another system that expects a `Box<dyn Error>`.
    #[inline]
    pub fn take(self) -> BoxedError {
        let ptr: *mut InnerError = self.get_ptr();
        ::core::mem::forget(self);
        unsafe { Box::from_raw(ptr).content }
    }
}

impl Drop for ZlimError {
    fn drop(&mut self) {
        unsafe {
            let ptr: *mut InnerError = self.get_ptr();
            ::core::mem::drop(Box::from_raw(ptr));
        }
    }
}

impl Display for Severity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Severity::Info => f.write_str("info"),
            Severity::Warning => f.write_str("warning"),
            Severity::Error => f.write_str("error"),
            Severity::Panic => f.write_str("panic"),
        }
    }
}

impl Debug for Severity {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(&self, f)
    }
}

impl Display for ZlimError {
    /// Directly display internal error messages without severity.
    ///
    /// If you want the output of severity, use [`Debug::fmt`] instead.
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self.get(), f)
    }
}

impl Debug for ZlimError {
    /// Display internal error messages and severity.
    ///
    /// If you don't want the output of severity, use [`Display::fmt`] instead.
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ZlimError")
            .field("severity", &self.severity())
            .field("error", &self.get())
            .finish()
    }
}

// -----------------------------------------------------------------------------
// IntoZlimResult

/// Conversion into a [`ZlimResult`].
///
/// This trait bridges the gap between various function return conventions and
/// the engine's unified error type.  It is also used as a bound to constrain
/// what types can serve as script function return values — implementing
/// `IntoZlimResult` signals that a type is an acceptable return type for a
/// script function.
///
/// # Implementations
///
/// Two blanket implementations cover the common patterns:
///
/// | Type                        | Behavior                                      |
/// |-----------------------------|-----------------------------------------------|
/// | `T`                         | Returns `Ok(T)` unchanged.                    |
/// | `Result<T, E>`              | Converts the error via `E: Into<ZlimError>`.  |
pub trait IntoZlimResult<T> {
    /// Converts `self` into a [`ZlimResult`].
    fn into_zlim_result(self) -> Result<T, ZlimError>;
}

impl<T> IntoZlimResult<T> for T {
    #[inline(always)]
    fn into_zlim_result(self) -> Result<T, ZlimError> {
        Ok(self)
    }
}

impl<T, E: Into<ZlimError>> IntoZlimResult<T> for Result<T, E> {
    fn into_zlim_result(self) -> Result<T, ZlimError> {
        self.map_err(Into::into)
    }
}
