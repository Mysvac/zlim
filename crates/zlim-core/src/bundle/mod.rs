//! Bundles — composite types for spawning entities with multiple components.
//!
//! A bundle is a collection of components (and sub-bundles) that can be
//! written to storage in a single operation.  When you spawn an entity,
//! you provide a bundle, and the ECS writes every component inside it to
//! the entity's archetype row.
//!
//! # Bundle vs. Component
//!
//! | Concept    | Description                                        |
//! |------------|----------------------------------------------------|
//! | [`Bundle`] | A set of components written together at spawn time.|
//! | [`Component`] | A single piece of data stored per entity.       |
//!
//! Every [`Component`] is itself a [`Bundle`] (and a [`DataBundle`]), so
//! you can pass individual components directly to spawn functions.
//!
//! # Traits
//!
//! - [`Bundle`] — the core trait.  Collects, writes, and optionally applies
//!   post-spawn side effects.
//! - [`DataBundle`] — marker supertrait for bundles that contain only pure
//!   data (no side effects).  All components and tuples of `DataBundle`
//!   implement this automatically.
//!
//! # Tuple Bundles
//!
//! Tuples up to arity 12 implement [`Bundle`] (and [`DataBundle`] where
//! applicable).  This lets you write inline spawn calls without defining a
//! struct:
//!
//! ```ignore
//! // Spawn with a tuple bundle
//! world.spawn((Position { x: 0.0, y: 0.0 }, Velocity { dx: 1.0, dy: 0.0 }), /* ... */);
//! ```
//!
//! For larger bundles or reusable spawn patterns, use
//! `#[derive(Bundle)]` on a struct.
//!
//! # Derive Macro
//!
//! The recommended way to define a bundle is via `#[derive(Bundle)]`:
//!
//! ```ignore
//! use zlim_core::prelude::*;
//!
//! #[derive(Bundle)]
//! struct PlayerBundle {
//!     position: Position,
//!     velocity: Velocity,
//!     health: Health,
//!     sprite: Sprite,
//! }
//! ```
//!
//! This generates an `unsafe impl Bundle` (and `unsafe impl DataBundle`)
//! that collects, writes, and applies effects for every field in declaration
//! order.
//!
//! [`Component`]: crate::component::Component

// -----------------------------------------------------------------------------
// Modules
// -----------------------------------------------------------------------------

mod bundle;
mod helper;
mod info;

// -----------------------------------------------------------------------------
// Exports
// -----------------------------------------------------------------------------

pub use bundle::{Bundle, DataBundle};
pub use helper::{ComponentCollector, ComponentWriter};
pub use info::{BundleId, BundleInfo, Bundles};

pub use zlim_core_derive::Bundle;
