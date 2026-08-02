use super::{Attributes, Generics, Type};
use super::{impl_attributes_fn, impl_docs_fn, impl_with_attributes};
use super::{impl_generics_fn, impl_type_fn, impl_with_generics};
use crate::Reflect;
use crate::ops::Opaque;
use crate::path::TypePath;

// ----------------------------------------------------------------------------
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
    #[cfg(feature = "reflect_docs")]
    docs: Option<&'static str>,
}

impl OpaqueInfo {
    impl_type_fn!(ty);
    impl_generics_fn!(generics);
    impl_with_generics!(generics);
    impl_attributes_fn!(attributes);
    impl_with_attributes!(attributes);
    impl_docs_fn!(docs);

    /// Create a new [`OpaqueInfo`].
    #[inline]
    pub const fn new<T: Opaque + TypePath + ?Sized>() -> Self {
        Self {
            ty: Type::of::<T>(),
            generics: Generics::EMPTY,
            attributes: Attributes::EMPTY,
            #[cfg(feature = "reflect_docs")]
            docs: None,
        }
    }

    /// Create a new [`OpaqueInfo`] for Dynamic Types.
    #[inline]
    pub const fn dynamic<T: Reflect + TypePath>() -> Self {
        Self {
            ty: Type::of::<T>(),
            generics: Generics::EMPTY,
            attributes: Attributes::EMPTY,
            #[cfg(feature = "reflect_docs")]
            docs: None,
        }
    }
}

// ----------------------------------------------------------------------------
