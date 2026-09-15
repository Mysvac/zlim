//! The [`ZlimError`] type, [`Severity`], and result-conversion traits.

use core::error::Error;
use core::fmt::{Debug, Display};
use core::marker::PhantomData;
use core::ptr::NonNull;
use std::backtrace::Backtrace;

use zlim_utils::debug::DebugLocation;

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

// -----------------------------------------------------------------------------
// Severity

/// Indicates how severe a [`ZlimError`] is.
#[derive(Clone, Copy, Hash)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Severity {
    /// The error can be safely ignored and completely discarded.
    Ignore = 0,
    /// The error can be safely ignored but may need to be surfaced during debugging.
    Debug = 1,
    /// Nothing has gone wrong, but the error should be reported.
    Info = 2,
    /// Something unexpected but recoverable happened.
    Warning = 3,
    /// A real error occurred, but the program may continue.
    Error = 4,
    /// A fatal error; the program cannot continue.
    Panic = 5,
}

impl Display for Severity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Severity::Ignore => f.write_str("ignore"),
            Severity::Debug => f.write_str("debug"),
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

// -----------------------------------------------------------------------------
// Internal pointer-tagging helpers
//
// `ZlimError` packs a `Severity` (0–5) into the low 3 bits of an aligned
// heap pointer.  `InnerError` is `#[repr(align(8))]`, so any pointer to it has
// its low 3 bits zero.  The level is added as a byte offset before the pointer
// is stored; reading it back masks off the tag bits.
//
// Layout:
//   raw ptr:  [  ... upper bits ... | 0 0 0 ]  (aligned to ≥ 8)
//   stored:   raw ptr | level               (level ∈ {0,1,2,3,4,5})

/// Type-erased boxed error stored on the heap.
type BoxedError = Box<dyn Error + Send + Sync + 'static>;

/// The heap-allocated backing storage for a [`ZlimError`].
///
/// `#[repr(align(8))]` guarantees the low 3 bits of any pointer to this
/// struct are zero — the invariant that makes pointer-tagging safe.
#[repr(align(8))]
struct InnerError {
    content: BoxedError,
    location: DebugLocation,
    #[cfg(feature = "backtrace")]
    backtrace: Backtrace,
}

/// Compile-time guard: alignment must be at least 8.
const _: () = const {
    assert!(align_of::<InnerError>() >= 8);
};

/// Alignment of [`InnerError`] (≥ 8).
const ALIGN: usize = align_of::<InnerError>();
/// Bitmask for the tag bits (i.e. `ALIGN - 1`).
const MASKS: usize = ALIGN - 1;
/// Bitmask for the pointer bits (i.e. `!MASKS`).
const UPPER: usize = !MASKS;

// SAFETY: `InnerError` is `Box<dyn Error + Send + Sync>`, and `ZlimError`
// only owns a pointer to it plus tag bits — it never exposes mutable aliases.
unsafe impl Sync for ZlimError {}
// SAFETY: see the `Sync` impl above; ownership of the boxed error is unique.
unsafe impl Send for ZlimError {}

impl ZlimError {
    /// Extracts the raw [`InnerError`] pointer by masking off the tag bits.
    #[inline(always)]
    fn get_ptr(&self) -> *mut InnerError {
        (self.0.as_ptr() as usize & UPPER) as *mut InnerError
    }

    /// return a readonly reference of [`InnerError`].
    #[inline(always)]
    fn get_inner(&self) -> &InnerError {
        unsafe { &*self.get_ptr() }
    }
}

// -----------------------------------------------------------------------------
// Methods

impl ZlimError {
    /// Internal constructor that takes an already-boxed error.
    ///
    /// Encodes `severity` into the low bits of the heap pointer via pointer-tagging.
    #[inline(never)]
    fn new_boxed(severity: Severity, content: BoxedError, location: DebugLocation) -> Self {
        #[cfg(feature = "backtrace")]
        let blevel = backtrace_impls::get_level() as u8;

        #[cfg(feature = "backtrace")]
        let backtrace = if blevel <= severity as u8 {
            Backtrace::capture()
        } else {
            Backtrace::disabled()
        };

        #[cfg(not(feature = "backtrace"))]
        let boxed = Box::new(InnerError { content, location });

        #[cfg(feature = "backtrace")]
        let boxed = Box::new(InnerError {
            content,
            location,
            backtrace,
        });

        debug_assert!(
            (Box::as_ptr(&boxed) as usize & MASKS) == 0,
            "InnerError should be align of `8`"
        );

        let ptr: *mut InnerError = Box::leak(boxed);

        unsafe {
            let p: *mut () = (ptr as *mut ()).byte_add(severity as usize);
            Self(NonNull::new_unchecked(p), PhantomData)
        }
    }

