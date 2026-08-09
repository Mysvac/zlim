//! Helper functions and trait implementations for common types.
//!
//! This module contains two categories:
//!
//! - **Helper functions** — reusable logic in sub-modules for implementing
//!   `apply`, `eq`, `hash`, and `debug` methods of the reflection ops traits.
//! - **Type implementations** — [`Reflect`] and ops trait impls for Rust
//!   standard-library types and common external types:
//!
//! | Module | Types covered |
//! |--------|---------------|
//! | `primitive` | Primitive and built-in (`i32`, `f64`, `bool`, `char`, `()`, `[T; N]`, tuples, etc.) |
//! | `alloc` | Alloc types (`String`, `Vec<T>`, `BTreeMap<K,V>`, `BTreeSet<T>`, etc.) |
//! | `core` | Core types (`Option<T>`, `Result<T,E>`, `PhantomData<T>`, `Duration`, `TypeId`, etc.) |
//! | `std` | Std-only types (`HashMap<K,V>`, `HashSet<T>`, etc.) |
//! | `features` | External dependency types (`glam::Vec3A`, etc.) |
//!
//! [`Reflect`]: crate::Reflect

use ::core::any::TypeId;

use ::zlim_utils::hash::FixedState;
use ::zlim_utils::hash::hasher::FixedHasher;

use crate::db::TypeDB;
use crate::ops::{CloneError, Reflect};

// -----------------------------------------------------------------------------
// Modules

mod alloc;
mod common;
mod core;
mod features;
mod helper;
mod primitive;
mod std;
mod zlim_utils;

// -----------------------------------------------------------------------------
// Helper

pub use common::*;

/// A Fixed Hasher for [`reflect_hash`] implementation.
///
/// [`reflect_hash`]: crate::Reflect::reflect_hash
#[inline(always)]
pub const fn reflect_hasher() -> FixedHasher {
    FixedState::HASHER
}

#[inline(never)]
pub fn reflect_clone_field<T: Reflect>(field: &T) -> Result<T, CloneError> {
    let cloned = field.reflect_clone()?;
    Ok(cloned.take::<T>().expect(CLONE_TYPE_ERROR))
}

#[inline(never)]
pub fn is_convertable(value: &dyn Reflect, to: TypeId) -> bool {
    let this_id = value.type_id();
    if to == this_id {
        return true;
    }

    match TypeDB::get_by_type(this_id) {
        Some(db) => db.contains_convertor(to),
        None => false,
    }
}

// -----------------------------------------------------------------------------
// Internal Helper

const UNPACK_ERROR: &str = "`unpack` and `drain` must preserve the child element structure.";
const CLONE_TYPE_ERROR: &str = "`reflect_clone` must return a value of the same type";
const COMPATIBLE_ERROR: &str = "`from_reflect` must succeed for any compatible reflected value.";
const CONVERT_TYPE_ERROR: &str =
    "If TypeDB::convert succeeds, it must return a correct type value.";

macro_rules! impl_reflect_kind {
    ($kind:ident) => {
        #[inline]
        fn reflect_assign(
            &mut self,
            value: Box<dyn $crate::ops::Reflect>,
        ) -> Result<(), Box<dyn $crate::ops::Reflect>> {
            *self = *<dyn $crate::Reflect>::downcast::<Self>(value)?;
            Ok(()) // ↑ Faster than default implementation.
        }

        #[inline]
        fn reflect_kind(&self) -> $crate::info::ReflectKind {
            $crate::info::ReflectKind::$kind
        }

        #[inline]
        fn reflect_ref(&self) -> $crate::ops::ReflectRef<'_> {
            $crate::ops::ReflectRef::$kind(self)
        }

        #[inline]
        fn reflect_mut(&mut self) -> $crate::ops::ReflectMut<'_> {
            $crate::ops::ReflectMut::$kind(self)
        }

        #[inline]
        fn reflect_owned(self: ::std::boxed::Box<Self>) -> $crate::ops::ReflectOwned {
            $crate::ops::ReflectOwned::$kind(self)
        }
    };
}

use impl_reflect_kind;

// -----------------------------------------------------------------------------
