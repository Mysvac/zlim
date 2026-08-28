//! Type aliases shared across the resource subsystem.

use erased_serde::{Deserializer, Error, Serialize};
use zlim_ptr::{OwningPtr, Ptr};
use zlim_utils::mem::Bump;

// -----------------------------------------------------------------------------
// Aliases
// -----------------------------------------------------------------------------

/// Type-erased function that returns an `erased_serde::Serialize` reference
/// from a resource pointer.
pub type SerializeFunc = for<'a> fn(Ptr<'a>) -> &'a dyn Serialize;

/// Type-erased function that deserializes a resource from an
/// `erased_serde` deserializer, allocating through a [`Bump`].
pub type DeserializeFunc =
    for<'a, 'b> fn(&'a mut dyn Deserializer, &'b Bump) -> Result<OwningPtr<'b>, Error>;
