use core::fmt::{Debug, Formatter};

use super::Reflect;

// ----------------------------------------------------------------------------
// Opaque

/// A reflection interface for opaque (unstructured) values.
///
/// An "opaque" type is one whose internal layout is not exposed to the
/// reflection system — primitive types (`i32`, `f64`, `bool`, etc.) and
/// heap-backed types (`String`, etc.) fall into this category.
///
/// Opaque values are serialized to a compact string representation and can
/// be edited through typed apply methods ([`apply_str`]).
///
/// # FromReflect Specialization
///
/// Opaque types may specialize [`from_reflect`]:
/// because all opaque values implement [`stringify`], they
/// can convert between different concrete types by serializing the source to
/// a string and deserializing into the target. For example, an `i32` can be
/// converted from a `String` (and vice versa) through this mechanism.
///
/// # Hash and Equality
///
/// The default [`reflect_hash`] and [`reflect_eq`] for opaque types are
/// text-based: values are compared/hashed via their [`stringify`]
/// representation. This ensures that types with unusual equality semantics
/// (e.g. `f32` / `f64` with `NaN != NaN`) work correctly in hash-based
/// containers.
///
/// # Examples
///
/// ## Primitive types
///
/// ```
/// use zlim_reflect::ops::{Opaque, Reflect};
///
/// let val = 42i32;
/// let s = val.stringify();
/// assert_eq!(s, "42");
///
/// let mut target = 0i32;
/// target.apply_str("99").unwrap();
/// assert_eq!(target, 99);
/// ```
///
/// ## Cross-type conversion via stringify
///
/// ```
/// use zlim_reflect::ops::{Opaque, Reflect};
///
/// // Convert an i32 to a String through the stringify/apply_str round-trip.
/// let val = 42i32;
/// let s = val.stringify();
/// assert_eq!(s, "42");
///
/// let mut text = String::new();
/// text.apply_str(&s).unwrap();
/// assert_eq!(text, "42");
/// ```
///
/// [`from_reflect`]: crate::Reflect::from_reflect
/// [`reflect_hash`]: crate::Reflect::reflect_hash
/// [`reflect_eq`]: crate::Reflect::reflect_eq
/// [`stringify`]: Opaque::stringify
/// [`apply_str`]: Opaque::apply_str
pub trait Opaque: Reflect {
    /// Applies a string value to this opaque value.
    fn apply_str(&mut self, v: &str) -> Result<(), String>;

    /// Serializes this opaque value into a compact string.
    fn stringify(&self) -> String;
}

impl Debug for dyn Opaque {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Opaque").field(&self.stringify()).finish()
    }
}

// ----------------------------------------------------------------------------
