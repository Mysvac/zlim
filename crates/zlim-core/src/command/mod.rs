//! Deferred command system for ECS world mutations.
//!
//! # Overview
//!
//! The command module provides a deferred mutation framework for the [`World`].
//! Instead of mutating the world directly, systems enqueue [`Command`]s that are
//! applied later (typically at the end of the schedule, or via [`World::flush`]),
//! outside active system execution.  This avoids borrow-checker conflicts with
//! in-flight queries and supports structural changes — entity spawn/despawn,
//! component insert/remove, resource insert/remove — that would otherwise be
//! disallowed while the world is borrowed.
//!
//! The [`Commands`] and [`EntityCommands`] interfaces are the ergonomic way to
//! queue commands from systems; they wrap a [`CommandQueue`] and offer
//! high-level helpers such as `spawn`, `insert`, and `despawn`.
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`Command`] | A deferred world mutation; closures and function helpers implement it |
//! | [`EntityCommand`] | An entity-scoped command; wraps a mutation on a known entity ID |
//! | [`CommandQueue`] | Type-erased, contiguous buffer for queuing and batch-executing commands |
//! | [`Commands`] | System-parameter interface for queueing [`Command`]s |
//! | [`EntityCommands`] | Entity-scoped [`Commands`] interface |
//!
//! # Function helpers
//!
//! The module exports convenience functions that each return a [`Command`]:
//!
//! | Function | Effect |
//! |----------|--------|
//! | [`spawn_empty`] | Spawn an entity with no components |
//! | [`spawn`] | Spawn an entity from a [`Bundle`] |
//! | [`spawn_batch`] | Spawn many entities from an iterator of [`DataBundle`]s |
//! | [`despawn`] | Despawn an entity (warn on missing) |
//! | [`try_despawn`] | Despawn an entity (no-op if missing) |
//! | [`init_resource`] | Insert a [`Resource`] if it does not already exist |
//! | [`init_non_send`] | Insert a non-`Send` [`Resource`] if it does not exist |
//! | [`insert_resource`] | Insert or overwrite a [`Resource`] |
//! | [`remove_resource`] | Remove a [`Resource`] |
//!
//! And convenience [`EntityCommand`] constructors:
//!
//! | Function | Effect |
//! |----------|--------|
//! | [`insert`] | Insert a [`Bundle`] of components |
//! | [`insert_if_new`] | Insert a [`Bundle`] only if it is missing |
//! | [`remove`] | Remove a [`Bundle`]'s components |
//! | [`clear`] | Remove all components |
//! | [`clone`] | Clone the entity (optionally recursively) |
//!
//! # Impl for closures
//!
//! [`Command`] is blanket-implemented for `FnOnce(&mut World) -> O` (where
//! `O: IntoZlimResult<()>`), and [`EntityCommand`] for
//! `FnOnce(EntityOwned) -> O`.  This means any closure with the right
//! signature can be passed directly to [`CommandQueue::push`] — closures
//! returning `()` directly, and fallible ones after conversion with
//! [`Command::handle_error`].
//!
//! [`World`]: crate::world::World
//! [`World::flush`]: crate::world::World::flush
//! [`Bundle`]: crate::bundle::Bundle
//! [`DataBundle`]: crate::bundle::DataBundle
//! [`Resource`]: crate::resource::Resource
//! [`Command`]: crate::command::Command
//! [`Command::handle_error`]: crate::command::Command::handle_error
//! [`EntityCommand`]: crate::command::EntityCommand
//! [`Commands`]: crate::command::Commands
//! [`EntityCommands`]: crate::command::EntityCommands
//! [`CommandQueue`]: crate::command::CommandQueue
//! [`CommandQueue::push`]: crate::command::CommandQueue::push
//! [`spawn_empty`]: crate::command::spawn_empty
//! [`spawn`]: crate::command::spawn
//! [`spawn_batch`]: crate::command::spawn_batch
//! [`despawn`]: crate::command::despawn
//! [`try_despawn`]: crate::command::try_despawn
//! [`init_resource`]: crate::command::init_resource
//! [`init_non_send`]: crate::command::init_non_send
//! [`insert_resource`]: crate::command::insert_resource
//! [`remove_resource`]: crate::command::remove_resource
//! [`insert`]: crate::command::insert
//! [`insert_if_new`]: crate::command::insert_if_new
//! [`remove`]: crate::command::remove
//! [`clear`]: crate::command::clear
//! [`clone`]: crate::command::clone

mod command;
mod commands;
mod function;
mod queue;

pub use command::{Command, EntityCommand};
pub use commands::{Commands, EntityCommands};
pub use function::*;
pub use queue::CommandQueue;
pub(crate) use queue::RawCommandQueue;
