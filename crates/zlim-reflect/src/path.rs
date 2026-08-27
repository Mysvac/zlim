//! Traits for stable type-path identifiers and their dynamic dispatch.
//!
//! # Menu
//!
//! - [`TypePath`]: A trait for obtaining canonical type names without a leading `::`.
//!     - [`type_path`]: Full name, a fixed and unique identifier for the type.
//!     - [`type_name`]: The name without module path, may be duplicated.
//!     - [`IDENT`]: The name without generics and module path.
//!     - [`MODULE`]: Optional module path.
//!     - [`CRATE`]: Optional crate name.
//!
//! - [`DynamicTypePath`]: Dynamic dispatch support for `TypePath`.
//!
//! - [`PathCell`]: A `Map<TypeId, str>` used to store static type names.
//!
//! - [`concat()`]: A simple and efficient runtime string concat function.
//!
//! [`type_path`]: crate::path::TypePath::type_path
//! [`type_name`]: crate::path::TypePath::type_name
//! [`IDENT`]: crate::path::TypePath::IDENT
//! [`MODULE`]: crate::path::TypePath::MODULE
//! [`CRATE`]: crate::path::TypePath::CRATE

pub use zlim_reflect_derive::TypePath;

// -----------------------------------------------------------------------------
// TypePath

/// A static accessor to type paths and names.
///
/// Provide a stable and flexible alternative to [`core::any::type_name`]
/// that works across compiler versions and survives code refactoring.
///
/// # Associated items
///
/// | Item | Kind | Description |
/// |------|------|-------------|
/// | [`type_path`] | fn | The unique identifier of the type, cannot be duplicated. |
/// | [`type_name`] | fn | Short type name without module path, may be duplicated. |
/// | [`IDENT`] | const | The shortest type name without module path and generics. |
/// | [`CRATE`] | const | Optional crate name. |
/// | [`MODULE`] | const | Optional module path. |
///
/// We guarantee that these names do not have the prefix `::`.
/// Users should also ensure this when manually implementing it.
///
/// [`type_path`]: TypePath::type_path
/// [`type_name`]: TypePath::type_name
/// [`IDENT`]: TypePath::IDENT
/// [`MODULE`]: TypePath::MODULE
/// [`CRATE`]: TypePath::CRATE
#[diagnostic::on_unimplemented(
    message = "`{Self}` does not implement `TypePath`",
    label = "invalid `TypePath`",
    note = "consider annotating `{Self}` with `#[derive(TypePath)]`"
)]
pub trait TypePath: 'static {
    /// Returns the fully qualified path with generics of the target type.
    ///
    /// This is the complete unique identifier of a type,
    /// and should **not** duplicated in different types.
    ///
    /// For `Option<Vec<usize>>`, this is `"core::option::Option<alloc::vec::Vec<usize>>"`.
    fn type_path() -> &'static str;

    /// Returns a short, pretty-print enabled path to the type.
    ///
    /// This name allows for duplication.
    ///
    /// Note that this is different from [`core::any::type_name`],
    /// the latter is more like [`TypePath::type_path`].
    ///
    /// For `Option<Vec<usize>>`, this is `"Option<Vec<usize>>"`.
    fn type_name() -> &'static str;

    /// The short name of the type, without generics.
    ///
    /// Compile time evaluation for better performance.
    ///
    /// - For `Option<Vec<usize>>`, this is `"Option"`.
    ///
    /// The Ident of arrays and tuples is special, and generics
    /// are replaced by '_'.
    ///
    /// - For `()`, this is `()`.
    /// - For `(A,)`, this is `(_,)`.
    /// - For `(A, B)`, this is `(_, _)`.
    /// - For `[u32; 5]`, this is `[_; _]`.
    /// - For `&A`, `&str` ... this is `&_`.
    const IDENT: &str;

    /// Optional crate name where the type is defined.
    ///
    /// Primitive built-in types may return `None`.
    ///
    /// Compile time evaluation for better performance.
    ///
    /// For `Option<Vec<usize>>`, this is `Some("core")`.
    const CRATE: Option<&str>;

    /// Optional module path where the type is defined.
    ///
    /// Primitive built-in types may return `None`.
    ///
    /// Compile time evaluation for better performance.
    ///
    /// For `Option<Vec<usize>>`, this is `Some("core::option")`.
    const MODULE: Option<&str>;
}

