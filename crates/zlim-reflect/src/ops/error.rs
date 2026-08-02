use core::error::Error;
use core::fmt::{self, Display};

use crate::info::{ReflectKind, VariantKind};

// ----------------------------------------------------------------------------
// CloneError

/// Error returned when [`reflect_clone`] fails.
///
/// Identifies the type, and optionally the field or variant, for which
/// cloning is not supported.
///
/// [`reflect_clone`]: crate::Reflect::reflect_clone
#[derive(Debug)]
pub enum CloneError {
    /// Cloning is not supported for this type.
    Unsupport { type_path: &'static str },

    /// Cloning is not supported for a specific field of this type.
    FieldUnsupport {
        type_path: &'static str,
        field_name: &'static str,
    },

    /// Cloning is not supported for a specific variant of this enum.
    VariantUnsupport {
        type_path: &'static str,
        variant_name: &'static str,
    },
}

impl Error for CloneError {}

impl Display for CloneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CloneError::Unsupport { type_path } => {
                write!(f, "`reflect_clone` not supported for `{type_path}`")
            }
            CloneError::FieldUnsupport {
                type_path,
                field_name,
            } => {
                write!(
                    f,
                    "`reflect_clone` not supported for `{type_path}`'s field `{field_name}`"
                )
            }
            CloneError::VariantUnsupport {
                type_path,
                variant_name,
            } => {
                write!(
                    f,
                    "`reflect_clone` not supported for `{type_path}`'s variant `{variant_name}`"
                )
            }
        }
    }
}

// ----------------------------------------------------------------------------
// ApplyError

/// Error returned when [`reflect_apply`] fails.
///
/// Carries the source and target type paths along with a human-readable
/// description of the failure.
///
/// Note: the field naming reflects the direction of the apply operation:
/// `src` names the *receiver* (the target that was being applied *to*),
/// while `apply` names the *source* (the value being applied *from*).
///
/// [`reflect_apply`]: crate::Reflect::reflect_apply
#[derive(Debug)]
pub struct ApplyError {
    /// The [`TypePath`](crate::path::TypePath) of the *receiver* type — the
    /// target that was being applied to.
    pub src: &'static str,
    /// The [`TypePath`](crate::path::TypePath) of the source type being applied.
    pub apply: &'static str,
    /// A human-readable description of the failure.
    pub error: String,
}

impl Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { src, apply, error } = self;
        write!(f, "Failed to apply `{apply}` to `{src}`: {error}")
    }
}

impl Error for ApplyError {}

impl ApplyError {
    /// Creates an error for a [`ReflectKind`] mismatch.
    #[cold]
    pub fn mismatched_kind(
        src: &'static str,
        apply: &'static str,
        expected: ReflectKind,
        received: ReflectKind,
    ) -> Self {
        let error =
            format!("Mismatched reflect kind, expected `{expected}`, received `{received}`.");
        Self { src, apply, error }
    }

    /// Creates an error for a size/length mismatch.
    #[cold]
    pub fn mismatched_size(
        src: &'static str,
        apply: &'static str,
        expected: usize,
        received: usize,
    ) -> Self {
        let error = format!("Mismatched size, expected `{expected}`, received `{received}`.");
        Self { src, apply, error }
    }

    /// Creates an error for a mismatched item type within a container.
    #[cold]
    pub fn mismatched_item(
        src: &'static str,
        apply: &'static str,
        item_name: &str,
        expected: &str,
        received: &str,
    ) -> Self {
        let error =
            format!("Mismatched item `{item_name}`, expected `{expected}`, received `{received}`.");
        Self { src, apply, error }
    }

    /// Creates an error for a mismatched enum variant kind.
    #[cold]
    pub fn mismatched_variant(
        src: &'static str,
        apply: &'static str,
        variant_name: &str,
        expected: VariantKind,
        received: VariantKind,
    ) -> Self {
        let error = format!(
            "Mismatched variant `{variant_name}`, expected `{expected}`, received `{received}`."
        );
        Self { src, apply, error }
    }

    /// Creates an error for a clone failure on a specific item.
    #[cold]
    pub fn clone_error(
        src: &'static str,
        apply: &'static str,
        item_name: &str,
        error: CloneError,
    ) -> Self {
        let error = format!("The item `{item_name}` cannot be cloned: {error}.");
        Self { src, apply, error }
    }
}
