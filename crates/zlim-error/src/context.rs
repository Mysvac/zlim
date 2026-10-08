//! Error context metadata describing where an error originated.

use core::fmt::Display;
use std::borrow::Cow;

/// Context for a [`ZlimError`] to aid in debugging.
///
/// [`ZlimError`]: crate::ZlimError
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub kind: &'static str,
    pub name: Cow<'static, str>,
}

impl Display for ErrorContext {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "{} `{}` failed", self.kind, self.name)
    }
}
