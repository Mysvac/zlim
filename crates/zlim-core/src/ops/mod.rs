//! World and entity operation methods.
//!
//! This module layers the operational API on top of the ECS core: spawning,
//! manipulating and despawning entities, running systems and schedules,
//! sending messages, and managing resources.
//!
//! # Entity handles
//!
//! Four handle types provide access to a single entity, each with a
//! different level of capability:
//!
//! ## [`EntityOwned`]
//!
//! `EntityOwned` is the **owning** handle: it lets you copy the entity,
//! despawn it, and insert or remove components. Because of component
//! lifecycle hooks, any of these operations may cause the entity this
//! handle points to to be despawned; in that case the affected functions
//! return an error.
//!
//! ## [`Entity`]
//!
//! `Entity` is a **data-mutable-only** handle. It can read and write
//! component data and traverse the entity's hierarchy, but it cannot
//! despawn the entity or add/remove components. This guarantees the handle
//! always stays valid.
//!
//! ## [`EntityMut`]
//!
//! `EntityMut` is a **component view** of a single entity. It can modify the
//! entity's own component data, but cannot add/remove components or access
//! the entity hierarchy. It is primarily used as a `Query` parameter, so
//! these restrictions are required to keep data access sound.
//!
//! ## [`EntityRef`]
//!
//! `EntityRef` is a **read-only view** of a single entity. It can read
//! component data (with change detection), but cannot modify it,
//! add/remove components, or access the entity hierarchy. As a `Query`
//! parameter it represents shared access to all of the entity's components.

mod command;
mod entity;
mod job;
mod message;
mod non_send;
mod query;
mod resource;
mod schedule;
mod system;
mod time;
mod world;

pub use entity::{Entity, EntityMut, EntityOwned, EntityRef};
pub use entity::{FetchComponents, GetComponents};
