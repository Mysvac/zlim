//! Direction types: unit vectors that are guaranteed to be normalized.
//!
//! Each dimension has its own type — [`Dir2`], [`Dir3`], [`Dir3A`], [`Dir4`] —
//! and its own file. They share only two things, both defined here: the
//! [`InvalidDirectionError`] a failed construction reports, and the
//! [`assert_is_normalized`] check behind the `unsafe` unchecked constructors.

use core::fmt::{Display, Formatter};

// ---------------------------------------------------------------------
// Modules

mod dir2;
mod dir3;
mod dir3a;
mod dir4;

pub use dir2::Dir2;
pub use dir3::Dir3;
pub use dir3a::Dir3A;
pub use dir4::Dir4;

// ---------------------------------------------------------------------
// InvalidDirectionError

/// An error indicating that a direction is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidDirectionError {
    /// The length of the direction vector is zero or very close to zero.
    Zero,
    /// The length of the direction vector is `std::f32::INFINITY`.
    Infinite,
    /// The length of the direction vector is `NaN`.
    NaN,
}

impl Display for InvalidDirectionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Zero => {
                f.write_str("The length of the direction vector is zero or very close to zero")
            }
            Self::Infinite => {
                f.write_str("The length of the direction vector is `std::f32::INFINITY`")
            }
            Self::NaN => f.write_str("The length of the direction vector is `NaN`"),
        }
    }
}

impl core::error::Error for InvalidDirectionError {}

impl InvalidDirectionError {
    /// Creates an [`InvalidDirectionError`] from the length of an invalid direction vector.
    pub const fn from_length(length: f32) -> Self {
        if length.is_nan() {
            InvalidDirectionError::NaN
        } else if !length.is_finite() {
            // If the direction is non-finite but also not NaN, it must be infinite
            InvalidDirectionError::Infinite
        } else {
            // If the direction is invalid but neither NaN nor infinite, it must be zero
            InvalidDirectionError::Zero
        }
    }
}

// ---------------------------------------------------------------------
// assert_is_normalized

/// Checks that a vector with the given squared length is normalized.
///
/// Warns for small error with a length threshold of approximately `1e-4`,
/// and panics for large error with a length threshold of approximately `1e-2`.
///
/// The format used for the logged warning is `"Warning: {warning} The length is {length}`,
/// and similarly for the error.
#[cfg(debug_assertions)]
pub(super) fn assert_is_normalized(message: &str, length_squared: f32) {
    use crate::ops;

    let length_error_squared = ops::abs(length_squared - 1.0);

    // Panic for large error and warn for slight error.
    if length_error_squared > 2e-2 || length_error_squared.is_nan() {
        ::core::hint::cold_path();
        // Length error is approximately 1e-2 or more.
        panic!(
            "Error: {message} The length is {}.",
            ops::sqrt(length_squared)
        );
    } else if length_error_squared > 2e-4 {
        ::core::hint::cold_path();
        // Length error is approximately 1e-4 or more.
        zlim_log::warn!(
            "Warning: {message} The length is {}.",
            ops::sqrt(length_squared)
        );
    }
}
