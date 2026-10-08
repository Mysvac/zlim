//! zlim result and conversions

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
/// This trait bridges the gap between the return conventions a job, system, or
/// command may use and the engine's unified error type. It is also used as a
/// bound to constrain what a job or command function may return: implementing
/// `IntoZlimResult<bool>` is what marks a return type as acceptable for a job,
/// and [`IntoZlimResult<()>`] for a command.
///
/// The output type says what the caller wants to know. A job asks
/// `IntoZlimResult<bool>`, so the answer is the truth value it gates on; a
/// command asks `IntoZlimResult<()>`, so the answer is only success or failure.
///
/// # `IntoZlimResult<bool>`
///
/// | Type            | Behavior                                            |
/// |-----------------|-----------------------------------------------------|
/// | `()`            | `Ok(true)` — running without failing counts as pass. |
/// | `bool`          | `Ok(self)` — the value is the answer.                |
/// | `Result<(), E>` | `Ok(true)`, or the converted error.                  |
/// | `Result<T, E>`  | `Ok(value)`, or the converted error.                 |
///
/// # `IntoZlimResult<()>`
///
/// | Type                | Behavior                                       |
/// |---------------------|------------------------------------------------|
/// | `()`                | `Ok(())`.                                      |
/// | `Result<T, E>`      | `Ok(value)`, or the error via `E: Into<ZlimError>`. |
/// | `ControlFlow<B, C>` | `Continue` recurses, `Break` becomes an error. |
///
/// Note that a `false` is **not** an error: it is `Ok(false)`, a successful
/// conversion whose *value* says "skip". Only a genuine failure produces
/// `Err`, and that is what the caller routes to its error handler.
pub trait IntoZlimResult<T>: Sized {
    /// Converts `self` into a [`ZlimResult`].
    fn into_zlim_result(self) -> ZlimResult<T>;
}

impl IntoZlimResult<()> for () {
    #[inline(always)]
    fn into_zlim_result(self) -> ZlimResult<()> {
        Ok(())
    }
}

impl IntoZlimResult<bool> for () {
    #[inline(always)]
    fn into_zlim_result(self) -> ZlimResult<bool> {
        Ok(true)
    }
}

impl IntoZlimResult<bool> for bool {
    #[inline(always)]
    fn into_zlim_result(self) -> ZlimResult<bool> {
        Ok(self)
    }
}

impl<E: Into<ZlimError>> IntoZlimResult<bool> for Result<(), E> {
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    fn into_zlim_result(self) -> ZlimResult<bool> {
        match self {
            Ok(()) => Ok(true),
            Err(e) => Err(e.into()),
        }
    }
}

impl<T, E: Into<ZlimError>> IntoZlimResult<T> for Result<T, E> {
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    fn into_zlim_result(self) -> ZlimResult<T> {
        match self {
            Ok(t) => Ok(t),
            Err(e) => Err(e.into()),
        }
    }
}
