use core::any::{Any, TypeId};

use super::{Attributes, Generics, Type, TypeInfo, Typed};
use super::{impl_attributes_fn, impl_with_attributes};
use super::{impl_generics_fn, impl_type_fn, impl_with_generics};
use crate::Reflect;
use crate::ops::Map;
use crate::path::TypePath;

// -----------------------------------------------------------------------------
// MapInfo

/// A container for compile-time map-like info.
#[derive(Debug)]
pub struct MapInfo {
    ty: Type,
    // Cache type_id.
    key_id: TypeId,
    value_id: TypeId,
    // `TypeInfo` is created on first access; use function pointers to delay it.
    key_info: fn() -> &'static TypeInfo,
    value_info: fn() -> &'static TypeInfo,
    generics: Generics,
    attributes: Attributes,
    /// Serde type info used for serialization and deserialization.
    ///
    /// Usually `None`, meaning the reflected structure is used directly.
    serde_info: Option<&'static TypeInfo>,
}

impl MapInfo {
    impl_type_fn!(ty);
    impl_generics_fn!(generics);
    impl_with_generics!(generics);
    impl_attributes_fn!(attributes);
    impl_with_attributes!(attributes);

    /// Create a new [`MapInfo`].
    #[inline]
    pub const fn new<TMap: Map + TypePath, TKey: Reflect + Typed, TValue: Reflect + Typed>() -> Self
    {
        Self {
            ty: Type::of::<TMap>(),
            key_id: TypeId::of::<TKey>(),
            value_id: TypeId::of::<TValue>(),
            key_info: TKey::type_info,
            value_info: TValue::type_info,
            generics: Generics::EMPTY,
            attributes: Attributes::EMPTY,
            serde_info: None,
        }
    }

    /// Returns the [`TypeId`] of the key.
    #[inline]
    pub const fn key_id(&self) -> TypeId {
        self.key_id
    }

    /// Returns `true` if the key type is `T`.
    #[inline]
    pub fn key_is<T: Any>(&self) -> bool {
        self.key_id == TypeId::of::<T>()
    }

    /// Returns the [`TypeId`] of the value.
    #[inline]
    pub const fn value_id(&self) -> TypeId {
        self.value_id
    }

    /// Returns `true` if the value type is `T`.
    #[inline]
    pub fn value_is<T: Any>(&self) -> bool {
        self.value_id == TypeId::of::<T>()
    }

    /// Returns the key's [`TypeInfo`].
    #[inline]
    pub fn key_info(&self) -> &'static TypeInfo {
        (self.key_info)()
    }

    /// Returns the value's [`TypeInfo`].
    #[inline]
    pub fn value_info(&self) -> &'static TypeInfo {
        (self.value_info)()
    }
}

impl MapInfo {
    /// Sets the serde type info, overriding the reflected representation.
    #[inline]
    pub const fn with_serde_info(self, info: &'static TypeInfo) -> Self {
        Self {
            serde_info: Some(info),
            ..self
        }
    }

    /// Returns the serde type info, if any.
    #[inline]
    pub const fn serde_info(&self) -> Option<&'static TypeInfo> {
        self.serde_info
    }

    /// Creates a [`MapInfo`] without the [`Map`] requirement.
    ///
    /// Otherwise identical to [`MapInfo::new`].
    #[inline]
    pub const fn dynamic<
        TMap: Reflect + TypePath,
        TKey: Reflect + Typed,
        TValue: Reflect + Typed,
    >() -> Self {
        Self {
            ty: Type::of::<TMap>(),
            key_id: TypeId::of::<TKey>(),
            value_id: TypeId::of::<TValue>(),
            key_info: TKey::type_info,
            value_info: TValue::type_info,
            generics: Generics::EMPTY,
            attributes: Attributes::EMPTY,
            serde_info: None,
        }
    }
}

// -----------------------------------------------------------------------------
