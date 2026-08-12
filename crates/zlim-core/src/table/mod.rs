//! Dense columnar storage for ECS components.
//!
//! # Architecture
//!
//! The table module implements Bevy-style archetype-based storage:
//!
//! | Type | Role |
//! |------|------|
//! | [`Table`] | A dense columnar store for one archetype (fixed component set).  Each row is an entity, each column is a component type. |
//! | [`Tables`] | Registry of all tables in a world.  Provides O(1) lookup by component-set signature and lazy table-transition caching. |
//! | [`Column`] | Low-level untyped primitive: one `BlobArray` for component bytes + two `TickArray`s for change detection. |
//! | `BlobArray` | Raw heap-allocated byte array with optional drop semantics.  The building block of [`Column`]. |
//! | `TickArray` | Contiguous tick storage for change-detection epochs.  Used by [`Column`] for `added`/`changed` tracking. |
//! | [`TableId`] | Niche-optimised identifier for a [`Table`] within a world. |
//! | [`TableRow`] / [`TableCol`] | Positional indices within a table (row = entity slot, col = component slot). |
//! | [`MovedEntityRow`] | Return value recording the side-effects of entity movement between tables. |
//!
//! # Entity movement
//!
//! When an entity gains or loses components (via `insert` / `remove`), it
//! moves from one table to another.  [`Table::move_row`] handles the
//! low-level column swapping, and [`MovedEntityRow`] informs the entity tree
//! of any index changes in the source table.
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
