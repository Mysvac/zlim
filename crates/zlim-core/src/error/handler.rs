use super::{ErrorContext, Severity, ZlimError};

// ----------------------------------------------------------------------------
// ErrorHandler

/// Function signature for zlim error handlers.
///
/// Receives the captured error and its execution context.
///
/// This is used by schedule executors and command application
/// paths when fallible work returns a [`ZlimError`].
pub type ErrorHandler = fn(e: ZlimError, ctx: ErrorContext);

// ----------------------------------------------------------------------------
// default_error_handler

/// Error handler that defers to an error's [`Severity`].
///
/// Dispatch table:
/// - [`Severity::Info`] => [`info()`]
/// - [`Severity::Warning`] => [`warn()`]
/// - [`Severity::Error`] => [`error()`]
/// - [`Severity::Panic`] => [`panic()`]
#[cold]
#[track_caller]
#[inline(never)]
pub fn default_error_handler(e: ZlimError, ctx: ErrorContext) {
    match e.severity() {
        Severity::Info => info(e, ctx),
        Severity::Warning => warn(e, ctx),
        Severity::Error => error(e, ctx),
        Severity::Panic => panic(e, ctx),
    }
}

// ----------------------------------------------------------------------------
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
    inner!(panic, error, ctx);
}

/// Error handler that logs the error at the `error` level.
#[inline]
#[track_caller]
pub fn error(error: ZlimError, ctx: ErrorContext) {
    inner!(log::error, error, ctx);
}

/// Error handler that logs the error at the `warn` level.
#[inline]
#[track_caller]
pub fn warn(error: ZlimError, ctx: ErrorContext) {
    inner!(log::warn, error, ctx);
}

/// Error handler that logs the error at the `info` level.
#[inline]
#[track_caller]
pub fn info(error: ZlimError, ctx: ErrorContext) {
    inner!(log::info, error, ctx);
}

/// Error handler that logs the error at the `debug` level.
#[inline]
#[track_caller]
pub fn debug(error: ZlimError, ctx: ErrorContext) {
    inner!(log::debug, error, ctx);
}

/// Error handler that logs the error at the `trace` level.
#[inline]
#[track_caller]
pub fn trace(error: ZlimError, ctx: ErrorContext) {
    inner!(log::trace, error, ctx);
}

/// Error handler that ignores the error.
#[inline]
#[track_caller]
pub fn ignore(_: ZlimError, _: ErrorContext) {}
