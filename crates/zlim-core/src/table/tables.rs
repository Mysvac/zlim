//! Registry of all tables in a world.

use core::cmp::Ordering;
use core::fmt::{Debug, Formatter};
use core::iter::FusedIterator;

use zlim_utils::hash::HashMap;

use super::Table;
use super::TableId;
use crate::bundle::{BundleId, Bundles};
use crate::component::{ComponentCollector, ComponentId, Components};
use crate::utils::{DebugCheckedUnwrap, SlicePool};

// -----------------------------------------------------------------------------
// Tables

/// Registry of all tables in a world, keyed by component set.
///
/// Provides O(1) lookup by exact component-set signature and lazily caches
/// archetype transitions for bundle insertion and removal.
///
/// The registry is owned by the [`World`] and always contains at least one
/// table: the empty table ([`TableId::EMPTY`]) for entities without
/// components.
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_reflect::derive::TypePath;
/// use zlim_core::table::Tables;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Position {
///     x: f32,
/// }
///
/// let mut world = World::alloc();
/// world.spawn((Position { x: 1.0 },), None);
///
/// // The registry owns the empty table plus one table per distinct
/// // component set (archetype).
/// let tables: &Tables = world.tables();
/// assert_eq!(tables.len(), 2);
///
/// for _table in tables.iter() {
///     // ...
/// }
/// ```
///
/// [`World`]: crate::world::World
pub struct Tables {
    tables: Vec<Table>,
    mapper: HashMap<&'static [ComponentId], TableId>,
    bundles: Vec<Option<TableId>>,
}

impl Debug for Tables {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(self.tables.as_slice(), f)
    }
}

impl Tables {
    pub(crate) fn new() -> Self {
        let mut val = Self {
            tables: Vec::with_capacity(32),
            mapper: HashMap::with_capacity(32),
            bundles: Vec::with_capacity(32),
        };

        val.tables.push(Table::empty());
        val.mapper.insert(&[], TableId::EMPTY);

        val
    }
}

// -----------------------------------------------------------------------------
// Basic methods

impl Tables {
    /// Returns `true` if there are no tables.
    ///
    /// This always returns `false`, because the empty table is always
    /// registered.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    /// Returns the number of registered tables.
    #[inline]
    pub fn len(&self) -> usize {
        self.tables.len()
    }

    /// Returns the ID of the table exactly matching the given component set,
    /// if any.
    #[inline]
    pub fn get_id(&self, components: &[ComponentId]) -> Option<TableId> {
        self.mapper.get(components).copied()
    }

    /// Returns a reference to the table with the given ID, if it exists.
    #[inline]
    pub fn get(&self, id: TableId) -> Option<&Table> {
        self.tables.get(id.index())
    }

    /// Returns a mutable reference to the table with the given ID, if it
    /// exists.
    #[inline]
    pub fn get_mut(&mut self, id: TableId) -> Option<&mut Table> {
        self.tables.get_mut(id.index())
    }

    /// Returns mutable references to the tables at indices `x` and `y`
    /// at once.
    ///
    /// Returns `None` if either index is out of bounds or if `x == y`.
    #[inline]
    pub fn get_mut_2(&mut self, x: usize, y: usize) -> Option<[&mut Table; 2]> {
        self.tables.get_disjoint_mut([x, y]).ok()
    }

