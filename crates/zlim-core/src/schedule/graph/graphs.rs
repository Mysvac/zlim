//! Directed graph storage used by the scheduler's dependency graph.

use core::fmt::Debug;

use indexmap::IndexMap;
use zlim_utils::hash::{FixedState, HashSet};

use Direction::{Incoming, Outgoing};

use super::Node;

// -----------------------------------------------------------------------------
// Direction
// -----------------------------------------------------------------------------

/// Edge direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    /// An `Outgoing` edge is an outward edge *from* the current node.
    Outgoing = 0,
    /// An `Incoming` edge is an inbound edge *to* the current node.
    Incoming = 1,
}

// -----------------------------------------------------------------------------
// DiGraph
// -----------------------------------------------------------------------------

/// A directed graph with an edge set, used to model job dependency edges.
#[derive(Clone)]
pub struct DiGraph {
    nodes: IndexMap<Node, Vec<(Node, Direction)>, FixedState>,
    edges: HashSet<(Node, Node)>,
}

impl Debug for DiGraph {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.nodes.fmt(f)
    }
}

impl Default for DiGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl DiGraph {
    /// Creates a new empty directed graph.
    pub const fn new() -> Self {
        Self {
            nodes: IndexMap::with_hasher(FixedState),
            edges: HashSet::new(),
        }
    }

    /// Creates a new empty directed graph with the given node and edge
    /// capacities.
    pub fn with_capacity(nodes: usize, edges: usize) -> Self {
        Self {
            nodes: IndexMap::with_capacity_and_hasher(nodes, FixedState),
            edges: HashSet::with_capacity(edges),
        }
    }
}

impl DiGraph {
    fn remove_link(&mut self, x: Node, y: Node, dir: Direction) -> bool {
        if let Some(links) = self.nodes.get_mut(&x) {
            let index = links.iter().copied().position(|link| link == (y, dir));

            if let Some(index) = index {
                links.swap_remove(index);
                return true;
            }
        };
        false
    }

    /// Returns the number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Reserves capacity for at least `additional` more nodes.
    pub fn reserve_nodes(&mut self, additional: usize) {
        self.nodes.reserve(additional);
    }

    /// Reserves capacity for at least `additional` more edges.
    pub fn reserve_edges(&mut self, additional: usize) {
        self.edges.reserve(additional);
    }

    /// Returns `true` if the graph contains `n`.
    pub fn contains_node(&self, n: Node) -> bool {
        self.nodes.contains_key(&n)
    }

    /// Returns `true` if the graph contains an edge from `a` to `b`.
    pub fn contains_edge(&self, a: Node, b: Node) -> bool {
        self.edges.contains(&(a, b))
    }

    /// Inserts `n` into the graph if it is not already present.
    pub fn insert_node(&mut self, n: Node) {
        self.nodes.entry(n).or_default();
    }

    /// Removes `n` and all edges incident to it.
    pub fn remove_node(&mut self, n: Node) {
        let Some(links) = self.nodes.swap_remove(&n) else {
            return;
        };

        links.into_iter().for_each(|(to, dir)| {
            let (edge, rdir) = if dir == Outgoing {
                ((n, to), Incoming)
            } else {
                ((to, n), Outgoing)
            };

            self.remove_link(to, n, rdir);
            self.edges.remove(&edge);
        })
    }

    /// Inserts a directed edge from `a` to `b`, if it is not already present.
    pub fn insert_edge(&mut self, a: Node, b: Node) {
        if self.edges.insert((a, b)) {
            // insert in the adjacency list if it's a new edge
            self.nodes
                .entry(a)
                .or_insert_with(|| Vec::with_capacity(1))
                .push((b, Outgoing));
            if a != b {
                // self loops don't have the Incoming entry
                self.nodes
                    .entry(b)
                    .or_insert_with(|| Vec::with_capacity(1))
                    .push((a, Incoming));
            }
        }
    }

    /// Removes the edge from `a` to `b`, returning `true` if it existed.
    pub fn remove_edge(&mut self, a: Node, b: Node) -> bool {
        let exist1 = self.remove_link(a, b, Outgoing);
        let exist2 = if a != b {
            self.remove_link(b, a, Incoming)
        } else {
            core::hint::cold_path();
            exist1
        };
        let weight = self.edges.remove(&(a, b));
        debug_assert!(exist1 == exist2 && exist1 == weight);
        weight
    }

    /// Returns an iterator over all nodes in the graph.
    pub fn nodes(
        &self,
    ) -> impl DoubleEndedIterator<Item = Node> + ExactSizeIterator<Item = Node> + '_ {
        self.nodes.keys().copied()
    }

    /// Returns an iterator over all edges in the graph.
    pub fn all_edges(&self) -> impl ExactSizeIterator<Item = (Node, Node)> + '_ {
        self.edges.iter().copied()
    }

    /// Returns an iterator over the outgoing edges from `a`.
    pub fn edges(&self, a: Node) -> impl DoubleEndedIterator<Item = (Node, Node)> + '_ {
        let iter = match self.nodes.get(&a) {
            Some(neigh) => neigh.iter(),
            None => [].iter(),
        };

        iter.copied()
            .filter_map(move |(b, dir)| (dir == Outgoing).then_some((a, b)))
    }

    /// Returns an iterator over the outgoing neighbors of `a`.
    pub fn neighbors(&self, a: Node) -> impl DoubleEndedIterator<Item = Node> + '_ {
        let iter = match self.nodes.get(&a) {
            Some(neigh) => neigh.iter(),
            None => [].iter(),
        };

        iter.copied()
            .filter_map(|(n, dir)| (dir == Outgoing).then_some(n))
    }

    /// Returns an iterator over the incoming edges of `a` (including
    /// self-edges).
    pub fn rev_edges(&self, a: Node) -> impl DoubleEndedIterator<Item = (Node, Node)> + '_ {
        let iter = match self.nodes.get(&a) {
            Some(neigh) => neigh.iter(),
            None => [].iter(),
        };

        iter.copied()
            .filter_map(move |(b, d)| (d == Incoming || b == a).then_some((a, b)))
    }

    /// Returns an iterator over the incoming neighbors of `a` (including
    /// `a` itself for self-edges).
    pub fn rev_neighbors(&self, a: Node) -> impl DoubleEndedIterator<Item = Node> + '_ {
        let iter = match self.nodes.get(&a) {
            Some(neigh) => neigh.iter(),
            None => [].iter(),
        };

        iter.copied()
            .filter_map(move |(n, d)| (d == Incoming || n == a).then_some(n))
    }

    pub(super) fn to_index(&self, ix: Node) -> usize {
        self.nodes.get_index_of(&ix).unwrap()
    }
}