    /// Creates a new `ZlimError` with the given [`Severity`] and error value.
    ///
    /// The error is automatically boxed via [`Into::into`], and the severity
    /// is encoded into the pointer's low bits via pointer-tagging.
    ///
    /// This function is cold and never inlined to keep the success path fast.
    ///
    /// For common severity levels, prefer the convenience constructors:
    /// [`ZlimError::info`], [`ZlimError::warning`], [`ZlimError::error`],
    /// [`ZlimError::panic`].
    #[cold]
    #[inline(never)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn new(severity: Severity, error: impl Into<BoxedError>) -> Self {
        Self::new_boxed(severity, error.into(), DebugLocation::caller())
    }

    /// Creates a new `ZlimError` with [`Severity::Ignore`].
    #[cold]
    #[inline(never)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn ignore(error: impl Into<BoxedError>) -> Self {
        Self::new_boxed(Severity::Ignore, error.into(), DebugLocation::caller())
    }

    /// Creates a new `ZlimError` with [`Severity::Debug`].
    #[cold]
    #[inline(never)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn debug(error: impl Into<BoxedError>) -> Self {
        Self::new_boxed(Severity::Debug, error.into(), DebugLocation::caller())
    }

    /// Creates a new `ZlimError` with [`Severity::Info`].
    #[cold]
    #[inline(never)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn info(error: impl Into<BoxedError>) -> Self {
        Self::new_boxed(Severity::Info, error.into(), DebugLocation::caller())
    }

    /// Creates a new `ZlimError` with [`Severity::Warning`].
    #[cold]
    #[inline(never)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn warning(error: impl Into<BoxedError>) -> Self {
        Self::new_boxed(Severity::Warning, error.into(), DebugLocation::caller())
    }

    /// Creates a new `ZlimError` with [`Severity::Error`].
    #[cold]
    #[inline(never)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn error(error: impl Into<BoxedError>) -> Self {
        Self::new_boxed(Severity::Error, error.into(), DebugLocation::caller())
    }

    /// Creates a new `ZlimError` with [`Severity::Panic`].
    #[cold]
    #[inline(never)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn panic(error: impl Into<BoxedError>) -> Self {
        Self::new_boxed(Severity::Panic, error.into(), DebugLocation::caller())
    }

    /// Returns the [`Severity`] stored in the pointer's low bits.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_core::error::{ZlimError, Severity};
    /// #
    /// let err = ZlimError::panic("something broke".to_string());
    /// assert_eq!(err.severity(), Severity::Panic);
    /// ```
    #[inline]
    pub fn severity(&self) -> Severity {
        match self.0.as_ptr() as usize & MASKS {
            0 => Severity::Ignore,
            1 => Severity::Debug,
            2 => Severity::Info,
            3 => Severity::Warning,
            4 => Severity::Error,
            _ => Severity::Panic,
        }
    }

    /// Overrides the severity level of this error.
    ///
    /// This only changes the metadata; the underlying error value remains
    /// unchanged.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_core::error::{ZlimError, Severity};
    /// #
    /// let err = ZlimError::panic("something broke".to_string()).with_severity(Severity::Warning);
    /// assert_eq!(err.severity(), Severity::Warning);
    /// ```
    #[cold]
    #[inline]
    pub fn with_severity(self, level: Severity) -> Self {
        unsafe {
            let p: *mut InnerError = self.get_ptr();
            let ptr: *mut () = (p as *mut ()).byte_add(level as usize);
            ::core::mem::forget(self);
            Self(NonNull::new_unchecked(ptr), PhantomData)
        }
    }

    ///  Merges the given severity into the current severity, taking the
    /// maximum of the two values.
    ///
    /// This only changes the metadata; the underlying error value remains
    /// unchanged.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_core::error::{ZlimError, Severity};
    /// #
    /// let e1 = ZlimError::info("something broke").merge_severity(Severity::Warning);
    /// assert_eq!(e1.severity(), Severity::Warning);
    ///
    /// let e2 = ZlimError::error("something broke").merge_severity(Severity::Warning);
    /// assert_eq!(e2.severity(), Severity::Error);
    /// ```
    #[cold]
    pub fn merge_severity(self, severity: Severity) -> Self {
        let old_severity = self.severity();
        self.with_severity(old_severity.max(severity))
    }

    /// Map severity through given function.
    ///
    /// This only changes the metadata; the underlying error value remains
    /// unchanged.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_core::error::{ZlimError, Severity};
    /// #
    /// let e = ZlimError::info("something broke")
    ///     .map_severity(|e| e.max(Severity::Warning));
    ///
    /// assert_eq!(e.severity(), Severity::Warning);
    /// ```
    #[cold]
    pub fn map_severity(self, f: impl FnOnce(Severity) -> Severity) -> Self {
        let old_severity = self.severity();
        self.with_severity(f(old_severity))
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

    /// Returns a reference to the underlying boxed error.
    #[inline]
    pub fn get(&self) -> &(dyn Error + Send + Sync + 'static) {
        self.get_inner().content.as_ref()
    }

    /// Returns the source code location where this [`ZlimError`] was triggered.
    #[inline]
    pub fn location(&self) -> DebugLocation {
        self.get_inner().location
    }

    /// Overrides the [`DebugLocation`] of this error.
    #[inline]
    pub fn with_location(self, location: DebugLocation) -> Self {
        let ptr: *mut InnerError = self.get_ptr();
        unsafe { (*ptr).location = location };
        self
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

// -----------------------------------------------------------------------------
// Backtrace

#[cfg(feature = "backtrace")]
mod backtrace_impls {
    use super::{BoxedError, InnerError, MASKS, Severity, ZlimError};
    use core::sync::atomic::{AtomicU8, Ordering};
    use core::{marker::PhantomData, ptr::NonNull};
    use std::backtrace::{Backtrace, BacktraceStatus};
    use zlim_utils::debug::DebugLocation;

    const FILTER_MESSAGE: &str = "NOTE: Some \"noisy\" backtrace lines have been filtered out. \
        Run with `ZLIM_BACKTRACE=full` for a verbose backtrace.";

    const NOISE_CONTENTS: &[&str] = &[
        "std::backtrace_rs::backtrace::",
        "std::backtrace::Backtrace::",
        "std::panicking::catch_unwind",
        "std::panic::catch_unwind",
        "std::thread::local::LocalKey",
        "core::panic::unwind_safe",
        "core::ops::function::",
        "zlim_core::job::into_job::",
        "zlim_core::system::function::",
        "zlim_core::schedule::executor::",
        "zlim_core::error::zlim_error::ZlimError::new_boxed",
        "zlim_task::platform::",
        "futures_lite::future::",
        "async_task::raw::",
        "async_task::runnable::Runnable",
    ];

    const STOP_SIGNALS: &[&str] = &["std::sys::backtrace::__rust_begin_short_backtrace"];

    static BACKTRACE_LEVEL: AtomicU8 = AtomicU8::new(Severity::Error as u8);

    #[inline]
    pub(super) fn get_level() -> Severity {
        const IGNORE: u8 = Severity::Ignore as u8;
        const DEBUG: u8 = Severity::Debug as u8;
        const INFO: u8 = Severity::Info as u8;
        const WARNING: u8 = Severity::Warning as u8;
        const ERROR: u8 = Severity::Error as u8;
        const PANIC: u8 = Severity::Panic as u8;

        match BACKTRACE_LEVEL.load(Ordering::Relaxed) {
            IGNORE => Severity::Ignore,
            DEBUG => Severity::Debug,
            INFO => Severity::Info,
            WARNING => Severity::Warning,
            ERROR => Severity::Error,
            PANIC => Severity::Panic,
            _ => Severity::Panic,
        }
    }

    #[inline]
    pub(super) fn set_level(severity: Severity) {
        BACKTRACE_LEVEL.store(severity as u8, Ordering::Relaxed);
    }

    impl ZlimError {
        pub(super) fn format_backtrace(
            &self,
            f: &mut core::fmt::Formatter<'_>,
        ) -> core::fmt::Result {
            let backtrace = unsafe { &(*self.get_ptr()).backtrace };

            if !matches!(backtrace.status(), BacktraceStatus::Captured) {
                return Ok(());
            }

            f.write_str("\n\nstack backtrace:\n")?;

            // `std::env::var` will panic in Wasm.
            #[cfg(not(target_family = "wasm"))]
            static FULL_BACKTRACE: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
                std::env::var("ZLIM_BACKTRACE").is_ok_and(|val| val == "full")
            });

            #[cfg(not(target_family = "wasm"))]
            if *FULL_BACKTRACE {
                return core::fmt::Display::fmt(backtrace, f);
            }

            let backtrace_str = backtrace.to_string();
            let mut skip_next_location_line = false;

            for line in backtrace_str.split('\n') {
                if skip_next_location_line {
                    if line.starts_with("             at") {
                        continue;
                    }
                    skip_next_location_line = false;
                }

                // Separate the beginning part, for example:
                // "  5: zlim_core::error::zlim_error::ZlimError::panic"
                //     ↑
                if let Some(index) = line.find(": ") {
                    let pattern = line[(index + 2)..].trim_start();

                    if NOISE_CONTENTS.iter().any(|&x| pattern.starts_with(x)) {
                        skip_next_location_line = true;
                        continue;
                    }

                    if STOP_SIGNALS.iter().any(|&x| pattern.starts_with(x)) {
                        break;
                    }
                }

                f.write_str(line)?;
                f.write_str("\n")?;
            }

            f.write_str(FILTER_MESSAGE)?;
            f.write_str("\n\n")
        }

        #[inline(never)]
        pub(super) fn new_with_backtrace_boxed(
            severity: Severity,
            content: BoxedError,
            backtrace: Backtrace,
            location: DebugLocation,
        ) -> Self {
            let ptr: *mut InnerError = Box::leak(Box::new(InnerError {
                content,
                location,
                backtrace,
            }));

            debug_assert!(
                (ptr as usize & MASKS) == 0,
                "InnerError should be align of `8`"
            );

            unsafe {
                let p: *mut () = (ptr as *mut ()).byte_add(severity as usize);
                Self(NonNull::new_unchecked(p), PhantomData)
            }
        }
    }
}

