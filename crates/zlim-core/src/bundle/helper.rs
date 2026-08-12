use core::any::TypeId;
use std::collections::BTreeSet;

use zlim_ptr::OwningPtr;
use zlim_utils::hash::{HashSet, NoopState};

use crate::component::{Component, Components};
use crate::component::{ComponentDB, ComponentId};
use crate::table::{Table, TableRow};
use crate::tick::Tick;
use crate::utils::{DebugCheckedUnwrap, SlicePool};

// -----------------------------------------------------------------------------
// ComponentCollector
// -----------------------------------------------------------------------------

/// Collects component type IDs during bundle registration.
///
/// `ComponentCollector` is used by [`Bundle::collect`] to register every
/// component type that a bundle needs.  After collection, the sorted
/// component ID list determines the target archetype for entity spawning.
///
/// # Static vs. world-bound collection
///
/// When `components` is `Some(&mut Components)`, component types are
/// registered into the specific world's component registry.  When `None`,
/// only the process-global `ComponentDB` registry is used — this is
/// appropriate for static analysis or early validation.
///
/// [`Bundle::collect`]: crate::bundle::Bundle::collect
pub struct ComponentCollector<'a> {
    components: Option<&'a mut Components>,
    collected: BTreeSet<ComponentId>,
}

impl<'a> ComponentCollector<'a> {
    /// Creates a new collector.
    ///
    /// Pass `Some(&mut world.components)` to register types into a specific
    /// world, or `None` to use only the global registry.
    #[inline(always)]
    pub fn new(components: Option<&'a mut Components>) -> Self {
        ComponentCollector {
            components,
            collected: BTreeSet::new(),
        }
    }

    /// Collects a component type, registering it if necessary.
    ///
    /// If a world-bound `Components` registry was provided at construction,
    /// the type is registered into that world.  Otherwise, only the global
    /// `ComponentDB` is consulted.
    ///
    /// This method is marked `#[inline(never)]` to reduce code bloat — it
    /// is called once per component type per bundle variant, which is cold
    /// relative to the hot spawn path.
    #[inline(never)]
    pub fn collect<C: Component>(&mut self) {
        if let Some(c) = &mut self.components {
            self.collected.insert(c.get::<C>().id);
        } else {
            self.collected.insert(ComponentDB::of::<C>().id);
        }
    }

    /// Finalises collection and returns the sorted, deduplicated component
    /// ID list.
    ///
    /// The returned slice is interned via `SlicePool` and has `'static`
    /// lifetime.
    #[inline(never)]
    pub fn finish(self) -> &'static [ComponentId] {
        let buf: Vec<ComponentId> = self.collected.into_iter().collect();
        debug_assert!(buf.is_sorted());
        SlicePool::component(&buf)
    }
}

// -----------------------------------------------------------------------------
// ComponentWriter
// -----------------------------------------------------------------------------

/// Writes component data into a target table row during entity spawning.
///
/// `ComponentWriter` is used by [`Bundle::write`] to copy component data
/// from the bundle's memory into the destination table's column storage.
///
/// # Duplicate handling
///
/// When the same component type appears more than once in a bundle (e.g.,
/// through tuple nesting), the **last** write wins.  The first call to
/// [`write`] or [`write_custom`] for a given Component [`TypeId`] calls
/// [`Table::init_item`]; subsequent calls call [`Table::replace_item`],
/// which drops the previous value and writes the new one.
///
/// [`Bundle::write`]: crate::bundle::Bundle::write
/// [`write`]: ComponentWriter::write
/// [`write_custom`]: ComponentWriter::write_custom
/// [`Table::init_item`]: crate::table::Table::init_item
/// [`Table::replace_item`]: crate::table::Table::replace_item
pub struct ComponentWriter<'a> {
    tick: Tick,
    table_row: TableRow,
    table: &'a mut Table,
    writed: HashSet<TypeId, NoopState>,
}

impl ComponentWriter<'_> {
    /// # Safety
    /// Guaranteed by the caller.
    #[inline]
    pub unsafe fn new<'a>(
        tick: Tick,
        table: &'a mut Table,
        table_row: TableRow,
    ) -> ComponentWriter<'a> {
        let hint = table.components().len();
        let cap = hint + (hint >> 1);
        ComponentWriter {
            tick,
            table_row,
            table,
            writed: HashSet::with_capacity_and_hasher(cap, NoopState),
        }
    }

    /// Marks a component type as already written without performing a write.
    ///
    /// This is useful when the component was initialised before the bundle
    /// write phase (e.g., by the entity spawner).
    ///
    /// # Safety
    ///
    /// The component `ty` must already be initialised in the target table row.
    #[inline(always)]
    pub unsafe fn set_writed(&mut self, ty: TypeId) {
        self.writed.insert(ty);
    }

    /// Writes a component value by moving ownership into storage.
    ///
    /// This is a convenience wrapper around [`write`] that converts a
    /// value into an [`OwningPtr`] via [`zlim_ptr::into_owning!`].
    ///
    /// # Safety
    ///
    /// - The component's column must exist in the target table.
    #[inline(always)]
    pub unsafe fn write_custom<T: Component>(&mut self, data: T) {
        let type_id = TypeId::of::<T>();

        zlim_ptr::into_owning!(data);

        unsafe { self.write_internal(type_id, data) };
    }

    /// Writes component data from an [`OwningPtr`] into the target row.
    ///
    /// # Safety
    ///
    /// - The component's column must exist in the target table.
    /// - `data` must point to a valid, properly-aligned instance of `T`.
    #[inline(always)]
    pub unsafe fn write<T: Component>(&mut self, data: OwningPtr<'_>) {
        let type_id = TypeId::of::<T>();

        unsafe { self.write_internal(type_id, data) };
    }

    /// Internal write dispatch — first write initialises, subsequent writes
    /// replace (later-overrides-earlier semantics).
    ///
    /// # Safety
    ///
    /// - `type_id` must be a valid `Component` present in the table.
    /// - `data` must point to valid component data.
    #[inline(never)]
    unsafe fn write_internal(&mut self, ty: TypeId, data: OwningPtr<'_>) {
        unsafe {
            let col = self.table.get_type_col(ty).debug_checked_unwrap();
            let row = self.table_row;
            if self.writed.insert(ty) {
                // First write for this component — allocate and
                // initialise.
                self.table.init_item(col, row, data, self.tick);
            } else {
                // Duplicate component — replace the previous value.
                self.table.replace_item(col, row, data, self.tick);
            }
        }
    }
}
