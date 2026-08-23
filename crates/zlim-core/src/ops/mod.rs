//! World and entity operation methods.
//!
//! This module groups the concrete mutation and query methods implemented on
//! [`World`] and on the entity handles ([`Entity`], [`EntityRef`],
//! [`EntityMut`], [`EntityOwned`]), together with the component-access traits
//! [`FetchComponents`] and [`GetComponents`].
//!
//! Together these cover the full entity lifecycle:
//!
//! - **Spawning** — [`World::spawn`], [`World::spawn_at`],
//!   [`World::spawn_batch`], and [`World::spawn_empty`].
//! - **Despawning** — [`World::despawn`] and [`EntityOwned::despawn`], both
//!   recursively despawning descendants.
//! - **Structural mutation** — inserting, removing, and clearing components
//!   through [`EntityOwned`].
//! - **Hierarchy** — parent/children traversal on [`Entity`], plus child
//!   spawning and reparenting on [`EntityOwned`].
//!
//! [`World`]: crate::world::World
//! [`World::spawn`]: crate::world::World::spawn
//! [`World::spawn_at`]: crate::world::World::spawn_at
//! [`World::spawn_batch`]: crate::world::World::spawn_batch
//! [`World::spawn_empty`]: crate::world::World::spawn_empty
//! [`World::despawn`]: crate::world::World::despawn
//! [`EntityOwned::despawn`]: crate::ops::EntityOwned::despawn

mod command;
mod entity;
mod job;
mod message;
mod query;
mod resource;
mod system;
mod world;

pub use entity::{Entity, EntityMut, EntityOwned, EntityRef};
pub use entity::{FetchComponents, GetComponents};
