//! Engine error types and the `#[derive(Error)]` macro.
//!
//! # `ZlimError` — the core error type
//!
//! [`ZlimError`] wraps any `Error + Send + Sync + 'static` value in a
//! single heap allocation and attaches a [`Severity`].
//!
//! Construct one with the severity-specific helpers — [`ZlimError::ignore`],
//! [`ZlimError::debug`], [`ZlimError::info`], [`ZlimError::warning`],
//! [`ZlimError::error`], [`ZlimError::panic`] — or with [`ZlimError::new`]
//! plus an explicit [`Severity`]:
//!
//! ```rust
//! use zlim_core::error::{Severity, ZlimError};
//!
//! let err = ZlimError::warning("disk is nearly full".to_string());
//! assert_eq!(err.severity(), Severity::Warning);
//!
//! // Severity metadata can be adjusted without touching the payload.
//! let err = err.with_severity(Severity::Error);
//! assert_eq!(err.severity(), Severity::Error);
//!
//! // Unbox the payload when handing it to foreign code.
//! let boxed: Box<dyn std::error::Error + Send + Sync> = err.take();
//! assert_eq!(boxed.to_string(), "disk is nearly full");
//! ```
//!
//! With the `backtrace` feature, [`Severity::Warning`] and
//! [`Severity::Error`] capture a backtrace that is printed with the error.
//!
//! Backtrace will skip some noisy(useless) lines by default, set
//! `ZLIM_BACKTRACE=full` to disable filtering.
//!
//! # Severity levels
//!
//! [`Severity`] ranks failures from ignorable to fatal; the variants are
//! ordered so that `max`/`min` semantics work (`Panic > Error > … > Ignore`):
//!
//! | Level | Meaning |
//! |-------|---------|
//! | [`Severity::Ignore`] | Safe to discard entirely. |
//! | [`Severity::Debug`] | Harmless, but may help debugging. |
//! | [`Severity::Info`] | Nothing went wrong, still worth reporting. |
//! | [`Severity::Warning`] | Unexpected but recoverable. |
//! | [`Severity::Error`] | A real error; the program may continue. |
//! | [`Severity::Panic`] | Fatal; execution cannot continue. |
//!
//! [`ZlimError::merge_severity`] / [`ZlimError::map_severity`] raise or
//! transform the level while keeping the payload, e.g. to escalate an
//! inner error to the severity of the operation that failed.
//!
//! # Error handling
//!
//! Fallible functions return [`ZlimResult<T>`] and convert into it through
//! [`IntoZlimResult`], which is implemented for:
//!
//! - `T` — success, unchanged;
//! - `Result<T, E>` — the error is converted via `E: Into<ZlimError>`;
//! - `ControlFlow<B, C>` — `Continue` passes through, `Break` becomes an
//!   error.
//!
//! When a job, system, or command fails, the executor builds an [`ErrorContext`]
//! (`Job`, `System`, or `Command`, plus the tick/id of the failing construct) and
//! invokes the world's [`ErrorHandler`] through [`World::error_handler`].
//!
//! The default handler, [`default_error_handler`], dispatches by severity:
//! logs `debug`/`info`/`warn`/`error` at the matching level, **panics** for
//! [`Severity::Panic`], and drops [`Severity::Ignore`].
//!
//! Custom handlers (e.g. telemetry or crash reporting) can be set through
//! [`World::set_error_handler`]
//!
//! # The `#[derive(Error)]` macro
//!
//! Deriving [`Error`](derive@Error) on a struct or enum generates:
//!
//! - **Always** — an implementation of `core::error::Error` (which also
//!   requires the type to implement `Debug`, so derive `Debug` alongside).
//!
//! - **`#[error("...")]`** — an implementation of `Display`.  The string
//!   works like [`format!`]: named fields are in scope by name, tuple
//!   fields by `_0`, `_1`, …, and extra arguments are allowed.
//!
//! - **`#[zlim_error(severity)]`** — a `From<Self> for ZlimError` impl that
//!   boxes the error at the given [`Severity`] (this is what makes `?`
//!   and [`IntoZlimResult`] work with your error type).
//!
//! For enums, attributes placed on the enum act as defaults; individual
//! variants can override them:
//!
//! ```rust
//! use zlim_core::error::{Error, Severity, ZlimError, ZlimResult};
//!
//! #[derive(Debug, Error)]
//! #[error("validation failed")]
//! #[zlim_error(warning)]
//! enum ValidationError {
//!     #[error("age {_0} is negative")]
//!     NegativeAge(i32),
//!     #[error("limit {limit} exceeded")]
//!     #[zlim_error(error)] // override the default severity
//!     LimitExceeded { limit: i32 },
//! }
//!
//! fn validate(age: i32, limit: i32) -> ZlimResult<()> {
//!     if age < 0 {
//!         return Err(ValidationError::NegativeAge(age).into());
//!     }
//!     if limit > 100 {
//!         return Err(ValidationError::LimitExceeded { limit }.into());
//!     }
//!     Ok(())
//! }
//!
//! let _ = ZlimError::from(ValidationError::NegativeAge(-1)); // `From` was derived
//! ```
//!
//! [`World::error_handler`]: crate::world::World::error_handler
//! [`World::set_error_handler`]: crate::world::World::set_error_handler

// -----------------------------------------------------------------------------
// Modules

mod context;
mod payload;
mod result;
mod zlim_error;

pub mod handler;

// -----------------------------------------------------------------------------
// Exports

pub use context::ErrorContext;
pub use handler::{ErrorHandler, default_error_handler};
pub use payload::PanicPayload;
pub use result::{IntoZlimResult, ZlimResult};
pub use zlim_core_derive::Error;
pub use zlim_error::{Severity, ZlimError};

// -----------------------------------------------------------------------------
