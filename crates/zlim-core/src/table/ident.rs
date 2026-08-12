use core::fmt::{Debug, Display, Formatter};
use core::hash::Hash;

use crate::entity::EntityId;

// -----------------------------------------------------------------------------
// TableId
// -----------------------------------------------------------------------------

crate::utils::define_ident!(
    /// A unique identifier for a [`Table`] within a specific [`World`].
    ///
    /// Each distinct component set (archetype) is assigned one `TableId`.
    /// The empty table (entity with no components) is always available at
    /// [`TableId::EMPTY`].
    ///
    /// [`Table`]: super::Table
    /// [`World`]: crate::world::World
    TableId
);

impl TableId {
    /// The ID of the empty table — entities with no components live here.
    ///
    /// The empty table is always registered at index 0 and never contains
    /// columns.
    pub const EMPTY: TableId = TableId::without_provenance(0);
}

// -----------------------------------------------------------------------------
// TableRow & TableCol
// -----------------------------------------------------------------------------

/// Row position within a table, identifying a specific entity's slot.
///
/// Once allocated via [`Table::alloc_row`], the row index is stable until
/// the entity is removed or moved to another table.  When the last row is
/// swap-removed, the displaced entity's row changes — callers must track
/// this via [`MovedEntityRow`].
///
/// [`Table::alloc_row`]: super::Table::alloc_row
/// [`MovedEntityRow`]: super::MovedEntityRow
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct TableRow(pub u32);

/// Column position within a table, identifying a specific component type's
/// storage.
///
/// Columns are ordered identically to the table's component list
/// ([`Table::components`]).  The column index is stable for the lifetime
/// of the table.
///
/// [`Table::components`]: super::Table::components
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
// MovedEntityRow
// -----------------------------------------------------------------------------

/// Records a change in an entity's storage location.
///
/// Returned by [`Table::move_row`] and [`Table::dealloc_row`] so that
/// callers can update the entity tree with any row-index changes that
/// occurred during swap-removal.
///
/// # Variants
///
/// | Variant | Meaning |
/// |---------|---------|
/// | `Some { entity, new_row }` | The entity that was swapped into the removed row, and the row it now occupies |
/// | `None` | The removed entity was the last row; no other entity was displaced |
///
/// [`Table::move_row`]: super::Table::move_row
/// [`Table::dealloc_row`]: super::Table::dealloc_row
#[derive(Debug, Clone, Copy)]
pub enum MovedEntityRow {
    /// The displaced entity and its new row index.
    Some { entity: EntityId, new_row: TableRow },
    /// The removed row was the last entity; nothing was displaced.
    None,
}
