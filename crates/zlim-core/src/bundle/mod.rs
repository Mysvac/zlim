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
//!
//! - [`DataBundle`] — marker supertrait for bundles that contain only pure
//!   data (no side effects).  All components and tuples of `DataBundle`
//!   implement this automatically.
//!
//! Side Effect allows you to implement some special bundles, such as automatically
//! spawn some children entity .
//!
//! # Tuple Bundles
//!
//! Tuples up to arity 12 implement [`Bundle`] (and [`DataBundle`] where
//! applicable).  This lets you write inline spawn calls without defining a
//! struct:
//!
//! ```rust
//! use zlim_core::prelude::*;
//!
//! #[derive(TypePath, Component, Clone, Debug, PartialEq)]
//! struct Position { x: f32, y: f32 }
//!
//! #[derive(TypePath, Component, Clone, Debug, PartialEq)]
//! struct Velocity { dx: f32, dy: f32 }
//!
//! let mut world = World::alloc();
//!
//! let bundle = (Position { x: 0.0, y: 0.0 }, Velocity { dx: 1.0, dy: 0.0 });
//!
//! let entity = world.spawn(bundle, None); // None: parent is none
//!
//! assert_eq!(entity.get::<Position>(), Some(&Position { x: 0.0, y: 0.0 }));
//! assert_eq!(entity.get::<Velocity>(), Some(&Velocity { dx: 1.0, dy: 0.0 }));
//! ```
//!
//! # Derive Macro
//!
//! The recommended way to define a bundle is via `#[derive(Bundle)]`:
//!
//! ```rust, no_run
//! use zlim_core::prelude::*;
//!
//! #[derive(TypePath, Component, Clone, Debug, PartialEq)]
//! struct Position { x: f32, y: f32 }
//!
//! #[derive(TypePath, Component, Clone, Debug, PartialEq)]
//! struct Velocity { dx: f32, dy: f32 }
//!
//! #[derive(Bundle)]
//! #[bundle(data)] // derive DataBundle
//! struct MovableBundle {
//!     position: Position,
//!     velocity: Velocity,
//! }
//!
//! let mut world = World::alloc();
//!
//! let bundle = MovableBundle {
//!     position: Position { x: 0.0, y: 0.0 },
//!     velocity: Velocity { dx: 1.0, dy: 0.0 },
//! };
//! let entity = world.spawn(bundle, None);
//!
//! assert_eq!(entity.get::<Position>(), Some(&Position { x: 0.0, y: 0.0 }));
//! assert_eq!(entity.get::<Velocity>(), Some(&Velocity { dx: 1.0, dy: 0.0 }));
//! ```
//!
//! This generates an `unsafe impl Bundle` that collects, writes, and
//! applies effects for every field in declaration order.
//!
//! `NEED_APPLY_EFFECT` is the logical OR of the field types' flags.
//!
//! For pure-data bundles, add `#[bundle(data)]` — this also emits an
//! `unsafe impl DataBundle`, which requires `NEED_APPLY_EFFECT` to be
//! `false`.
//!
//! > Duplicate document with Bundle Trait.
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
