use super::{Attributes, Generics, Type, TypeInfo};
use super::{impl_attributes_fn, impl_with_attributes};
use super::{impl_generics_fn, impl_type_fn, impl_with_generics};
use crate::Reflect;
use crate::ops::Opaque;
use crate::path::TypePath;

// -----------------------------------------------------------------------------
// OpaqueInfo

/// Metadata for types whose internals are opaque to the reflection system.
///
/// "Opaque" means the type's internal representation is not exposed — for
/// example primitive types like `u64` or heap-backed types like `String`.
#[derive(Debug)]
pub struct OpaqueInfo {
    ty: Type,
    generics: Generics,
    attributes: Attributes,
    /// Serde type info used for serialization and deserialization.
    ///
    /// Usually `None`, meaning the reflected structure is used directly.
    schema_info: Option<&'static TypeInfo>,
}

impl OpaqueInfo {
    impl_type_fn!(ty);
    impl_generics_fn!(generics);
    impl_with_generics!(generics);
    impl_attributes_fn!(attributes);
    impl_with_attributes!(attributes);

    /// Create a new [`OpaqueInfo`].
    #[inline]
    pub const fn new<T: Opaque + TypePath + ?Sized>() -> Self {
        Self {
            ty: Type::of::<T>(),
            generics: Generics::EMPTY,
            attributes: Attributes::EMPTY,
            schema_info: None,
        }
    }

    /// Create a new [`OpaqueInfo`] for Dynamic Types.
    #[inline]
    pub const fn dynamic<T: Reflect + TypePath>() -> Self {
        Self {
            ty: Type::of::<T>(),
            generics: Generics::EMPTY,
            attributes: Attributes::EMPTY,
            schema_info: None,
        }
    }
}

impl OpaqueInfo {
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
}

// -----------------------------------------------------------------------------
