use zlim_utils::mem::Global;

use super::{Attributes, Generics, Type, TypeInfo, UnnamedField};
use super::{impl_attributes_fn, impl_with_attributes};
use super::{impl_generics_fn, impl_type_fn, impl_with_generics};
use crate::Reflect;
use crate::ops::Tuple;
use crate::path::TypePath;

// -----------------------------------------------------------------------------
// TupleInfo

/// A container for compile-time tuple-struct info.
#[derive(Debug)]
pub struct TupleInfo {
    ty: Type,
    fields: &'static [UnnamedField],
    generics: Generics,
    attributes: Attributes,
    /// Serde type info used for serialization and deserialization.
    ///
    /// Usually `None`, meaning the reflected structure is used directly.
    schema_info: Option<&'static TypeInfo>,
}

impl TupleInfo {
    impl_type_fn!(ty);
    impl_generics_fn!(generics);
    impl_with_generics!(generics);
    impl_attributes_fn!(attributes);
    impl_with_attributes!(attributes);

    /// Create a new [`TupleInfo`].
    ///
    /// The order of internal fields is fixed, depends on the input order.
    #[inline]
    pub fn new<T: Tuple + TypePath>(fields: &[UnnamedField]) -> Self {
        TupleInfo {
            ty: Type::of::<T>(),
            fields: Global::alloc_slice(fields),
            generics: Generics::EMPTY,
            attributes: Attributes::EMPTY,
            schema_info: None,
        }
    }

    /// Returns the underlying fields slice.
    #[inline]
    pub fn fields(&self) -> &'static [UnnamedField] {
        self.fields
    }

    /// Returns the number of fields.
    #[inline]
    pub fn field_len(&self) -> usize {
        self.fields.len()
    }

    /// Returns the [`UnnamedField`] at the given index, if present.
    ///
    /// Complexity: O(1).
    #[inline]
    pub fn field(&self, index: usize) -> Option<&'static UnnamedField> {
        self.fields.get(index)
    }
}

impl TupleInfo {
    /// Sets the schema type info, overriding the reflected representation.
    #[inline]
    pub const fn with_schema_info(self, info: &'static TypeInfo) -> Self {
        Self {
            schema_info: Some(info),
            ..self
        }
    }

    /// Returns the schema type info, if any.
    #[inline]
    pub const fn schema_info(&self) -> Option<&'static TypeInfo> {
        self.schema_info
    }

    /// Creates a [`TupleInfo`] without the [`Tuple`] requirement.
    ///
    /// Otherwise identical to [`TupleInfo::new`].
    #[inline]
    pub fn dynamic<T: Reflect + TypePath>(fields: &[UnnamedField]) -> Self {
        Self {
            ty: Type::of::<T>(),
            fields: Global::alloc_slice(fields),
            generics: Generics::EMPTY,
            attributes: Attributes::EMPTY,
            schema_info: None,
        }
    }
}

impl TupleInfo {
    // A small optimization to avoid lock overhead.
    // See `src/impls/primitive/tuple.rs`.
    pub(crate) const UNIT: Self = Self {
        ty: Type::of::<()>(),
        fields: &[],
        generics: Generics::EMPTY,
        attributes: Attributes::EMPTY,
        schema_info: None,
    };
}

// -----------------------------------------------------------------------------