impl ZlimError {
    /// Constructs a new [`ZlimError`] with the given [`Severity`].
    ///
    /// Like [`ZlimError::new`], but if the `backtrace` cargo feature is enabled
    /// it will use the supplied backtrace instead of capturing a new one.
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn new_with_backtrace(
        severity: Severity,
        content: impl Into<BoxedError>,
        backtrace: Backtrace,
    ) -> Self {
        #[cfg(feature = "backtrace")]
        return Self::new_with_backtrace_boxed(
            severity,
            content.into(),
            backtrace,
            DebugLocation::caller(),
        );

        #[cfg(not(feature = "backtrace"))]
        let _ = backtrace;

        #[cfg(not(feature = "backtrace"))]
        return Self::new_boxed(severity, content.into(), DebugLocation::caller());
    }

    /// Returns `true` if a backtrace was captured for this error.
    ///
    /// When the `backtrace` feature is disabled, this always returns `false`.
    #[cfg_attr(not(feature = "backtrace"), inline(always))]
    pub fn backtrace_captured(&self) -> bool {
        #[cfg(not(feature = "backtrace"))]
        return false;

        #[cfg(feature = "backtrace")]
        let inner = unsafe { &*self.get_ptr() };

        #[cfg(feature = "backtrace")]
        return matches!(
            inner.backtrace.status(),
            std::backtrace::BacktraceStatus::Captured
        );
    }

