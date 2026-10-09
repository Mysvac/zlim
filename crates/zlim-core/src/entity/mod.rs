//! Entity identifiers, allocation, mapping, and the entity tree.
//!
//! # EntityId
//!
//! [`EntityId`] is a unique identifier for an entity, composed of a 32-bit
//! index and a 32-bit generation, stored as one `NonZeroU64`.
//!
//! ```text
//! EntityId(NonZeroU64)   // low 32 bits: index, high 32 bits: generation
//! ```
//!
//! The index names the slot the entity occupies, while the generation
//! distinguishes between successive occupants of that slot. `NonZero` is used
//! for niche optimization.
//!
//! Only the bit pattern is guaranteed non-zero; the generation of an id taken
//! from a live entity is non-zero as well, because an allocator never hands out
//! generation `0`. A scene remaps the ids it reads from a document, so a
//! deserialized id is only held to the first of those — see the type-level note
//! on [what is guaranteed](EntityId#what-is-guaranteed).
//!
//! ```text
//! Entities [
//!     index(0): { generation(..), other_data },
//!     index(1): { generation(..), other_data },
//!     ..
//! ]
//! ```
//!
//! # Entities
//!
//! [`Entities`] maintain an entity tree.
//!
//! Unlike the standard ECS architecture, the implementation of this crate
//! comes with hierarchical relationships inherent in the entities storage.
//!
//! ```text
//! Entities [
//!     index(0): { generation(..), parent, children, .. },
//!     index(1): { generation(..), parent, children, },
//!     ..
//! ]
//! ```
//!
//! This does not affect archetype query efficiency, as the table storage
//! provides a separate entity column that ensures cache locality during queries.
//!
//! # Others
//!
//! - [`EntityAllocator`] / [`RemoteAllocator`] — lock-free ID allocation,
//!   with [`AllocEntitiesIter`] for batched allocation.
//!
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
mod label;
mod mapper;

pub use allocator::AllocEntitiesIter;
pub use allocator::{EntityAllocator, RemoteAllocator};
pub use entities::RootEntities;
pub use entities::{Entities, EntityError, EntityNode};
pub use id::{EntityId, Location};
pub use label::{EntityLabel, InternedEntityLabel};
pub use mapper::{EntityMap, EntityMapper, MapEntities};

pub use crate::derive::EntityLabel;
