//! provides [`Error`](derive_error) macros.

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

mod error;

/// Derive macro for `core::error::Error` with optional `Display` and `ZlimError` conversions.
///
/// # Generated impls
///
/// | Conditions                        | Impls emitted                                  |
/// |-----------------------------------|------------------------------------------------|
/// | Always                            | `core::error::Error`                           |
/// | `#[error("...")]`                 | `core::fmt::Display`                           |
/// | `#[error(transparent)]`           | `core::fmt::Display` (delegates to the single field) |
/// | `#[zlim_error(info/warning/…)]`   | `From<Self> for ZlimError` (implies `Into<ZlimError>`) |
///
/// # `#[error(…)]`
///
/// The content inside `#[error(…)]` works like [`format!`]:
/// field names are available directly, tuple fields need a leading underscore
/// (`_0`, `_1`, …), and arbitrary expressions are supported as extra arguments.
///
/// ```ignore
/// #[derive(Error)]
/// #[error("limit {limit} exceeded (max {})", i32::MAX)]
/// struct LimitError { limit: i32 }
///
/// #[derive(Error)]
/// #[error("limit {_0} exceeded (max {_1})")]
/// struct LimitError2(i32, i32);
/// ```
///
/// # `#[error(transparent)]`
///
/// Delegates `Display` to the wrapped error instead of formatting a template,
/// which is both cheaper (no formatting machinery, no intermediate allocation)
/// and lossless.
///
/// It is only valid where there is exactly one tuple field to delegate to:
///
/// - a single-field tuple struct, or
/// - a single-field tuple enum variant.
///
/// Using it anywhere else — a named/unit struct, a multi-field tuple, an enum
/// type itself, or a non-tuple variant — is a compile-time error.
///
/// ```ignore
/// #[derive(Error)]
/// #[error(transparent)]
/// struct IoError(std::io::Error);
///
/// #[derive(Error)]
/// enum AppError {
///     #[error(transparent)]
///     Io(std::io::Error),
///     #[error("bad config: {_0}")]
///     Config(String),
/// }
/// ```
///
/// # `#[zlim_error(severity)]`
///
/// Generate `Into<ZlimError>` implementation, with given `severity`.
///
/// ```ignore
/// #[derive(Error)]
/// #[error(transparent)]
/// #[zlim_error(warning)]
/// struct IoError(std::io::Error);
/// ```
///
/// Available severity: "ignore" | "debug" | "info" | "warning" | "error" | "panic".
///
/// # Enums — defaults and overrides
///
/// Place `#[error(…)]` / `#[zlim_error(severity)]` on the enum type to set
/// a default for all variants.  Individual variants can override the default
/// with their own attribute.
///
/// `#[error(…)]`: if no default is provided, **every** variant must carry
/// its own `#[error(…)]` annotation.
///
/// `#[zlim_error(severity)]`: valid severities are `ignore`, `debug`, `info`,
/// `warning`, `error`, and `panic`.  If no `#[zlim_error]` appears on either
/// the enum or any variant, no `From` impl is generated (and no error is
/// raised).  If only some variants carry it, an enum-level default is
/// required and used as the fallback.
///
/// # Examples
///
/// ```ignore
/// use zlim_error::derive::Error;
///
/// #[derive(Error)]
/// #[error("something went wrong: {msg}")]
/// #[zlim_error(warning)]
/// struct MyError { msg: String }
/// ```
///
/// ```ignore
/// use zlim_error::derive::Error;
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
