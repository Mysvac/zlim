//! Bundles — composite types for spawning entities with multiple components.
//!
//! A bundle is a collection of components (and sub-bundles) that can be
//! written to storage in a single operation.  When you spawn an entity,
//! you provide a bundle, and the ECS writes every component inside it to
//! the entity's table row.
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
//! ```rust
//! use zlim_core::prelude::*;
//! use zlim_reflect::derive::TypePath;
//!
//! #[derive(TypePath, Component, Clone, Debug, PartialEq)]
//! struct Position { x: f32, y: f32 }
//!
//! #[derive(TypePath, Component, Clone, Debug, PartialEq)]
//! struct Velocity { dx: f32, dy: f32 }
//!
//! let mut world = World::alloc();
//! let entity = world.spawn(
//!     (Position { x: 0.0, y: 0.0 }, Velocity { dx: 1.0, dy: 0.0 }),
//!     None,
//! );
//! assert_eq!(entity.get::<Position>(), Some(&Position { x: 0.0, y: 0.0 }));
//! assert_eq!(entity.get::<Velocity>(), Some(&Velocity { dx: 1.0, dy: 0.0 }));
//! ```
//!
//! For larger bundles or reusable spawn patterns, use
//! `#[derive(Bundle)]` on a struct.
//!
//! # Derive Macro
//!
//! The recommended way to define a bundle is via `#[derive(Bundle)]`:
//!
//! ```rust
//! use zlim_core::prelude::*;
//! use zlim_reflect::derive::TypePath;
//!
//! #[derive(TypePath, Component, Clone, Debug, PartialEq)]
//! struct Position { x: f32, y: f32 }
//!
//! #[derive(TypePath, Component, Clone, Debug, PartialEq)]
//! struct Velocity { dx: f32, dy: f32 }
//!
//! #[derive(TypePath, Component, Clone, Debug, PartialEq)]
//! struct Health(u32);
//!
//! #[derive(Bundle)]
//! struct PlayerBundle {
//!     position: Position,
//!     velocity: Velocity,
//!     health: Health,
//! }
//!
//! let mut world = World::alloc();
//! let entity = world.spawn(
//!     PlayerBundle {
//!         position: Position { x: 0.0, y: 0.0 },
//!         velocity: Velocity { dx: 1.0, dy: 0.0 },
//!         health: Health(100),
//!     },
//!     None,
//! );
//! assert_eq!(entity.get::<Position>(), Some(&Position { x: 0.0, y: 0.0 }));
//! assert_eq!(entity.get::<Velocity>(), Some(&Velocity { dx: 1.0, dy: 0.0 }));
//! assert_eq!(entity.get::<Health>(), Some(&Health(100)));
//! ```
//!
//! This generates an `unsafe impl Bundle` that collects, writes, and
//! applies effects for every field in declaration order.  For pure-data
//! bundles, add `#[bundle(no_effect)]` — this also emits an
//! `unsafe impl DataBundle` and sets [`Bundle::NEED_APPLY_EFFECT`] to
//! `false`.
//!
//! [`Component`]: crate::component::Component

// -----------------------------------------------------------------------------
// Modules
// -----------------------------------------------------------------------------

mod bundle;
mod info;

// -----------------------------------------------------------------------------
// Exports
// -----------------------------------------------------------------------------

pub use bundle::{Bundle, DataBundle};
pub use info::{BundleId, BundleInfo, Bundles};

pub use zlim_core_derive::Bundle;
