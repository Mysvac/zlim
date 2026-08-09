//! Dynamic representations of reflected types.
//!
//! Dynamic types are owned, type-erased containers that can hold any
//! [`Reflect`] values. They are the runtime counterpart of the ops traits
//! ([`Struct`], [`Tuple`], [`Array`], etc.) and allow constructing and
//! manipulating reflected data without knowing the concrete Rust types.
//!
//! # Design rationale
//!
//! Dynamic types are **data-transformation intermediaries**, typically used
//! as internal plumbing for serialization and deserialization.
//!
//! Bevy addresses dynamic typing through a `PartialReflect` trait, which
//! adds considerable complexity to the reflection system. We take a simpler
//! approach: dynamic types implement the full [`Reflect`] trait directly,
//! at the cost that their [`TypeInfo`] is always [`OpaqueInfo`] — it
//! does **not** match the [`ReflectKind`](crate::info::ReflectKind) reported by
//! [`reflect_kind`](crate::Reflect::reflect_kind). This mismatch makes it easy to
//! misjudge a dynamic type's capabilities when operating on trait objects.
//!
//! **Guidance:**
//!
//! - Use [`is_dynamic`](crate::Reflect::is_dynamic) to check whether a
//!   `&dyn Reflect` is a dynamic type before making decisions based on its
//!   [`TypeInfo`] or [`ReflectKind`](crate::info::ReflectKind).
//! - Do **not** convert dynamic types to `Box<dyn Reflect>` for long-term
//!   storage. Use them only for temporary data conversion. This way you can
//!   be confident that any `Box<dyn Reflect>` obtained from outside the
//!   conversion layer is always a regular (non-dynamic) object.
//! - Dynamic types do **not** implement [`TypeDatabase`], so they cannot
//!   participate in serialization or deserialization themselves. Do not use
//!   a dynamic type as a field of a custom reflected type.
//!
//! # Type Information
//!
//! Dynamic types report their [`TypeInfo`] as [`OpaqueInfo`], but their
//! [`reflect_kind`], [`reflect_ref`], [`reflect_mut`], and
//! [`reflect_owned`] methods behave like the represented kind.
//!
//! [`TypeDatabase`]: crate::db::TypeDatabase
//!
//! # Menu
//!
//! | Type | Ops Trait | Description |
//! |------|-----------|-------------|
//! | [`DynamicStruct`] | [`Struct`] | Named fields, map-like access |
//! | [`DynamicTuple`] | [`Tuple`] | Indexed fields, vec-like access |
//! | [`DynamicArray`] | [`Array`] | Fixed-size homogeneous sequence |
//! | [`DynamicList`] | [`List`] | Growable homogeneous sequence |
//! | [`DynamicMap`] | [`Map`] | Key-value associative container |
//! | [`DynamicSet`] | [`Set`] | Unique-element container |
//! | [`DynamicEnum`] | [`Enum`] | Enum variant representation |
//! | [`DynamicVariant`] | — | Enum variant data (Unit/Tuple/Struct) |
//!
//! [`Reflect`]: crate::Reflect
//! [`TypeInfo`]: crate::info::TypeInfo
//! [`OpaqueInfo`]: crate::info::OpaqueInfo
//! [`reflect_kind`]: crate::Reflect::reflect_kind
//! [`reflect_ref`]: crate::Reflect::reflect_ref
//! [`reflect_mut`]: crate::Reflect::reflect_mut
//! [`reflect_owned`]: crate::Reflect::reflect_owned
//! [`Struct`]: crate::ops::Struct
//! [`Tuple`]: crate::ops::Tuple
//! [`Array`]: crate::ops::Array
//! [`List`]: crate::ops::List
//! [`Map`]: crate::ops::Map
//! [`Set`]: crate::ops::Set
//! [`Enum`]: crate::ops::Enum

// -----------------------------------------------------------------------------
// Modules

mod dynamic_array;
mod dynamic_enum;
mod dynamic_list;
mod dynamic_map;
mod dynamic_set;
mod dynamic_struct;
mod dynamic_tuple;

// -----------------------------------------------------------------------------
// Exports

pub use dynamic_array::DynamicArray;
pub use dynamic_enum::DynamicEnum;
pub use dynamic_enum::DynamicVariant;
pub use dynamic_list::DynamicList;
pub use dynamic_map::DynamicMap;
pub use dynamic_set::DynamicSet;
pub use dynamic_struct::DynamicStruct;
pub use dynamic_tuple::DynamicTuple;

// -----------------------------------------------------------------------------
// Shared macros

/// Implements [`TypePath`] for a dynamic type.
///
/// [`TypePath`]: crate::path::TypePath
macro_rules! impl_dynamic_type_path {
    ($ty:ident) => {
        impl crate::path::TypePath for $ty {
            #[inline]
            fn type_path() -> &'static str {
                concat!("zlim_reflect::dynamic::", stringify!($ty))
            }

            #[inline]
            fn type_name() -> &'static str {
                stringify!($ty)
            }

            const IDENT: &str = stringify!($ty);
            const CRATE: Option<&str> = Some("zlim_reflect");
            const MODULE: Option<&str> = Some("zlim_reflect::dynamic");
        }
    };
}

/// Implements [`Typed`] for a dynamic type.
///
/// All dynamic types report [`OpaqueInfo`] because their
/// structure is determined at runtime, not compile time.
///
/// [`Typed`]: crate::info::Typed
/// [`OpaqueInfo`]: crate::info::OpaqueInfo
macro_rules! impl_dynamic_type_info {
    ($ty:ty) => {
        impl crate::info::Typed for $ty {
            #[inline]
            fn type_info() -> &'static $crate::info::TypeInfo {
                use $crate::info::{OpaqueInfo, TypeInfo};
                static INFO: TypeInfo = TypeInfo::Opaque(OpaqueInfo::dynamic::<$ty>());
                &INFO
            }
        }
    };
}

/// Implements the four reflection kind dispatch methods for a dynamic type:
///
/// - [`reflect_assign`]
/// - [`reflect_kind`]
/// - [`reflect_ref`]
/// - [`reflect_mut`]
/// - [`reflect_owned`].
///
/// [`reflect_assign`]: crate::Reflect::reflect_assign
/// [`reflect_kind`]: crate::Reflect::reflect_kind
/// [`reflect_ref`]: crate::Reflect::reflect_ref
/// [`reflect_mut`]: crate::Reflect::reflect_mut
/// [`reflect_owned`]: crate::Reflect::reflect_owned
macro_rules! impl_dynamic_reflect_cast {
    ($kind:ident) => {
        #[inline]
        fn is_dynamic(&self) -> bool {
            true
        }

        #[inline]
        fn reflect_assign(&mut self, value: Box<dyn Reflect>) -> Result<(), Box<dyn Reflect>> {
            *self = *value.downcast::<Self>()?;
            Ok(()) // ↑ Faster than default implementation.
        }

        #[inline]
        fn reflect_kind(&self) -> crate::info::ReflectKind {
            crate::info::ReflectKind::$kind
        }

        #[inline]
        fn reflect_ref(&self) -> crate::ops::ReflectRef<'_> {
            crate::ops::ReflectRef::$kind(self)
        }

        #[inline]
        fn reflect_mut(&mut self) -> crate::ops::ReflectMut<'_> {
            crate::ops::ReflectMut::$kind(self)
        }

        #[inline]
        fn reflect_owned(self: Box<Self>) -> crate::ops::ReflectOwned {
            crate::ops::ReflectOwned::$kind(self)
        }
    };
}

use impl_dynamic_reflect_cast;
use impl_dynamic_type_info;
use impl_dynamic_type_path;