// -----------------------------------------------------------------------------
// DynamicTypePath

/// Provide dynamic dispatch for types that implement [`TypePath`].
///
/// Auto impl for all types that implemented [`TypePath`].
pub trait DynamicTypePath {
    /// Returns the fully qualified path with generics of the underlying type.
    ///
    /// See [`TypePath::type_path`].
    fn reflect_type_path(&self) -> &'static str;

    /// Returns a short, pretty-print enabled path to the type.
    ///
    /// See [`TypePath::type_name`].
    fn reflect_type_name(&self) -> &'static str;

    /// Returns the short name of the type, without generics.
    ///
    /// See [`TypePath::IDENT`].
    fn reflect_type_ident(&self) -> &'static str;

    /// Optional module path where the type is defined.
    ///
    /// See [`TypePath::MODULE`].
    fn reflect_module_path(&self) -> Option<&'static str>;

    /// Optional crate name where the type is defined.
    ///
    /// See [`TypePath::CRATE`].
    fn reflect_crate_name(&self) -> Option<&'static str>;
}

impl<T: TypePath> DynamicTypePath for T {
    #[inline]
    fn reflect_type_path(&self) -> &'static str {
        Self::type_path()
    }

    #[inline]
    fn reflect_type_name(&self) -> &'static str {
        Self::type_name()
    }

    #[inline]
    fn reflect_type_ident(&self) -> &'static str {
        Self::IDENT
    }

    #[inline]
    fn reflect_module_path(&self) -> Option<&'static str> {
        Self::MODULE
    }

    #[inline]
    fn reflect_crate_name(&self) -> Option<&'static str> {
        Self::CRATE
    }
}

// -----------------------------------------------------------------------------
// GenericTypePathCell

use core::any::TypeId;
use std::sync::{PoisonError, RwLock};
use zlim_utils::ext::TypeMap;

/// Container for static storage of type path with generics.
///
/// # Example
///
/// Non-generic types: implementation is straightforward.
///
/// ```
/// use zlim_reflect::path::*;
///
/// struct MyU8(u8);
///
/// impl TypePath for MyU8 {
///     fn type_path() -> &'static str { "my_crate::MyU8" }
///     fn type_name() -> &'static str { "MyU8" }
///     const IDENT: &str = "MyU8";
///     const MODULE: Option<&str> = Some("my_crate");
///     const CRATE: Option<&str> = Some("my_crate");
/// }
/// ```
///
/// Generic types need `PathCell` to cache each instantiation's path,
/// since a different `T` produces a different string.
///
/// ```
/// use zlim_reflect::path::*;
///
/// enum MyOption<T> { None, Some(T) }
///
/// impl<T: TypePath> TypePath for MyOption<T> {
///     fn type_path() -> &'static str {
///         static CELL: PathCell = PathCell::new();
///         CELL.get_or_init::<Self>(|| concat(&["my_crate::MyOption<", T::type_path(), ">"]))
///     }
///     fn type_name() -> &'static str {
///         static CELL: PathCell = PathCell::new();
///         CELL.get_or_init::<Self>(|| concat(&["MyOption<", T::type_name(), ">"]))
///     }
///     const IDENT: &str = "MyOption";
///     const MODULE: Option<&str> = Some("my_crate");
///     const CRATE: Option<&str> = Some("my_crate");
/// }
/// ```
pub struct PathCell(RwLock<TypeMap<&'static str>>);

impl PathCell {
    /// Create a empty cell.
    #[expect(clippy::new_without_default, reason = "need `const`")]
    pub const fn new() -> Self {
        Self(RwLock::new(TypeMap::new()))
    }

    /// Returns a reference to the `Info` stored in the cell.
    ///
    /// This method will then return the correct `Info` reference for the given type `T`.
    /// If there is no entry found, a new one will be generated from the given function.
    #[inline(always)]
    pub fn get_or_init<T: 'static + ?Sized>(&self, f: impl FnOnce() -> String) -> &'static str {
        match self.get_by_type_id(TypeId::of::<T>()) {
            Some(info) => info,
            None => self.insert_by_type_id(TypeId::of::<T>(), &f()),
        }
    }

    // Separate to reduce code compilation times
    #[inline(never)]
    fn get_by_type_id(&self, type_id: TypeId) -> Option<&'static str> {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(type_id)
            .copied()
    }

    // Separate to reduce code compilation times
    #[cold]
    #[inline(never)]
    fn insert_by_type_id(&self, type_id: TypeId, s: &str) -> &'static str {
        // Use `Global::alloc_str` rather than `intern_str`:
        // `intern_str`'s internal hash table is wasted here since `TypePath`
        // is typically called once per type. Concurrent allocations of the
        // same name are rare and acceptable — we avoid holding the write lock
        // during allocation.
        let value = zlim_utils::mem::Global::alloc_str(s);

        self.0
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_insert(type_id, || value)
    }
}

