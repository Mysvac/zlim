//! Query system: fetching data from entities with filters.
//!
//! A query is composed of two halves:
//!
//! - [`QueryData`] — what to fetch per entity (`&T`, `&mut T`, `Ref<T>`,
//!   `Mut<T>`, `EntityId`, tuples, ...).
//! - [`QueryFilter`] — which entities match (`With`, `Without`, `Added`,
//!   `Changed`, `And`, `Or`, ...).
//!
//! [`Query`] is the system parameter used inside systems; [`QueryState`]
//! holds the reusable per-query cache; [`QueryIter`] and [`QuerySliceIter`]
//! perform the actual iteration — row-by-row and as whole-table slices
//! respectively; [`Single`] requires exactly one match.
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
//! assert_eq!(total_x(world.query::<&Position, With<Player>>()), 1.0);
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
pub use data::{QueryData, QuerySlice, ReadOnlyQueryData};
pub use error::{QueryEntityError, QuerySingleError};
pub use filter::{Added, And, ArchetypeFilter, Changed, Or, QueryFilter, With, Without};
pub use iter::{QueryIter, QuerySliceIter};
pub use query::Query;
pub use single::Single;
pub use state::QueryState;
