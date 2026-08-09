use core::fmt::{Debug, Formatter};
use core::iter::FusedIterator;

use zlim_utils::hash::HashMap;

use super::Table;
use super::TableId;
use crate::component::{ComponentId, Components};

// -----------------------------------------------------------------------------
// Tables
// -----------------------------------------------------------------------------

pub struct Tables {
    tables: Vec<Table>,
    mapper: HashMap<&'static [ComponentId], TableId>,
}

impl Default for Tables {
    fn default() -> Self {
        let mut val = Self {
            tables: Vec::with_capacity(32),
            mapper: HashMap::with_capacity(32),
        };

        val.tables.push(Table::empty());
        val.mapper.insert(&[], TableId::EMPTY);

        val
    }
}

impl Debug for Tables {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(self.tables.as_slice(), f)
    }
}

// -----------------------------------------------------------------------------
// register

impl Tables {
    /// Registers a new table with the given component set, or returns an existing one.
    ///
    /// # Safety
    /// - `idents` must be sorted and contain valid component IDs
    /// - All component infos must be accessible from `components`
    pub unsafe fn register(
        &mut self,
        components: &Components,
        idents: &'static [ComponentId],
    ) -> TableId {
        use zlim_utils::hash::map::Entry;
        debug_assert!(idents.is_sorted());

        match self.mapper.entry(idents) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                ::core::hint::cold_path();
                let table_id = TableId::without_provenance(self.tables.len());
                let table = Table::new(table_id, components, idents);
                self.tables.push(table);
                entry.insert(table_id);
                table_id
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Methods

impl Tables {
    /// Returns `true` if no tables.
    ///
    /// Always return `false` because `table[0]` is exist.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    /// Returns the number of registered tables.
    #[inline]
    pub fn len(&self) -> usize {
        self.tables.len()
    }

    /// Returns the ID of the table exactly matching the given component set, if any.
    ///
    /// The component slice must use the same canonical ordering used during
    /// table registration.
    #[inline]
    pub fn get_id(&self, components: &[ComponentId]) -> Option<TableId> {
        self.mapper.get(components).copied()
    }

    /// Returns a reference to the table with the given ID, if it exists.
    #[inline]
    pub fn get(&self, id: TableId) -> Option<&Table> {
        self.tables.get(id.index())
    }

    /// Returns a mutable reference to the table with the given ID, if it exists.
    #[inline]
    pub fn get_mut(&mut self, id: TableId) -> Option<&mut Table> {
        self.tables.get_mut(id.index())
    }

    /// Returns a reference to the table with the given ID without bounds checking.
    ///
    /// # Safety
    /// - `id` must be a valid table ID obtained from this registry
    /// - The table must not be concurrently accessed mutably
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, id: TableId) -> &Table {
        debug_assert!(id.index() < self.tables.len());
        unsafe { self.tables.get_unchecked(id.index()) }
    }

    /// Returns a mutable reference to the table with the given ID without bounds checking.
    ///
    /// # Safety
    /// - `id` must be a valid table ID obtained from this registry
    /// - No other references to the table may exist
    #[inline(always)]
    pub unsafe fn get_unchecked_mut(&mut self, id: TableId) -> &mut Table {
        debug_assert!(id.index() < self.tables.len());
        unsafe { self.tables.get_unchecked_mut(id.index()) }
    }

    /// Returns an iterator over the tables.
    #[inline]
    pub fn iter(&self) -> impl FusedIterator<Item = &'_ Table> {
        self.tables.iter()
    }

    /// Returns an iterator that allows modifying each table.
    #[inline]
    pub fn iter_mut(&mut self) -> impl FusedIterator<Item = &'_ mut Table> {
        self.tables.iter_mut()
    }
}

// -----------------------------------------------------------------------------
