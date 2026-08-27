//! Table, row, and column identifiers.

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
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::table::TableId;
    ///
    /// // The empty table — entities without components — always exists and
    /// // is registered at index 0.
    /// let empty = TableId::EMPTY;
    /// assert_eq!(empty.index(), 0);
    ///
    /// // Table IDs are opaque handles into the world's `Tables` registry;
    /// // obtain one via `world.tables().get_id(components)`.
    /// ```
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
/// # Example
///
/// ```ignore
/// // Rows are dense and removal is a swap-remove: removing the last row
/// // displaces nothing, but removing any other row moves the last row into
/// // the gap.  `dealloc_row` / `move_row` report that displacement as a
/// // `MovedEntityRow`.
/// let table: &mut Table = /* obtained from the registry */;
/// let removed: TableRow = TableRow(2);
/// let _displacement: MovedEntityRow = unsafe { table.dealloc_row::<true>(removed) };
/// ```
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
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::table::TableCol;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Position {
///     x: f32,
/// }
///
/// let mut world = World::alloc();
/// world.spawn((Position { x: 1.0 },), None);
///
/// // Find the archetype table that holds `Position` entities.
/// let table = world
///     .tables()
///     .iter()
///     .find(|table| !table.entities().is_empty())
///     .unwrap();
///
/// // Column indices are stable for the lifetime of the table, and a
/// // column can be resolved from a component ID or a `TypeId`.
/// assert_eq!(
///     table.get_table_col(ComponentDB::of::<Position>().id),
///     Some(TableCol(0))
/// );
/// assert_eq!(
///     table.get_type_col(core::any::TypeId::of::<Position>()),
///     Some(TableCol(0))
/// );
/// ```
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
