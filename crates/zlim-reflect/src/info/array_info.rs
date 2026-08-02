use core::any::{Any, TypeId};

use super::{Attributes, Generics, Type, TypeInfo, Typed};
use super::{impl_attributes_fn, impl_docs_fn, impl_with_attributes};
use super::{impl_generics_fn, impl_type_fn, impl_with_generics};
use crate::Reflect;
use crate::ops::Array;
use crate::path::TypePath;

// ----------------------------------------------------------------------------
// ArrayInfo

/// A container for compile-time array information.
#[derive(Debug)]
pub struct ArrayInfo {
    ty: Type,
    // Cache `TypeId`.
    item_id: TypeId,
    // `TypeInfo` is created on the first visit,
    // use function pointers to delay it.
    item_info: fn() -> &'static TypeInfo,
    /// The compile-time length of the array.
    len: usize,
    generics: Generics,
    attributes: Attributes,
    #[cfg(feature = "reflect_docs")]
    docs: Option<&'static str>,
}

impl ArrayInfo {
    impl_type_fn!(ty);
    impl_generics_fn!(generics);
    impl_with_generics!(generics);
    impl_attributes_fn!(attributes);
    impl_with_attributes!(attributes);
    impl_docs_fn!(docs);

    /// Create a new [`ArrayInfo`].
    #[inline]
    pub const fn new<TArray: Array + TypePath, TItem: Reflect + Typed>(len: usize) -> Self {
        Self {
            ty: Type::of::<TArray>(),
            item_id: TypeId::of::<TItem>(),
            item_info: TItem::type_info,
            len,
            generics: Generics::EMPTY,
            attributes: Attributes::EMPTY,
            #[cfg(feature = "reflect_docs")]
            docs: None,
        }
    }

    /// Returns `true` if the compile-time length is zero.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The compile-time length of the array.
    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns the [`TypeId`] of an array item.
    #[inline]
    pub const fn item_id(&self) -> TypeId {
        self.item_id
    }

    /// Returns `true` if the item type is `T`.
    #[inline]
    pub fn item_is<T: Any>(&self) -> bool {
        self.item_id == TypeId::of::<T>()
    }

    /// Returns the [`TypeInfo`] of array items.
    #[inline]
    pub fn item_info(&self) -> &'static TypeInfo {
        (self.item_info)()
    }
}

// ----------------------------------------------------------------------------
