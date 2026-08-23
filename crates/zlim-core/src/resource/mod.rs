//! Resource types, registration, and metadata.
//!
//! This module owns the core resource primitives:
//!
//! - [`Resource`] — the core trait for singleton resources.
//! - [`ResourceDB`] — static per-type metadata in the global registry.
//! - [`Resources`] — a local snapshot of the global registry.
//! - [`ResourceId`] — a unique identifier for a resource type.
//!
//! A resource is a singleton value identified by its concrete Rust type: at
//! most one value of a given resource type exists in a [`World`] at any
//! time.  Resource **metadata** (identity, reflection info, layout) lives in
//! the global [`ResourceDB`] registry, while resource **values** are stored
//! in the [`slot`] module.
//!
//! # Example
//!
//! ```rust
//! use zlim_core::prelude::*;
//! use zlim_reflect::derive::TypePath;
//!
//! // Any `TypePath` type becomes a resource with the derive macro.
//! #[derive(TypePath, Resource)]
//! struct Score(u32);
//!
//! let mut world = World::alloc();
//! world.insert_resource(Score(100));
//! assert_eq!(world.get_resource::<Score>().unwrap().0, 100);
//!
//! // Metadata for a resource type is available from the world's snapshot.
//! let db = world.resources().get::<Score>();
//! assert_eq!(db.type_name, "Score");
//! ```
//!
//! # Submodules
//!
//! - `alias` — type-erased function pointer aliases.
//! - `collect` — bulk (CTOR-driven) registration.
//! - `db` — [`ResourceDB`] and the global lookup registries.
//! - `id` — [`ResourceId`].
//! - `register` — registration entry points.
//! - `snapshot` — the [`Resources`] snapshot.
//!
//! [`World`]: crate::world::World
//! [`slot`]: crate::slot

// -----------------------------------------------------------------------------
// Modules
// -----------------------------------------------------------------------------

pub mod alias;
mod collect;
mod db;
mod id;
mod register;
mod resource;
mod snapshot;

// -----------------------------------------------------------------------------
// Exports
// -----------------------------------------------------------------------------

#[doc(hidden)]
pub use collect::__internal__;
pub use db::ResourceDB;
pub use id::ResourceId;
pub use register::{register_base, register_serializable};
pub use resource::Resource;
pub use snapshot::Resources;

pub use zlim_core_derive::Resource;
