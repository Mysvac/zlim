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
//! Every [`Component`] is itself a [`Bundle`], so you can pass individual
//! components directly to spawn functions.
//!
//! # Traits
//!
//! - [`Bundle`] — the core trait: collects the bundle's components and writes
//!   their data into an entity's row.
//!
//! A bundle carries data only.  Writing it is the whole operation — the entity
//! it was written to is never handed back to the bundle afterwards.
//!
//! # Tuple Bundles
//!
//! Tuples up to arity 12 implement [`Bundle`].  This lets you write inline
//! spawn calls without defining a struct:
//!
//! ```rust
//! use zlim_core::prelude::*;
//!
//! #[derive(Component, Clone, Debug, PartialEq)]
//! struct Position { x: f32, y: f32 }
//!
//! #[derive(Component, Clone, Debug, PartialEq)]
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
//! #[derive(Component, Clone, Debug, PartialEq)]
//! struct Position { x: f32, y: f32 }
//!
//! #[derive(Component, Clone, Debug, PartialEq)]
//! struct Velocity { dx: f32, dy: f32 }
//!
//! #[derive(Bundle)]
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
//! This generates an `unsafe impl Bundle` that collects and writes every field
//! in declaration order, flattening fields that are themselves bundles.
//!
//! > Duplicate document with Bundle Trait.
//!
//! # Dynamic Insertion
//!
//! A bundle names its components when the code is written.  When the component
//! set is only known at runtime — while walking a scene description, for
//! example — stage the components in a [`BundleScratch`] instead and insert
//! them with the [`BundleWriter`] it hands out:
//!
//! ```rust
//! use zlim_core::prelude::*;
//! use zlim_core::bundle::BundleScratch;
//!
//! #[derive(Component, Clone, PartialEq, Debug)]
//! struct Position { x: f32, y: f32 }
//!
//! let mut world = World::alloc();
//! let mut entity = world.spawn((), None);
//!
//! let mut scratch = BundleScratch::default();
//! let mut writer = scratch.writer();
//!
//! writer.push(Position { x: 1.0, y: 2.0 }, None);
//! writer.write(&mut entity).unwrap();
//!
//! assert_eq!(entity.get::<Position>(), Some(&Position { x: 1.0, y: 2.0 }));
//! ```
//!
//! That path is the dynamic counterpart of [`EntityOwned::insert`], and unlike
//! [`World::spawn`] it inserts into an existing entity rather than creating one.
//! It stages every component in the scratch space before the bundle reaches
//! storage (one extra copy).
//!
//! [`EntityOwned::insert`]: crate::ops::EntityOwned::insert
//! [`World::spawn`]: crate::world::World::spawn
//! [`Component`]: crate::component::Component

// -----------------------------------------------------------------------------
// Modules
// -----------------------------------------------------------------------------

mod bundle;
mod info;
mod writer;

// -----------------------------------------------------------------------------
// Exports
// -----------------------------------------------------------------------------

pub use bundle::Bundle;
pub use info::{BundleId, BundleInfo, Bundles};
pub use writer::{BundleScratch, BundleWriter};

pub use crate::derive::Bundle;
