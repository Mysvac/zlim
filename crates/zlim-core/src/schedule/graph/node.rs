//! Graph node type.

use core::cmp::Ordering;
use core::fmt::{Debug, Display, Formatter};
use core::hash::Hash;

// -----------------------------------------------------------------------------

/// A node in a schedule dependency graph.
///
/// A `Node` pairs a slot index (`idx`) with a generation tag (`tag`). The tag
/// distinguishes reuses of the same slot after removal, preventing stale
/// references from aliasing newer nodes.
#[derive(Clone, Copy)]
pub struct Node {
    /// Generation tag; increments each time the slot is reused.
    pub tag: u16,
    /// Slot index into the graph's node storage.
    pub idx: u16,
}

impl Node {
    /// Returns the node's slot index as a `usize`.
    #[inline(always)]
    pub(crate) const fn index(self) -> usize {
        self.idx as usize
    }

    /// Packs the node into a `u32` as `(idx << 16) | tag`.
    #[inline(always)]
    pub const fn as_u32(self) -> u32 {
        ((self.idx as u32) << 16) + (self.tag as u32)
    }

    /// Reinterprets the node as its raw `u32` bit pattern.
    ///
    /// Only valid because `Node` has the same layout as `u32`.
    #[inline(always)]
    pub const fn as_bits(self) -> u32 {
        use core::mem::transmute;
        unsafe { transmute::<Self, u32>(self) }
    }
}

impl PartialOrd for Node {
    #[inline(always)]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Node {
    #[inline(always)]
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_u32().cmp(&other.as_u32())
    }
}

impl PartialEq for Node {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.as_bits() == other.as_bits()
    }
}

impl Eq for Node {}

impl Hash for Node {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write_u32(self.as_bits());
    }
}

impl Display for Node {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}v{}", self.idx, self.tag)
    }
}

impl Debug for Node {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}v{}", self.idx, self.tag)
    }
}

// -----------------------------------------------------------------------------
