use zlim_utils::mem::Global;

use super::{Attributes, Generics, NamedField, Type};
use super::{impl_attributes_fn, impl_with_attributes};
use super::{impl_generics_fn, impl_type_fn, impl_with_generics};
use crate::ops::Struct;
use crate::path::TypePath;

// -----------------------------------------------------------------------------
// StructInfo

/// A container for compile-time named struct info.
#[derive(Debug)]
pub struct StructInfo {
    ty: Type,
    fields: &'static [NamedField],
    // Needed for deserialization.
    field_names: &'static [&'static str],
    generics: Generics,
    attributes: Attributes,
}

impl StructInfo {
    impl_type_fn!(ty);
    impl_generics_fn!(generics);
    impl_with_generics!(generics);
    impl_attributes_fn!(attributes);
    impl_with_attributes!(attributes);

    /// Create a new [`StructInfo`].
    ///
    /// The order of internal fields is fixed, depends on the input order.
    pub fn new<T: Struct + TypePath>(fields: &[NamedField]) -> Self {
        // No need `Vec::with_capacity`, Fixed length iterators
        // come with built-in optimizations.
        let name: Vec<&'static str> = fields.iter().map(|f| f.name()).collect();
        Self {
            ty: Type::of::<T>(),
            fields: Global::alloc_slice(fields),
            field_names: Global::alloc_slice(name.as_slice()),
            generics: Generics::EMPTY,
            attributes: Attributes::EMPTY,
        }
    }

    /// Returns the underlying fields slice.
    #[inline]
    pub fn fields(&self) -> &'static [NamedField] {
        self.fields
    }

    /// Returns the underlying fields slice.
    #[inline]
    pub fn field_names(&self) -> &'static [&'static str] {
        self.field_names
    }

    /// Returns the number of fields.
    #[inline]
    pub fn field_len(&self) -> usize {
        self.fields.len()
    }

    /// Returns the [`NamedField`] for the given `name`, if present.
    ///
    /// Complexity: O(n) in the number of fields.
    #[inline]
    pub fn field(&self, name: &str) -> Option<&'static NamedField> {
        self.fields.iter().find(|f| f.name() == name)
    }

    /// Returns the [`NamedField`] at the given index, if present.
    ///
    /// Complexity: O(1).
    #[inline]
    pub fn field_at(&self, index: usize) -> Option<&'static NamedField> {
        self.fields.get(index)
    }

    /// Returns the index for the given field `name`, if present.
    ///
    /// Complexity: O(n) in the number of fields.
    #[inline]
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.field_names.iter().position(|n| *n == name)
    }

    /// Returns the field name for the given index, if present.
    ///
    /// Complexity: O(1).
    #[inline]
    pub fn name_at(&self, index: usize) -> Option<&'static str> {
        self.field_names.get(index).copied()
    }
}

// -----------------------------------------------------------------------------