    /// Takes the [`Backtrace`] out of this error, leaving a disabled one in its place.
    ///
    /// When the `backtrace` feature is disabled, this always returns [`Backtrace::disabled`]
    /// and does not modify the error.
    #[cfg_attr(not(feature = "backtrace"), inline(always))]
    pub fn take_backtrace(&mut self) -> Backtrace {
        #[cfg(not(feature = "backtrace"))]
        return Backtrace::disabled();

        #[cfg(feature = "backtrace")]
        let inner = unsafe { &mut *self.get_ptr() };

        #[cfg(feature = "backtrace")]
        return core::mem::replace(&mut inner.backtrace, Backtrace::disabled());
    }

    /// Sets the global backtrace capture level from the given [`Severity`].
    ///
    /// A backtrace is captured only when the severity of the error being
    /// constructed is **greater than or equal to** this level. For example,
    /// with a level of [`Severity::Error`], backtraces are captured for
    /// `Error` and `Panic`, but not for `Warning` and below.
    ///
    /// The default level is [`Severity::Error`].
    ///
    /// When the `backtrace` feature is disabled, this is a no-op.
    #[cfg_attr(not(feature = "backtrace"), inline(always))]
    pub fn set_backtrace_threshold(severity: Severity) {
        let _severity = severity;
        #[cfg(feature = "backtrace")]
        backtrace_impls::set_level(_severity);
    }
}

// -----------------------------------------------------------------------------
// Display

impl Display for ZlimError {
    /// Directly display internal error messages without severity.
    ///
    /// If you want the output of severity, use [`Debug::fmt`] instead.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        #[cfg(any(debug_assertions, feature = "debug"))]
        write!(f, "{}\n\tat {}", self.get(), self.location())?;

        #[cfg(not(any(debug_assertions, feature = "debug")))]
        write!(f, "{}", self.get())?;

        #[cfg(feature = "backtrace")]
        self.format_backtrace(f)?;

        Ok(())
    }
}

impl Debug for ZlimError {
    /// Display internal error messages and severity.
    ///
    /// If you don't want the output of severity, use [`Display::fmt`] instead.
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self, f)
    }
}
