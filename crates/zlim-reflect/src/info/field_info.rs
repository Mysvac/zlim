use core::any::{Any, TypeId};

use crate::info::{Attributes, TypeInfo, Typed};
use crate::info::{impl_attributes_fn, impl_with_attributes};

// ----------------------------------------------------------------------------
// NamedField

/// Information for a named (struct) field.
#[derive(Clone, Copy, Debug)]
pub struct NamedField {
    id: TypeId,
    name: &'static str,
    // `TypeInfo` is created on first access;
    // using a function pointer delays it.
    type_info: fn() -> &'static TypeInfo,
    attributes: Attributes,
}

impl NamedField {
    impl_attributes_fn!(attributes);
    impl_with_attributes!(attributes);

    /// Creates a new [`NamedField`] for the given field `name` and type `T`.
    #[inline]
    pub const fn new<T: Typed>(name: &'static str) -> Self {
        Self {
            name,
            id: TypeId::of::<T>(),
            type_info: T::type_info,
            attributes: Attributes::EMPTY,
        }
    }

    /// Returns the `TypeId`.
    #[inline]
    pub const fn type_id(&self) -> TypeId {
        self.id
    }

    /// Check if the given type matches this one.
    #[inline]
    pub fn type_is<T: Any>(&self) -> bool {
        self.id == TypeId::of::<T>()
    }

    /// Returns the field name.
    #[inline]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the field's [`TypeInfo`].
    #[inline]
    pub fn type_info(&self) -> &'static TypeInfo {
        (self.type_info)()
    }
}

// ----------------------------------------------------------------------------
// UnnamedField

/// Information for an unnamed (tuple) field.
#[derive(Clone, Copy, Debug)]
pub struct UnnamedField {
    id: TypeId,
    index: usize,
    // `TypeInfo` is created on first access;
    // using a function pointer delays it.
    type_info: fn() -> &'static TypeInfo,
    attributes: Attributes,
}

impl UnnamedField {
    impl_attributes_fn!(attributes);
    impl_with_attributes!(attributes);

    /// Creates a new [`UnnamedField`] for the field at `index` with type `T`.
    #[inline]
    pub const fn new<T: Typed>(index: usize) -> Self {
        Self {
            index,
            id: TypeId::of::<T>(),
            type_info: T::type_info,
            attributes: Attributes::EMPTY,
        }
    }

    /// Returns the `TypeId`.
    #[inline]
    pub const fn type_id(&self) -> TypeId {
        self.id
    }

    /// Check if the given type matches this one.
    #[inline]
    pub fn type_is<T: Any>(&self) -> bool {
        self.id == TypeId::of::<T>()
    }

    /// Returns the field index (position in the tuple struct).
    #[inline]
    pub const fn index(&self) -> usize {
        self.index
    }

    /// Returns the field's [`TypeInfo`].
    #[inline]
    pub fn type_info(&self) -> &'static TypeInfo {
        (self.type_info)()
    }
}

// ----------------------------------------------------------------------------
