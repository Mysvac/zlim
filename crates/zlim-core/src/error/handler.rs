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
#[track_caller] // useless, function pointer cannot track_caller
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

thread_local! {
    /// A thread-local flag indicating that the current panic was raised by an
    /// `ErrorHandler` **and** the stack has already been captured into the
    /// [`ZlimError`] payload.
    ///
    /// When the panic hook observes this flag it can write the cleaner
    /// [`ZlimError`] content straight to the error stream and skip the default
    /// hook output entirely — the error has already been fully reported with
    /// its backtrace, so re-printing a generic panic message would only add
    /// noise.
    ///
    /// # When to set this flag
    ///
    /// Set this flag to `true` **immediately before** panicking from an
    /// `ErrorHandler`, and only when the error's backtrace was captured into
    /// the [`ZlimError`] itself (see [`panic()`]).
    ///
    /// # Reset behavior
    ///
    /// The flag is **not** reset automatically after a panic — the panic hook
    /// is responsible for clearing it (e.g., with `replace(false)`) so the
    /// state does not leak into subsequent panics.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // In a custom panic hook:
    /// std::panic::set_hook(Box::new(|info| {
    ///     let captured = PANIC_BACKTRACE_CAPTURED.replace(false);
    ///
    ///     if captured {
    ///         // The ErrorHandler already reported the full error (message +
    ///         // captured stack). Print only the clean message and skip the
    ///         // default hook output.
    ///         if let Some(msg) = info.payload_as_str() {
    ///             eprintln!("{msg}");
    ///         }
    ///     } else {
    ///         // Unexpected panic: keep the default hook's full output.
    ///         default_hook(info);
    ///     }
    /// }));
    /// ```
    pub static PANIC_BACKTRACE_CAPTURED: Cell<bool> = const { Cell::new(false) };
}

// -----------------------------------------------------------------------------
// helper

macro_rules! inner {
    ($call:path, $e:ident, $c:ident) => {
        $call!(
            "Encountered an error in {} `{}`:\n\t{}",
            $c.kind(),
            $c.name(),
            $e,
        );
    };
}

/// Error handler that panics with the formatted error message.
#[inline]
#[track_caller]
pub fn panic(error: ZlimError, ctx: ErrorContext) {
    if error.backtrace_captured() {
        PANIC_BACKTRACE_CAPTURED.set(true);
    }
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
