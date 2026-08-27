//! Resource types, registration, and storage.
//!
//! A resource is a singleton value identified by its concrete Rust type:
//! at most one value of a given resource type exists in a [`World`] at any
//! time.
//!
//! Usually defined through `#[derive(Resource)]`.
//!
//! ```rust, no_run
//! # use zlim_core::prelude::*;
//! #
//! #[derive(TypePath, Resource)]
//! struct GlobalInstant(std::time::Instant);
//! ```
//!
//! Resource **metadata** (identity, reflection info, layout) lives in the
//! global [`ResourceDB`] registry, while resource **values** are stored
//! in the per-world [`Resources`] storage.
//!
//! ```rust
//! # use zlim_core::prelude::*;
//! #
//! #[derive(TypePath, Resource)]
//! struct GlobalInstant(std::time::Instant);
//!
//! // Look up (and lazily register) the resource's metadata:
//! let db = ResourceDB::of::<GlobalInstant>();
//! assert_eq!(db.type_name, "GlobalInstant");
//! ```
//!
//! # Examples
//!
//! ```rust
//! use zlim_core::prelude::*;
//!
//! // Any `TypePath` type becomes a resource with the derive macro.
//! #[derive(TypePath, Resource)]
//! struct Score(u32);
//!
//! let mut world = World::alloc();
//! world.insert_resource(Score(100));
//! assert_eq!(world.resource::<Score>().0, 100);
//!
//! // Resource values live in the world's storage:
//! let ty = core::any::TypeId::of::<Score>();
//! assert!(world.resources().get(ty).is_some());
//!
//! // Metadata comes from the global registry:
//! assert_eq!(ResourceDB::of::<Score>().type_name, "Score");
//!
//! // Access through System
//! fn modify_score(mut score: ResMut<Score>) {
//!     score.0 += 1_u32;
//! }
//!
//! world.invoke_once(modify_score, ()).unwrap();
//! assert_eq!(world.resource::<Score>().0, 101);
//! ```
//!
//! [`World`]: crate::world::World

// -----------------------------------------------------------------------------
// Modules
// -----------------------------------------------------------------------------

pub mod alias;
mod collect;
mod db;
mod id;
mod register;
mod resource;
mod storage;

// -----------------------------------------------------------------------------
// Exports
// -----------------------------------------------------------------------------

#[doc(hidden)]
pub use collect::__internal__;
pub use db::ResourceDB;
pub use id::ResourceId;
pub use register::{register_base, register_serializable};
pub use resource::Resource;
pub use storage::{ResourceCell, Resources};

pub use zlim_core_derive::Resource;
