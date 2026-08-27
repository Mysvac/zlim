//! Dense columnar storage for ECS components.
//!
//! # Architecture
//!
//! The table module implements Bevy-style archetype-based storage:
//!
//! Every table belongs to one *archetype*: a fixed set of component types.
//! All entities in a table share exactly that component set, which is what
//! makes the storage dense — each column stores one component type for every
//! row, with no per-entity type tags or gaps.
//!
//! |           | Component A | Component B | Component C | .. |
//! |-----------|-------------|-------------|-------------|----|
//! | Entity A  | /* data */  | /* data */  | /* data */  | .. |
//! | Entity B  | /* data */  | /* data */  | /* data */  | .. |
//! | Entity C  | /* data */  | /* data */  | /* data */  | .. |
//! | ........  | ..........  | ..........  | ..........  | .. |
//!
//! # Entity movement
//!
//! When an entity gains or loses components (via `insert` / `remove`), it
//! moves from one table to another.  [`Table::move_row`] handles the
//! low-level column swapping, and [`MovedEntityRow`] informs the entity tree
//! of any index changes in the source table.
//!
//! # Example
//!
//! ```rust
//! use zlim_core::prelude::*;
//! use zlim_core::table::Tables;
//!
//! #[derive(TypePath, Component, Clone)]
//! struct Position {
//!     x: f32,
//!     y: f32,
//! }
//!
//! let mut world = World::alloc();
//!
//! // Entities with the same component set share one table (archetype).
//! world.spawn((Position { x: 0.0, y: 0.0 }), None);
//! world.spawn((Position { x: 1.0, y: 2.0 }), None);
//!
//! // The world's `Tables` registry owns every table, including the always-
//! // present empty table for entities without components.
//! let tables: &Tables = world.tables();
//!
//! // Two spawns of the same archetype share one table, so the registry
//! // holds exactly two tables: the empty one plus the `Position` table.
//! assert_eq!(tables.len(), 2);
//!
//! for table in tables.iter() {
//!     // Each row is an entity, each column is a component type.
//!     for &entity in table.entities() {
//!         // ...
//!     }
//! }
//! ```
//!
//! [`Table::move_row`]: Table::move_row
//! [`MovedEntityRow`]: ident::MovedEntityRow

mod blob_array;
mod column;
mod ident;
mod table;
mod tables;
mod tick_array;

pub use column::Column;
pub use ident::MovedEntityRow;
pub use ident::TableId;
pub use ident::{TableCol, TableRow};
pub use table::Table;
pub use tables::Tables;
