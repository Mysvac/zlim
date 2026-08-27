//! Error handler types and the default severity dispatch.

use core::cell::Cell;

use super::{ErrorContext, Severity, ZlimError};

// -----------------------------------------------------------------------------
// ErrorHandler

/// Function signature for zlim error handlers.
///
/// Receives the captured error and its execution context.
///
/// This is used by schedule executors and command application
/// paths when fallible work returns a [`ZlimError`].
pub type ErrorHandler = fn(e: ZlimError, ctx: ErrorContext);

// -----------------------------------------------------------------------------
// default_error_handler

/// Error handler that defers to an error's [`Severity`].
///
/// Dispatch table:
/// - [`Severity::Ignore`] => [`ignore()`]
/// - [`Severity::Debug`] => [`debug()`]
/// - [`Severity::Info`] => [`info()`]
/// - [`Severity::Warning`] => [`warn()`]
/// - [`Severity::Error`] => [`error()`]
/// - [`Severity::Panic`] => [`panic()`]
#[cold]
#[track_caller]
#[inline(never)]
pub fn default_error_handler(e: ZlimError, ctx: ErrorContext) {
    match e.severity() {
        Severity::Ignore => ignore(e, ctx),
        Severity::Debug => debug(e, ctx),
        Severity::Info => info(e, ctx),
        Severity::Warning => warn(e, ctx),
        Severity::Error => error(e, ctx),
        Severity::Panic => panic(e, ctx),
    }
}

std::thread_local! {
    /// When deliberately throwing a panic in your [`ErrorHandler`],
    /// set this to true to indicate to the executor that the panic
    /// should not be turned back into a [`ZlimError`].
    ///
    /// For example, we will do this for `system` and `command` execution:
    ///
    /// ```rust, ignore
    /// fn catch_and_resume(func: F, /* .. */) {
    ///     // Reset this flag before function execution
    ///     PANIC_ORIGINATES_FROM_ERROR_HANDLER.set(false);
    ///     // Run the given function and catch any panic
    ///     if let Err(e) = catch_unwind(AssertUnwindSafe(func)) {
    ///         if PANIC_ORIGINATES_FROM_ERROR_HANDLER.get() {
    ///             // If the panic was thrown by ErrorHandler,
    ///             // resume it directly.
    ///             resume_unwind(e);
    ///         }
    ///         // Otherwise, convert it to ZlimError
    ///         let e = ZlimError::panic( .. );
    ///         let ctx = ErrorContext { .. };
    ///         error_handler(e, ctx);
    ///     }
    /// }
    /// ```
    pub static PANIC_ORIGINATES_FROM_ERROR_HANDLER: Cell<bool>  = const { Cell::new(false) };
}

// -----------------------------------------------------------------------------
// helper

macro_rules! inner {
    ($call:path, $e:ident, $c:ident) => {
        $call!(
            "Encountered an error in {} `{}`: {}",
            $c.kind(),
            $c.name(),
            $e
        );
    };
}

/// Error handler that panics with the formatted error message.
#[inline]
#[track_caller]
pub fn panic(error: ZlimError, ctx: ErrorContext) {
    PANIC_ORIGINATES_FROM_ERROR_HANDLER.set(true);
    inner!(panic, error, ctx);
}

/// Error handler that logs the error at the `error` level.
#[inline]
#[track_caller]
pub fn error(error: ZlimError, ctx: ErrorContext) {
    inner!(zlim_log::error, error, ctx);
}

/// Error handler that logs the error at the `warn` level.
#[inline]
#[track_caller]
pub fn warn(error: ZlimError, ctx: ErrorContext) {
    inner!(zlim_log::warn, error, ctx);
}

/// Error handler that logs the error at the `info` level.
#[inline]
#[track_caller]
pub fn info(error: ZlimError, ctx: ErrorContext) {
    inner!(zlim_log::info, error, ctx);
}

/// Error handler that logs the error at the `debug` level.
#[inline]
#[track_caller]
pub fn debug(error: ZlimError, ctx: ErrorContext) {
    inner!(zlim_log::debug, error, ctx);
}

/// Error handler that logs the error at the `trace` level.
#[inline]
#[track_caller]
pub fn trace(error: ZlimError, ctx: ErrorContext) {
    inner!(zlim_log::trace, error, ctx);
}

/// Error handler that ignores the error.
#[inline(always)]
pub fn ignore(_: ZlimError, _: ErrorContext) {}
