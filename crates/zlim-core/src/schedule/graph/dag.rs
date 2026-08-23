//! A directed acyclic graph with a cached topological ordering.

use core::fmt::Debug;
use core::ops::{Deref, DerefMut};

use super::{DiGraph, Node, ToposortError};

// -----------------------------------------------------------------------------
// Dag

/// A directed acyclic graph structure.
#[derive(Clone)]
pub struct Dag {
    /// The underlying directed graph.
    graph: DiGraph,
    /// A cached topological ordering of the graph. This is recomputed when the
    /// graph is modified, and is not valid when `dirty` is true.
    toposort: Vec<Node>,
    /// Whether the graph has been modified since the last topological sort.
    dirty: bool,
}

impl Default for Dag {
    fn default() -> Self {
        Self::new()
    }
}

impl Dag {
    /// Creates a new directed acyclic graph.
    pub const fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            toposort: Vec::new(),
            dirty: false,
        }
    }

    /// Creates a new directed acyclic graph with specific capacity.
    pub fn with_capacity(nodes: usize, edges: usize) -> Self {
        Self {
            graph: DiGraph::with_capacity(nodes, edges),
            toposort: Vec::new(),
            dirty: false,
        }
    }

    /// Read-only access to the underlying directed graph.
    #[must_use]
    pub fn graph(&self) -> &DiGraph {
        &self.graph
    }

    /// Mutable access to the underlying directed graph. Marks the graph as dirty.
    #[must_use = "This function marks the graph as dirty, so it should be used."]
    pub fn graph_mut(&mut self) -> &mut DiGraph {
        self.dirty = true;
        &mut self.graph
    }

    /// Returns whether the graph is dirty (i.e., has been modified since the
    /// last topological sort).
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Returns whether the graph is topologically sorted (i.e., not dirty).
    #[must_use]
    pub fn is_toposorted(&self) -> bool {
        !self.dirty
    }

    /// Returns the cached toposort if the graph is not dirty, otherwise returns
    /// `None`.
    #[must_use = "This method only returns a cached value and does not compute anything."]
    pub fn get_toposort(&self) -> Option<&[Node]> {
        if self.dirty {
            None
        } else {
            Some(&self.toposort)
        }
    }

    /// Ensures the cached topological order is up to date.
    ///
    /// If the graph is marked dirty, this recomputes the topological order
    /// from the underlying graph and refreshes the internal cache.
    ///
    /// # Errors
    ///
    /// Returns [`ToposortError`] if the graph cannot be topologically sorted,
    /// typically due to a cycle.
    ///
    /// [`ToposortError`]: crate::schedule::ToposortError
    pub fn ensure_toposorted(&mut self) -> Result<(), ToposortError> {
        if self.dirty {
            // recompute the toposort, reusing the existing allocation
            self.toposort = self.graph.toposort(core::mem::take(&mut self.toposort))?;
            self.dirty = false;
        }
        Ok(())
    }

    /// Returns the current topological order of this graph.
    ///
    /// This will recompute the order if the graph is dirty, otherwise it
    /// returns the cached order.
    ///
    /// # Errors
    ///
    /// Returns [`ToposortError`] if topological sorting fails.
    ///
    /// [`ToposortError`]: crate::schedule::ToposortError
    pub fn toposort(&mut self) -> Result<&[Node], ToposortError> {
        self.ensure_toposorted()?;
        Ok(&self.toposort)
    }

    /// Returns both the current topological order and the underlying graph.
    ///
    /// This will recompute the cached order if needed before returning.
    ///
    /// # Errors
    ///
    /// Returns [`ToposortError`] if topological sorting fails.
    ///
    /// [`ToposortError`]: crate::schedule::ToposortError
    pub fn toposort_and_graph(&mut self) -> Result<(&[Node], &DiGraph), ToposortError> {
        self.ensure_toposorted()?;
        Ok((&self.toposort, &self.graph))
    }
}

impl Deref for Dag {
    type Target = DiGraph;

    fn deref(&self) -> &Self::Target {
        self.graph()
    }
}

impl DerefMut for Dag {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.graph_mut()
    }
}

impl Debug for Dag {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.dirty {
            f.debug_struct("Dag")
                .field("graph", &self.graph)
                .field("dirty", &self.dirty)
                .finish()
        } else {
            f.debug_struct("Dag")
                .field("graph", &self.graph)
                .field("toposort", &self.toposort)
                .finish()
        }
    }
}

impl From<DiGraph> for Dag {
    fn from(value: DiGraph) -> Self {
        Self {
            graph: value,
            toposort: Vec::new(),
            dirty: true,
        }
    }
}

impl From<Dag> for DiGraph {
    fn from(value: Dag) -> Self {
        value.graph
    }
}
