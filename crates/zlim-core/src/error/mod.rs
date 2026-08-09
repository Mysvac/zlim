//! Engine error types and the `#[derive(Error)]` macro.
//!
//! # Main items
//!
//! | Item | Description |
//! |------|-------------|
//! | [`ZlimError`] | Heap-allocated error with severity metadata. |
//! | [`ZlimResult<T>`] | `Result<T, ZlimError>` alias. |
//! | [`Severity`] | `Info` / `Warning` / `Error` / `Panic`. |
//! | [`IntoZlimResult`] | Convert a type into a [`ZlimResult<T>`]. |
//! | [`ErrorContext`] | Context metadata for where an error originated. |
//! | [`ErrorHandler`] | `fn(ZlimError, ErrorContext)` signature for error callbacks. |
//! | [`default_error_handler`] | Dispatches to the appropriate log level by [`Severity`]. |
//! | [`Error`](derive@Error) | Derive macro for `core::error::Error` + optional `Display` and `From<Self> for ZlimError`. |

// -----------------------------------------------------------------------------
// Modules

mod context;
mod error;

pub mod handler;

// -----------------------------------------------------------------------------
// Exports

pub use zlim_core_derive::Error;

pub use context::ErrorContext;
pub use error::{IntoZlimResult, Severity};
pub use error::{ZlimError, ZlimResult};
pub use handler::{ErrorHandler, default_error_handler};
