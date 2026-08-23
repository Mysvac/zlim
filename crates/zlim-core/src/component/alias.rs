//! Type-erased function pointer aliases used by [`ComponentDB`].
//!
//! These allow the component database to store type-independent function
//! pointers for field reflection, entity mapping, serialization, and
//! deserialization.
//!
//! [`ComponentDB`]: crate::component::ComponentDB

use erased_serde::{Deserializer, Error, Serialize};
use zlim_ptr::{OwningPtr, Ptr, PtrMut};
use zlim_reflect::Reflect;
use zlim_utils::mem::Bump;

use crate::entity::EntityMapper;

// -----------------------------------------------------------------------------
// Aliases
// -----------------------------------------------------------------------------

/// Type-erased function for reading a field by name via reflection.
///
/// Returns `Some(&dyn Reflect)` on success, `None` if the field does not exist.
pub type GetFieldFunc = for<'a> unsafe fn(Ptr<'a>, &str) -> Option<&'a dyn Reflect>;

/// Type-erased function for writing a field by name via reflection.
///
/// Returns `Err(message)` if the field does not exist or the value cannot
/// be applied.
pub type SetFieldFunc = for<'a> unsafe fn(PtrMut<'a>, &str, &dyn Reflect) -> Result<(), String>;

/// Type-erased function that remaps entity references within a component instance.
pub type MapEntitiesFunc = unsafe fn(PtrMut<'_>, &mut dyn EntityMapper);

/// Type-erased function that returns an `erased_serde::Serialize` reference
/// from a component pointer.
pub type SerializeFunc = for<'a> fn(Ptr<'a>) -> &'a dyn Serialize;

/// Type-erased function that deserializes a component from an `erased_serde`
/// deserializer, allocating through a [`Bump`].
pub type DeserializeFunc =
    for<'a, 'b> fn(&'a mut dyn Deserializer, &'b Bump) -> Result<OwningPtr<'b>, Error>;

// -----------------------------------------------------------------------------
