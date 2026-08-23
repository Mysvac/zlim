//! Graph primitives used by the schedule dependency system.
//!
//! Provides directed/undirected graph containers, topological sort,
//! strongly-connected component detection, and a [`Dag`] wrapper that
//! combines a directed graph with a cached topological order.
//!
//! [`Dag`]: crate::schedule::Dag

mod dag;
mod graphs;
mod node;
mod scc;
mod toposort;

// -----------------------------------------------------------------------------
// Exports

pub use dag::Dag;
pub use graphs::DiGraph;
pub use node::Node;
pub use scc::{SccIterator, SccNodes};
pub use toposort::ToposortError;
