use core::any::{Any, TypeId};

use super::{Attributes, Generics, Type, TypeInfo, Typed};
use super::{impl_attributes_fn, impl_with_attributes};
use super::{impl_generics_fn, impl_type_fn, impl_with_generics};
use crate::Reflect;
use crate::ops::Set;
use crate::path::TypePath;

// -----------------------------------------------------------------------------
// SetInfo

/// A container for compile-time set-like info.
#[derive(Debug)]
pub struct SetInfo {
    ty: Type,
    // Cache type_id.
    value_id: TypeId,
    // `TypeInfo` is created on first access; use a function pointer to delay it.
    value_info: fn() -> &'static TypeInfo,
    generics: Generics,
    attributes: Attributes,
}

impl SetInfo {
    impl_type_fn!(ty);
    impl_generics_fn!(generics);
    impl_with_generics!(generics);
    impl_attributes_fn!(attributes);
    impl_with_attributes!(attributes);

    /// Create a new [`SetInfo`].
    #[inline]
    pub const fn new<TSet: Set + TypePath, TValue: Reflect + Typed>() -> Self {
        Self {
            ty: Type::of::<TSet>(),
            value_id: TypeId::of::<TValue>(),
            value_info: TValue::type_info,
            generics: Generics::EMPTY,
            attributes: Attributes::EMPTY,
        }
    }

    /// Returns the [`TypeId`] of the set's element type.
    #[inline]
    pub const fn value_id(&self) -> TypeId {
        self.value_id
    }

    /// Returns `true` if the value type is `T`.
    #[inline]
    pub fn value_is<T: Any>(&self) -> bool {
        self.value_id == TypeId::of::<T>()
    }

    /// Returns the value element's [`TypeInfo`].
    #[inline]
    pub fn value_info(&self) -> &'static TypeInfo {
        (self.value_info)()
    }
}

// -----------------------------------------------------------------------------
