use core::fmt::{Debug, Display, Formatter};
use core::hash::Hash;

use crate::entity::EntityId;

// -----------------------------------------------------------------------------
// TableId
// -----------------------------------------------------------------------------

crate::utils::define_ident!(
    /// A unique identifier for a `Table` within a specific `World`.
    TableId
);

impl TableId {
    pub const EMPTY: TableId = TableId::without_provenance(0);
}

// -----------------------------------------------------------------------------
// TableRow & TableCol
// -----------------------------------------------------------------------------

/// Row position within a table.
///
/// Represents an index into a table's columnar storage for a specific entity.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct TableRow(pub u32);

/// Column position within a table.
///
/// Represents an index into a table's columnar storage for a specific component type.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct TableCol(pub u32);

impl Debug for TableRow {
    #[inline(always)]
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl Display for TableRow {
    #[inline(always)]
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl Hash for TableRow {
    #[inline(always)]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write_u32(self.0);
    }
}

impl Debug for TableCol {
    #[inline(always)]
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl Display for TableCol {
    #[inline(always)]
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl Hash for TableCol {
    #[inline(always)]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write_u32(self.0);
    }
}

// -----------------------------------------------------------------------------

/// Records a change in an entity's storage location.
///
/// This is used internally when entities move between tables,
/// ensuring that entity locations stay in sync with component storage.
#[derive(Debug, Clone, Copy)]
pub enum MovedEntityRow {
    Some { entity: EntityId, new_row: TableRow },
    None,
}

// -----------------------------------------------------------------------------