// -----------------------------------------------------------------------------
// Concat helper

/// An efficient string concatenation function.
///
/// This is usually used for the implementation of [`TypePath`].
///
/// # Design
///
/// Pre-calculates the total byte length and pre-allocates the buffer,
/// which is roughly 30% faster than chaining [`String::push_str`] calls
/// that may trigger multiple reallocations.
///
/// Inline is intentionally disabled here, because this function is called
/// extensively in generic contexts.
///
/// # Example
///
/// ```
/// use zlim_reflect::path;
///
/// let s = path::concat(&["module", "::", "name", "<", "T", ">"]);
///
/// assert_eq!(s.as_str(), "module::name<T>");
/// ```
#[inline(never)]
pub fn concat(arr: &[&str]) -> String {
    use core::ptr::copy_nonoverlapping;

    let mut len = 0usize;
    for &item in arr {
        len += item.len();
    }

    let mut res = String::with_capacity(len);

    #[expect(unsafe_code, reason = "skip length assertions")]
    unsafe {
        let buf = res.as_mut_vec();
        let mut dst = buf.as_mut_ptr();

        for &new in arr {
            copy_nonoverlapping::<u8>(new.as_ptr(), dst, new.len());
            dst = dst.add(new.len());
        }

        buf.set_len(len);
    }

    res
}

// -----------------------------------------------------------------------------
// Tests

// see <zlim-reflect/tests/type_path.rs> for macros's tests

#[cfg(test)]
mod tests {
    use super::concat;

    #[test]
    fn empty_slice() {
        let s = concat(&[]);
        assert_eq!(s, "");
        assert!(s.is_empty());
    }

    #[test]
    fn single_string() {
        let s = concat(&["hello"]);
        assert_eq!(s, "hello");
        assert_eq!(s.len(), 5);
    }

    #[test]
    fn multiple_strings() {
        let s = concat(&["module", "::", "name", "<", "T", ">"]);
        assert_eq!(s, "module::name<T>");
    }

    #[test]
    fn empty_strings() {
        let s = concat(&["", "hello", "", "world", ""]);
        assert_eq!(s, "helloworld");
        assert_eq!(s.len(), 10);
    }

    #[test]
    fn unicode_characters() {
        let s = concat(&["ASCII", " 世界 ", "🌟", " 123"]);
        assert_eq!(s, "ASCII 世界 🌟 123");
    }

    #[test]
    fn long_strings() {
        let long = "a".repeat(1000);
        let s = concat(&[&long, &long, &long]);
        assert_eq!(s.len(), 3000);
        assert_eq!(s, long.clone() + &long + &long);
    }

    #[test]
    fn capacity_exact() {
        let parts = ["hello", " ", "world"];
        let expected_len: usize = parts.iter().map(|s| s.len()).sum();
        let s = concat(&parts);
        assert_eq!(s.len(), expected_len);
        assert!(s.capacity() >= expected_len);
    }

    #[test]
    fn many_small_strings() {
        let parts: Vec<&str> = (0..10000).map(|_| "a").collect();
        let s = concat(&parts);
        assert_eq!(s.len(), 10000);
        assert!(s.chars().all(|c| c == 'a'));
    }
}
