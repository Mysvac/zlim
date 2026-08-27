//! Hash primitives and container aliases.

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
