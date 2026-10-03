//! The [`ZlimError`] type, [`Severity`], and result-conversion traits.

use core::ops::ControlFlow;

use super::ZlimError;

// -----------------------------------------------------------------------------
// ZlimResult

/// A specialized [`Result`] type alias for [`ZlimError`].
///
/// This is the recommended return type for fallible functions throughout the
/// engine. Prefer this over `Result<T, ZlimError>` for brevity and consistency.
pub type ZlimResult<T> = Result<T, ZlimError>;

// -----------------------------------------------------------------------------
// IntoZlimResult

/// Conversion into a [`ZlimResult`].
///
/// This trait bridges the gap between various function return conventions and
/// the engine's unified error type.  It is also used as a bound to constrain
/// what types can serve as job function return values — implementing
/// `IntoZlimResult` signals that a type is an acceptable return type for a
/// job function.
///
/// # Implementations
///
/// Two blanket implementations cover the common patterns:
///
/// | Type                        | Behavior                                      |
/// |-----------------------------|-----------------------------------------------|
/// | `T`                         | Returns `Ok(T)` unchanged.                    |
/// | `Result<T, E>`              | Converts the error via `E: Into<ZlimError>`.  |
pub trait IntoZlimResult<T>: Sized {
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
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    fn into_zlim_result(self) -> Result<T, ZlimError> {
        match self {
            Ok(x) => Ok(x),
            Err(e) => Err(e.into()),
        }
    }
}

impl<C: IntoZlimResult<()>, B: Into<ZlimError>> IntoZlimResult<()> for ControlFlow<B, C> {
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    fn into_zlim_result(self) -> Result<(), ZlimError> {
        match self {
            ControlFlow::Continue(c) => c.into_zlim_result(),
            ControlFlow::Break(b) => Err(b.into()),
        }
    }
}
