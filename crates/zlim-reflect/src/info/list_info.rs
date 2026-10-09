use core::any::{Any, TypeId};

use super::{Attributes, Generics, Type, TypeInfo, Typed};
use super::{impl_attributes_fn, impl_with_attributes};
use super::{impl_generics_fn, impl_type_fn, impl_with_generics};
use crate::Reflect;
use crate::ops::List;
use crate::path::TypePath;

// -----------------------------------------------------------------------------
// ListInfo

/// A container for compile-time list-like info.
#[derive(Debug)]
pub struct ListInfo {
    ty: Type,
    item_id: TypeId,
    // `TypeInfo` is created on the first visit,
    // use function pointers to delay it.
    item_info: fn() -> &'static TypeInfo,
    generics: Generics,
    attributes: Attributes,
    /// Serde type info used for serialization and deserialization.
    ///
    /// Usually `None`, meaning the reflected structure is used directly.
    schema_info: Option<&'static TypeInfo>,
}

impl ListInfo {
    impl_type_fn!(ty);
    impl_generics_fn!(generics);
    impl_with_generics!(generics);
    impl_attributes_fn!(attributes);
    impl_with_attributes!(attributes);

    /// Creates a new [`ListInfo`].
    #[inline]
    pub const fn new<TList: List + TypePath, TItem: Reflect + Typed>() -> Self {
        Self {
            ty: Type::of::<TList>(),
            item_id: TypeId::of::<TItem>(),
            item_info: TItem::type_info,
            generics: Generics::EMPTY,
            attributes: Attributes::EMPTY,
            schema_info: None,
        }
    }

    /// Returns the [`TypeId`] of list items.
    #[inline]
    pub const fn item_id(&self) -> TypeId {
        self.item_id
    }

    /// Returns `true` if the item type is `T`.
    #[inline]
    pub fn item_is<T: Any>(&self) -> bool {
        self.item_id == TypeId::of::<T>()
    }

    /// Returns the [`TypeInfo`] of list items.
    #[inline]
    pub fn item_info(&self) -> &'static TypeInfo {
        (self.item_info)()
    }
}

impl ListInfo {
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

    /// Creates a [`ListInfo`] without the [`List`] requirement.
    ///
    /// Otherwise identical to [`ListInfo::new`].
    #[inline]
    pub const fn dynamic<TList: Reflect + TypePath, TItem: Reflect + Typed>() -> Self {
        Self {
            ty: Type::of::<TList>(),
            item_id: TypeId::of::<TItem>(),
            item_info: TItem::type_info,
            generics: Generics::EMPTY,
            attributes: Attributes::EMPTY,
            schema_info: None,
        }
    }
}

// -----------------------------------------------------------------------------
