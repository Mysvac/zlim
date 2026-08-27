//! [`World`]-scoped operations.
//!
//! Implementation modules for the [`World`] and [`DeferredWorld`] methods:
//! spawning, despawning, command application, bundle registration, and entity
//! cloning.
//!
//! [`World`]: crate::world::World
//! [`DeferredWorld`]: crate::world::DeferredWorld

mod bundle;
mod cloner;
mod despawn;
mod empty;
mod entity;
mod forget;
mod spawn;
mod uninit;

pub(crate) use despawn::despawn_internal;
