//! Query system: fetching data from entities with filters.
//!
//! A query is composed of two halves:
//!
//! - [`QueryData`] — what to fetch per entity.
//! - [`QueryFilter`] — which entities match.
//!
//! ```ignore
//! for name in world.query::<Name, With<Health>>() {
//!     std::println!("{name}");
//! }
//! ```
//!
//! # QueryData — available data parameters
//!
//! The following types can be used as query data (implement [`QueryData`]):
//!
//! | Parameter | Description |
//! |---|---|
//! | `EntityId` | The entity's own ID |
//! | `&T` / `Option<&T>` | Shared references to component `T` |
//! | `&mut T` / `Option<&mut T>` | Exclusive references to component `T`; iterating/fetching yields [`Mut<T>`](crate::borrow::Mut), so change ticks are preserved |
//! | `Ref<T>` / `Option<Ref<T>>` | Shared references with change detection |
//! | `Mut<T>` / `Option<Mut<T>>` | Exclusive references with change detection |
//! | [`Parent`] | The entity's parent (`Option<EntityId>`, `None` when root) |
//! | [`Children`] | The entity's direct children (`&[EntityId]`, ordered by insertion) |
//! | `EntityRef` / `EntityMut` | A handle to the whole entity — shared / exclusive access to all of its components |
//! | `(A, B, ...)` | Tuples combining 0–12 items, e.g. `(&Position, &mut Velocity, Parent)` |
//! | Custom `#[derive(QueryData)]` structs | Derived from the forms above (supports `#[query_data(readonly)]` and `#[query_data(query_slice(...))]`) |
//!
//! # QueryFilter — available filters
//!
//! The following filters are available (implement [`QueryFilter`]):
//!
//! | Filter | Description |
//! |--------|-------------|
//! | `()` | No filtering (the default) |
//! | `With<T>` / `With<(A, B, ...)>` | Entity must contain component `T` / all of `A, B, ...` (table-level, no per-entity cost) |
//! | `Without<T>` / `Without<(A, B, ...)>` | Entity must lack component `T` / all of `A, B, ...` (table-level) |
//! | `Changed<T>` | Component `T` changed in the interval `(last_run, this_run]` (entity-level) |
//! | `Added<T>` | Component `T` added in the interval `(last_run, this_run]` (entity-level) |
//! | `And<(F1, F2, ...)>` | Logical AND — every inner filter must match |
//! | `Or<(F1, F2, ...)>` | Logical OR — at least one inner filter must match |
//! | Custom [`QueryFilter`] impls | Table-level prefiltering (`register_filter`) plus per-entity checks (`filter`) |
//!
//! # Types
//!
//! - [`Query`] is the system parameter used inside systems;
//!
//! - [`QueryState`] holds the reusable per-query cache;
//!
//! - [`QueryIter`] and [`QuerySliceIter`] perform the actual
//!   iteration — row-by-row and as whole-table slices respectively;
//!
//! - [`Single`] requires exactly one match.
//!
//! # Examples
//!
//! ```rust
//! use zlim_core::prelude::*;
//! use zlim_reflect::derive::TypePath;
//!
//! #[derive(TypePath, Component, Clone)]
//! struct Position { x: f32, y: f32 }
//!
//! #[derive(TypePath, Component, Clone)]
//! struct Player;
//!
//! // Iterate every `Position`, optionally restricted by a filter.
//! fn total_x(query: Query<&Position, With<Player>>) -> f32 {
//!     query.iter().map(|p| p.x).sum()
//! }
//!
//! let mut world = World::alloc();
//! world.spawn((Position { x: 1.0, y: 2.0 }, Player), None);
//! world.spawn(Position { x: 10.0, y: 0.0 }, None);
//!
//! // Only the entity carrying `Player` contributes to the total.
//! assert_eq!(world.invoke_once(total_x, ()).unwrap(), 1.0);
//! ```

mod cache;
mod data;
mod error;
mod filter;
mod iter;
mod query;
mod single;
mod state;

pub use cache::QueryCache;
pub use data::{Children, Parent, QueryData, QuerySlice, ReadOnlyQueryData};
pub use error::{QueryEntityError, QuerySingleError};
pub use filter::{Added, And, ArchetypeFilter, Changed, Or, QueryFilter, With, Without};
pub use iter::{QueryIter, QuerySliceIter};
pub use query::Query;
pub use single::Single;
pub use state::QueryState;
