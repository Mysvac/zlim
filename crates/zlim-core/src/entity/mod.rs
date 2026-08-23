//! Entity identifiers, allocation, mapping, and the entity tree.
//!
//! This module owns the core entity primitives:
//!
//! - [`EntityId`] — a generation-checked entity handle, plus [`Location`]
//!   describing where a spawned entity's components are stored.
//! - [`EntityAllocator`] / [`RemoteAllocator`] — lock-free ID allocation,
//!   with [`AllocEntitiesIter`] for batched allocation.
//! - [`Entities`] — sparse storage for entity metadata and hierarchy,
//!   backed by [`EntityNode`] and reporting failures as [`EntityError`].
//! - [`EntityMap`] / [`EntityMapper`] / [`MapEntities`] — entity remapping
//!   support for cloning and scene instantiation.
//!
//! [`EntityId`]: crate::entity::EntityId
//! [`Location`]: crate::entity::Location
//! [`EntityAllocator`]: crate::entity::EntityAllocator
//! [`RemoteAllocator`]: crate::entity::RemoteAllocator
//! [`AllocEntitiesIter`]: crate::entity::AllocEntitiesIter
//! [`Entities`]: crate::entity::Entities
//! [`EntityNode`]: crate::entity::EntityNode
//! [`EntityError`]: crate::entity::EntityError
//! [`EntityMap`]: crate::entity::EntityMap
//! [`EntityMapper`]: crate::entity::EntityMapper
//! [`MapEntities`]: crate::entity::MapEntities

mod allocator;
mod entities;
mod id;
mod mapper;

pub use allocator::AllocEntitiesIter;
pub use allocator::{EntityAllocator, RemoteAllocator};
pub use entities::{Entities, EntityError, EntityNode};
pub use id::{EntityId, Location};
pub use mapper::{EntityMap, EntityMapper, MapEntities};
