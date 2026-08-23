//! Errors produced by entity-targeted and single-target query APIs.

use zlim_core_derive::Error;

use crate::entity::EntityId;

/// Errors produced by entity-targeted query APIs like `get` and
/// `get_many_mut`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[zlim_error(warning)]
pub enum QueryEntityError {
    /// The entity does not currently exist in the world (despawned, stale,
    /// or invalid).
    #[error("Entity {_0} does not exist in this world")]
    NoSuchEntity(EntityId),

    /// The entity exists, but does not match query data/filter constraints.
    #[error("Entity {_0} does not satisfy this query")]
    QueryMismatch(EntityId),

    /// Duplicate entity in mutable many-access APIs.
    #[error("Entity {_0} appears more than once in mutable many-query access")]
    DuplicateEntity(EntityId),
}

/// Errors produced by single-target query APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[zlim_error(warning)]
pub enum QuerySingleError {
    /// No entity matched the query.
    #[error("No entities match this query")]
    NoEntities,

    /// More than one entity matched the query.
    #[error("More than one entity matches this query")]
    MultipleEntities,
}

// -----------------------------------------------------------------------------
