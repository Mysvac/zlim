use core::fmt::{Debug, Formatter};
use core::iter::FusedIterator;
use std::borrow::Cow;

use super::Reflect;
use crate::info::VariantKind;

// ----------------------------------------------------------------------------
// Enum trait

/// A trait for reflected enum types.
///
/// Allows enums to be inspected and manipulated dynamically at runtime
/// without knowing the concrete type.
///
/// # FromReflect Conversion
///
/// The [`from_reflect`] and [`reflect_apply`] rules depend on the variant kind:
///
/// - **Struct variants** are **lenient** (like [`Struct`]): extra fields in the
///   source are ignored, but all non-default fields must be present.
/// - **Tuple variants** are **strict** (like [`Tuple`]): the field count must
///   match exactly.
/// - **Unit variants** carry no data and require no fields.
///
/// # Variant Kinds
///
/// | Kind   | Syntax                         |
/// |--------|--------------------------------|
/// | Unit   | `MyEnum::Foo`                  |
/// | Tuple  | `MyEnum::Foo(i32, i32)`        |
/// | Struct | `MyEnum::Foo { value: String }`|
///
/// See [`VariantKind`] for details.
///
/// [`reflect_apply`]: crate::Reflect::reflect_apply
/// [`from_reflect`]: crate::Reflect::from_reflect
/// [`Struct`]: super::Struct
/// [`Tuple`]: super::Tuple
pub trait Enum: Reflect {
    /// Returns a reference to the named field in the current variant.
    ///
    /// Returns `None` for non-[`VariantKind::Struct`] variants.
    fn field(&self, name: &str) -> Option<&dyn Reflect>;

    /// Returns a reference to the field at `index` in the current variant.
    fn field_at(&self, index: usize) -> Option<&dyn Reflect>;

    /// Returns a mutable reference to the named field in the current
    /// variant.
    ///
    /// Returns `None` for non-[`VariantKind::Struct`] variants.
    fn field_mut(&mut self, name: &str) -> Option<&mut dyn Reflect>;

    /// Returns a mutable reference to the field at `index` in the current
    /// variant.
    fn field_at_mut(&mut self, index: usize) -> Option<&mut dyn Reflect>;

    /// Returns the name of the field at `index` in the current variant.
    ///
    /// Returns `None` for non-[`VariantKind::Struct`] variants.
    fn field_name_at(&self, index: usize) -> Option<&str>;

    /// Returns the index of the field with the given `name` in the
    /// current variant, or `None` if no such field exists.
    ///
    /// Always returns `None` for non-struct variants.
    fn field_index_of(&self, name: &str) -> Option<usize>;

    /// Returns the number of fields in the current variant.
    fn field_len(&self) -> usize;

    /// Returns an iterator over the current variant's fields.
    fn iter_fields(&self) -> VariantFieldIter<'_>;

    /// Returns the [`VariantKind`] of the current variant.
    fn variant_kind(&self) -> VariantKind;

    /// Returns the declaration-order index of the current variant.
    fn variant_index(&self) -> usize;

    /// Returns the name of the current variant.
    fn variant_name(&self) -> &str;

    /// Consumes the enum and returns its fields as `(name_or_none, value)`
    /// pairs. The name is `None` for tuple-variant fields and `Some(name)`
    /// for struct-variant fields.
    fn unpack(self: Box<Self>) -> Vec<(Option<Cow<'static, str>>, Box<dyn Reflect>)>;
}

impl Debug for dyn Enum {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let path = format_args!("{}::{}", self.reflect_type_path(), self.variant_name());
        f.debug_tuple("Enum").field(&path).finish()
    }
}

// ----------------------------------------------------------------------------
// Variant Field Iterator

/// An iterator over the fields of the current enum variant.
///
/// Yields [`&dyn Reflect`] values in declaration order.
///
/// [`&dyn Reflect`]: crate::Reflect
pub struct VariantFieldIter<'a> {
    data: &'a dyn Enum,
    index: usize,
}

impl<'a> VariantFieldIter<'a> {
    /// Creates a new iterator for the given enum.
    #[inline(always)]
    pub const fn new(data: &'a dyn Enum) -> Self {
        Self { data, index: 0 }
    }
}

impl<'a> Iterator for VariantFieldIter<'a> {
    type Item = &'a dyn Reflect;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let value = self.data.field_at(self.index);
        self.index += value.is_some() as usize;
        value
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let hint = self.data.field_len() - self.index;
        (hint, Some(hint))
    }
}

impl ExactSizeIterator for VariantFieldIter<'_> {}
impl FusedIterator for VariantFieldIter<'_> {}

// ----------------------------------------------------------------------------