    /// Returns a reference to the table with the given ID without bounds
    /// checking.
    ///
    /// # Safety
    /// - `id` must be a valid table ID obtained from this registry
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, id: TableId) -> &Table {
        debug_assert!(id.index() < self.tables.len());
        unsafe { self.tables.get_unchecked(id.index()) }
    }

    /// Returns a mutable reference to the table with the given ID without
    /// bounds checking.
    ///
    /// # Safety
    /// - `id` must be a valid table ID obtained from this registry
    #[inline(always)]
    pub unsafe fn get_unchecked_mut(&mut self, id: TableId) -> &mut Table {
        debug_assert!(id.index() < self.tables.len());
        unsafe { self.tables.get_unchecked_mut(id.index()) }
    }

    /// Returns mutable references to the tables at indices `x` and `y`
    /// without any checks.
    ///
    /// # Safety
    /// - `x` and `y` must be valid indices into this registry (each
    ///   `< self.len()`)
    /// - `x != y`
    #[inline]
    pub unsafe fn get_unchecked_mut_2(&mut self, x: usize, y: usize) -> [&mut Table; 2] {
        unsafe { self.tables.get_disjoint_unchecked_mut([x, y]) }
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
// register

impl Tables {
    /// Looks up or creates a table for the given component set.
    #[inline(always)]
    fn register_dynamic(
        &mut self,
        idents: &'static [ComponentId],
        components: &Components,
    ) -> TableId {
        use zlim_utils::hash::map::Entry;

        match self.mapper.entry(idents) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let table_id = TableId::without_provenance(self.tables.len());
                let table = Table::new(table_id, components, idents);
                self.tables.push(table);
                entry.insert(table_id);
                table_id
            }
        }
    }

    /// Registers a new table with the given component set, or returns an
    /// existing one.
    ///
    /// # Safety
    /// - `BundleId` must be valid (already registered in Bundles).
    #[inline]
    pub(crate) fn register(
        &mut self,
        bundle_id: BundleId,
        bundles: &Bundles,
        components: &Components,
    ) -> TableId {
        #[cold]
        #[inline(never)]
        fn register_cold(
            this: &mut Tables,
            bundle_id: BundleId,
            bundles: &Bundles,
            components: &Components,
        ) -> TableId {
            let index = bundle_id.index();
            let idents = unsafe { bundles.get_unchecked(bundle_id).components() };

            debug_assert!(idents.is_sorted());
            let table_id = this.register_dynamic(idents, components);

            if this.bundles.len() <= index {
                this.bundles.resize_with(index + 1, || None);
            }

            unsafe {
                *this.bundles.get_unchecked_mut(index) = Some(table_id);
            }

            table_id
        }

        if let Some(Some(id)) = self.bundles.get(bundle_id.index()).copied() {
            return id;
        };

        register_cold(self, bundle_id, bundles, components)
    }

    /// Computes (or retrieves from cache) the target table after inserting
    /// the given bundle into `current`.
    ///
    /// Returns the ID of the table whose component set is the union of the
    /// current table's components and the bundle's components.
    #[inline]
    pub(crate) fn table_after_insert(
        &mut self,
        current: TableId,
        bundle_id: BundleId,
        bundles: &Bundles,
        components: &Components,
    ) -> TableId {
        // Check cache on the current table first.
        if let Some(cached) = unsafe { self.get_unchecked(current).after_insert(bundle_id) } {
            return cached;
        }

        #[cold]
        #[inline(never)]
        fn compute_and_cache(
            this: &mut Tables,
            current: TableId,
            bundle_id: BundleId,
            bundles: &Bundles,
            components: &Components,
        ) -> TableId {
            let current_comps = unsafe { this.get_unchecked(current).components() };
            let bundle_comps = unsafe { bundles.get_unchecked(bundle_id).components() };

            let merged = merge_sorted(current_comps, bundle_comps);
            let interned = SlicePool::component(&merged);
            let target = this.register_dynamic(interned, components);

            let table = unsafe { this.get_unchecked_mut(current) };
            table.set_after_insert(bundle_id, target);

            target
        }

        compute_and_cache(self, current, bundle_id, bundles, components)
    }

    /// Computes (or retrieves from cache) the target table after removing
    /// the given bundle from `current`.
    ///
    /// Returns the ID of the table whose component set is the current
    /// table's components minus the bundle's components.
    #[inline]
    pub(crate) fn table_after_remove(
        &mut self,
        current: TableId,
        bundle_id: BundleId,
        bundles: &Bundles,
        components: &Components,
    ) -> TableId {
        // Check cache first.
        if let Some(cached) = unsafe { self.get_unchecked(current).after_remove(bundle_id) } {
            return cached;
        }

        #[cold]
        #[inline(never)]
        fn compute_and_cache(
            this: &mut Tables,
            current: TableId,
            bundle_id: BundleId,
            bundles: &Bundles,
            components: &Components,
        ) -> TableId {
            let current_comps = unsafe { this.get_unchecked(current).components() };
            let bundle_comps = unsafe { bundles.get_unchecked(bundle_id).components() };

            let subtracted = subtract_sorted(current_comps, bundle_comps);

            let mut collector = ComponentCollector::new(Some(components));

            for &id in subtracted.iter() {
                let db = unsafe { components.get_by_id(id).debug_checked_unwrap() };
                if let Some(required) = db.required {
                    required.collect(&mut collector);
                }
                collector.insert(id);
            }

            let interned = collector.finish();
            let target = this.register_dynamic(interned, components);

            let table = unsafe { this.get_unchecked_mut(current) };
            table.set_after_remove(bundle_id, target);

            target
        }

        compute_and_cache(self, current, bundle_id, bundles, components)
    }
}

// -----------------------------------------------------------------------------
// helpers

/// Merges two sorted `ComponentId` slices into a sorted union.
#[inline]
fn merge_sorted(a: &[ComponentId], b: &[ComponentId]) -> Vec<ComponentId> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        unsafe {
            core::hint::assert_unchecked(out.len() < out.capacity());
        }
        match a[i].cmp(&b[j]) {
            Ordering::Less => {
                out.push(a[i]);
                i += 1;
            }
            Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
            Ordering::Greater => {
                out.push(b[j]);
                j += 1;
            }
        }
    }
    out.extend_from_slice(&a[i..]);
    out.extend_from_slice(&b[j..]);
    out
}

/// Subtracts `b` from `a` (both sorted), producing `a \ b`.
#[inline]
fn subtract_sorted(a: &[ComponentId], b: &[ComponentId]) -> Vec<ComponentId> {
    let mut out = Vec::with_capacity(a.len());
    let mut j = 0;
    for &item in a {
        while j < b.len() && b[j] < item {
            j += 1;
        }
        if j < b.len() && b[j] == item {
            continue;
        }
        unsafe {
            core::hint::assert_unchecked(out.len() < out.capacity());
        }
        out.push(item);
    }
    out
}

// -----------------------------------------------------------------------------
