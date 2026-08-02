#![allow(linker_messages, reason = "It's noisy and interferes with CI output")]

extern crate proc_macro;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

// ----------------------------------------------------------------------------
// Modules

mod error;
mod path;

// ----------------------------------------------------------------------------
// Derive macros

/// Derive macro for `core::error::Error` with optional `Display` and
/// `ZlimError` conversions.
///
/// # Generated impls
///
/// | Conditions                        | Impls emitted                                  |
/// |-----------------------------------|------------------------------------------------|
/// | Always                            | `core::error::Error`                           |
/// | `#[error("...")]`                 | `core::fmt::Display`                           |
/// | `#[zlim_error(info/warning/…)]`   | `From<Self> for ZlimError` (implies `Into<ZlimError>`) |
///
/// # `#[error(…)]`
///
/// The content inside `#[error(…)]` works like [`core::format!`]:
/// field names are available directly, tuple fields need a leading
/// underscore (`_0`, `_1`, …), and arbitrary expressions are
/// supported as extra arguments.
///
/// ```ignore
/// #[derive(Error)]
/// #[error("limit {limit} exceeded (max {})", i32::MAX)]
/// struct LimitError { limit: i32 }
/// ```
///
/// # Enums — defaults and overrides
///
/// Place `#[error(…)]` / `#[zlim_error(severity)]` on the enum type to set
/// a default for all variants.  Individual variants can override the default
/// with their own attribute.  If no default is provided, **every** variant
/// must carry its own annotation.
///
/// # Examples
///
/// ```ignore
/// use zlim_core_derive::Error;
///
/// #[derive(Error)]
/// #[error("something went wrong: {msg}")]
/// #[zlim_error(warning)]
/// struct MyError { msg: String }
/// ```
///
/// ```ignore
/// use zlim_core_derive::Error;
///
/// #[derive(Error)]
/// #[error("a database error occurred")]
/// #[zlim_error(error)]
/// enum DbError {
///     #[error("connection refused")]
///     ConnectionRefused,
///     #[error("query timed out after {_0} ms")]
///     #[zlim_error(warning)]
///     Timeout(u64),
///     NotFound,
/// }
/// ```
#[proc_macro_derive(Error, attributes(error, zlim_error))]
pub fn derive_error(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    error::expand(&input).into()
}
