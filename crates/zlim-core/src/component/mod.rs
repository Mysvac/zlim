//! Component types, registration, and metadata.
//!
//! A component is any plain data type annotated with `#[derive(Component)]`.
//!
//! ```rust, no_run
//! # use zlim_core::prelude::*;
//! #
//! #[derive(TypePath, Component, Clone)]
//! struct Position { x: f32, y: f32 }
//! ```
//!
//! See [crate::table] module documents for storage details.
//!
//! zlim-core collects some runtime type information of all components,
//! and stores them within a [`ComponentDB`].
//!
//! ```rust
//! # use zlim_core::prelude::*;
//! #
//! #[derive(TypePath, Component, Clone)]
//! struct Position { x: f32, y: f32 }
//!
//! // Look up (and lazily register) the component's metadata:
//! let db = ComponentDB::of::<Position>();
//! assert_eq!(db.type_name, "Position");
//! ```
//!
//! Non-generic components implemented via macros are automatically collected
//! through [`ComponentDB::collect`], but generic components may require explicit
//! registration using the [`register_component!`] macro, or they can be
//! automatically registered upon first use.
//!
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
