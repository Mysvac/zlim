//! Component types, registration, and metadata.
//!
//! This module owns the core component primitives:
//!
//! - [`Component`] — the core trait for all component types.
//! - [`ComponentDB`] — static per-type metadata in the global registry.
//! - [`Components`] — a local snapshot of the global registry.
//! - [`ComponentId`] — a unique identifier for a component type.
//! - [`Required`] / [`RequiredComponents`] — required-component support.
//!
//! A component is any plain data type annotated with
//! `#[derive(Component)]`. Component types are registered lazily (or in bulk
//! through [`register_component!`]) into a process-global registry of
//! [`ComponentDB`] entries, which each [`World`] snapshots into a local
//! [`Components`] for fast, lock-free lookups.
//!
//! # Example
//!
//! ```rust
//! use zlim_core::prelude::*;
//! use zlim_reflect::derive::TypePath;
//!
//! #[derive(TypePath, Component, Clone)]
//! struct Position {
//!     x: f32,
//!     y: f32,
//! }
//!
//! // Look up (and lazily register) the component's metadata:
//! let db = ComponentDB::of::<Position>();
//! assert_eq!(db.type_name, "Position");
//! ```
//!
//! # Submodules
//!
//! - `alias` — type-erased function pointer aliases.
//! - `collect` — bulk (CTOR-driven) registration.
//! - `collector` — the [`ComponentCollector`] used during bundle collection.
//! - `db` — [`ComponentDB`] and the global lookup registries.
//! - `hook` — lifecycle [`hooks`](ComponentHook) and their [`context`](HookContext).
//! - `id` — [`ComponentId`].
//! - `register` — registration entry points.
//! - `required` — required-component support.
//! - `snapshot` — the [`Components`] snapshot.
//! - `writer` — the [`ComponentWriter`] used during bundle writes.
//!
//! [`World`]: crate::world::World
//! [`register_component!`]: crate::register_component

// -----------------------------------------------------------------------------
// ID

crate::utils::define_ident!(
    /// A unique identifier for a [`Component`] type.
    ///
    /// Component IDs are assigned sequentially at registration time and are
    /// **shared by all worlds** — the same type always maps to the same
    /// `ComponentId` in every [`World`].  Obtain one from a [`ComponentDB`],
    /// e.g. [`ComponentDB::of::<T>()`](ComponentDB::of).id.
    ///
    /// The ID is niche-optimized over a `NonMaxU32`, so
    /// `Option<ComponentId>` has no size overhead. It supports `Copy`,
    /// `Eq`, `Ord`, `Hash`, `Debug`, and `Display`.
    ///
    /// [`World`]: crate::world::World
    ComponentId
);

// -----------------------------------------------------------------------------
// Modules
// -----------------------------------------------------------------------------

mod alias;

mod collect;
mod collector;
mod component;
mod db;
mod hook;
mod register;
mod required;
mod snapshot;
mod writer;

// -----------------------------------------------------------------------------
// Exports
// -----------------------------------------------------------------------------

#[doc(hidden)]
pub use collect::__internal__;

pub use collector::ComponentCollector;
pub use component::Component;
pub use db::ComponentDB;
pub use hook::{ComponentHook, HookContext};
pub use register::{register_base, register_serializable};
pub use required::{Required, RequiredComponents};
pub use snapshot::Components;
pub use writer::ComponentWriter;

pub use zlim_core_derive::Component;
