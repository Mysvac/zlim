//! Deferred command system for ECS world mutations.
//!
//! # Overview
//!
//! The command module provides a deferred mutation framework for the [`World`].
//! Instead of mutating the world directly, systems enqueue [`Command`]s that are
//! applied later (typically at the end of the schedule), outside active system
//! execution.  This avoids borrow-checker conflicts with in-flight queries and
//! supports structural changes — entity spawn/despawn, resource insert/remove —
//! that would otherwise be disallowed.
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`Command`] | A deferred world mutation; closures and function helpers implement it |
//! | [`EntityCommand`] | An entity-scoped command; wraps a mutation on a known entity ID |
//! | [`CommandQueue`] | Type-erased, contiguous buffer for queuing and batch-executing commands |
//!
//! # Function helpers
//!
//! The module exports convenience functions that each return a [`Command`]:
//!
//! | Function | Effect |
//! |----------|--------|
//! | [`spawn_empty`] | Spawn an entity with no components |
//! | [`spawn`] | Spawn an entity from a [`Bundle`] |
//! | [`despawn`] | Despawn an entity (warn on missing) |
//! | [`init_resource`] | Insert a [`Resource`] if it does not already exist |
//! | [`insert_resource`] | Insert or overwrite a [`Resource`] |
//! | [`remove_resource`] | Remove a [`Resource`] |
//!
//! # Impl for closures
//!
//! [`Command`] is blanket-implemented for `FnOnce(&mut World) -> O` (where
//! `O: IntoZlimResult<()>`), and [`EntityCommand`] for
//! `FnOnce(EntityOwned) -> O`.  This means any closure with the right
//! signature can be passed directly to [`CommandQueue::push`].
//!
//! [`World`]: crate::world::World
//! [`Bundle`]: crate::bundle::Bundle
//! [`Resource`]: crate::resource::Resource

mod command;
mod function;
mod queue;

pub use command::{Command, EntityCommand};
pub use function::*;
pub use queue::CommandQueue;
