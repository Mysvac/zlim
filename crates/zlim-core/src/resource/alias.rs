//! Type aliases shared across the resource subsystem.

use erased_serde::{Deserializer, Error, Serialize};
use zlim_ptr::{OwningPtr, Ptr, PtrMut};
use zlim_reflect::Reflect;
use zlim_utils::mem::Bump;

// -----------------------------------------------------------------------------
// Aliases
// -----------------------------------------------------------------------------

/// Type-erased function pointer for reading a reflected field.
///
/// Given a type-erased [`Ptr`] and a field name, returns a reflected
/// reference to that field if it exists. Used internally by the editor
/// and serialization systems to inspect resource data without knowing
/// the concrete type.
pub type GetFieldFunc = for<'a> unsafe fn(Ptr<'a>, &str) -> Option<&'a dyn Reflect>;

/// Type-erased function pointer for writing a reflected field.
///
/// Given a type-erased [`PtrMut`], a field name, and a reflected value,
/// assigns the value to the field. Used internally by the editor and
/// serialization systems to mutate resource data without knowing the
/// concrete type.
pub type SetFieldFunc = for<'a> unsafe fn(PtrMut<'a>, &str, &dyn Reflect) -> Result<(), String>;

/// Type-erased function that returns an `erased_serde::Serialize` reference
/// from a resource pointer.
pub type SerializeFunc = for<'a> fn(Ptr<'a>) -> &'a dyn Serialize;

/// Type-erased function that deserializes a resource from an
/// `erased_serde` deserializer, allocating through a [`Bump`].
pub type DeserializeFunc =
    for<'a, 'b> fn(&'a mut dyn Deserializer, &'b Bump) -> Result<OwningPtr<'b>, Error>;
