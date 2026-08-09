//! Hash primitives and container aliases.
//!
//! This module re-exports `hashbrown` / `foldhash` and provides crate-level
//! hash builders plus map/set aliases for common usage patterns.

// -----------------------------------------------------------------------------
// Modules

pub mod hasher;
pub mod map;
pub mod set;
pub mod table;

// -----------------------------------------------------------------------------
// Exports

pub use hashbrown::Equivalent;
pub use hasher::{FixedState, NoopState, SparseState};
pub use map::HashMap;
pub use set::HashSet;
pub use table::HashTable;

// -----------------------------------------------------------------------------
// Re-export crates

pub use foldhash;
pub use hashbrown;
