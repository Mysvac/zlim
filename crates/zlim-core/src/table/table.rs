//! Dense columnar table storage.

#![expect(clippy::module_inception, reason = "For better structure.")]

use core::any::TypeId;
use core::fmt::{Debug, Formatter};
use core::num::NonZeroUsize;
use core::panic::{RefUnwindSafe, UnwindSafe};
use core::ptr;

use zlim_log as log;
use zlim_ptr::{OwningPtr, Ptr, PtrMut};
use zlim_utils::debug::DebugLocation;
use zlim_utils::ext::TypeMap;
use zlim_utils::hash::{HashMap, SparseState};

use super::column::Column;
use super::ident::TableId;
use super::ident::{TableCol, TableRow};
use crate::borrow::{UntypedMut, UntypedRef};
use crate::borrow::{UntypedSliceMut, UntypedSliceRef};
use crate::bundle::BundleId;
use crate::component::{ComponentHook, HookContext};
use crate::component::{ComponentId, Components};
use crate::entity::EntityId;
use crate::table::ident::MovedEntityRow;
use crate::tick::Tick;
use crate::utils::{DebugCheckedUnwrap, SlicePool};
use crate::world::DeferredWorld;

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

/// A guard that aborts the process when dropped during allocation failure.
struct AbortOnPanic;

impl Drop for AbortOnPanic {
    /// Aborts the process when dropped.
    #[cold]
    #[inline(never)]
    fn drop(&mut self) {
        log::error!("Aborting due to allocator error.");
        ::std::process::abort();
    }
}

/// Efficient removal operations for [`Vec`] using swap-remove semantics.
trait VecRemoveExt<T> {
    /// Removes and returns the last element without checking bounds.
    ///
    /// # Safety
    /// - `last_index` must be the index of the last element (vector length - 1)
    /// - The vector must have at least one element
    ///
    /// # Performance
    /// O(1), just reads the last element and updates length.
    unsafe fn remove_last(&mut self, last_index: usize) -> T;

    /// Moves the last element to a specified position and returns it.
    ///
    /// This combines a swap-remove operation into a single step:
    /// 1. Reads the last element
    /// 2. Writes it to the target position
    /// 3. Reduces the vector length
    /// 4. Return the copied **last** element.
    ///
    /// # Safety
    /// - `last_index` must be the index of the last element (vector length - 1)
    /// - `to` must be a valid index **less** than `last_index`
    /// - The vector must have at least one element
    ///
    /// # Performance
    /// O(1), just reads from end and writes to target position.
    unsafe fn move_last_to(&mut self, last_index: usize, to: usize) -> T;
}

impl<T: Copy> VecRemoveExt<T> for Vec<T> {
    #[inline(always)]
    unsafe fn remove_last(&mut self, last_index: usize) -> T {
        unsafe {
            let last = self.as_ptr().add(last_index);

            let value = ptr::read(last);
            self.set_len(last_index);
            value
        }
    }

    #[inline(always)]
    unsafe fn move_last_to(&mut self, last_index: usize, to: usize) -> T {
        let base_ptr = self.as_mut_ptr();

        unsafe {
            let src = base_ptr.add(last_index);
            let dst = base_ptr.add(to);

            let value = ptr::read(src);
            ptr::write(dst, value);
            self.set_len(last_index);
            value
        }
    }
}

// -----------------------------------------------------------------------------
// Table
// -----------------------------------------------------------------------------

type HookItem = (ComponentId, ComponentHook);

/// A dense columnar storage table for ECS components.
///
/// Every table belongs to one *archetype*: a fixed set of component types.
/// All entities in a table share exactly that component set, which is what
/// makes the storage dense — each column stores one component type for every
/// row, with no per-entity type tags or gaps.
///
/// |           | Component A | Component B | Component C | .. |
/// |-----------|-------------|-------------|-------------|----|
/// | Entity A  | /* data */  | /* data */  | /* data */  | .. |
/// | Entity B  | /* data */  | /* data */  | /* data */  | .. |
/// | Entity C  | /* data */  | /* data */  | /* data */  | .. |
/// | ........  | ..........  | ..........  | ..........  | .. |
///
/// Rows are identified by [`TableRow`] and columns by [`TableCol`]; the
/// column order always matches the component list returned by
/// [`Table::components`].  Iterating a column (or a whole table) touches
/// contiguous memory, which provides optimal cache locality.
///
/// Tables are created and owned by the world's [`Tables`] registry — users
/// never construct a [`Table`] directly.  An entity moves between tables
/// whenever it gains or loses components; see [`Table::move_row`].
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::table::Tables;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Position {
///     x: f32,
///     y: f32,
/// }
///
/// #[derive(TypePath, Component, Clone)]
/// struct Health {
///     value: u32,
/// }
///
/// let mut world = World::alloc();
///
/// // Both entities share the same component set, so they live in the same
/// // table.  An entity with a different set would live in another table.
/// world.spawn((Position { x: 0.0, y: 0.0 }, Health { value: 100 }), None);
/// world.spawn((Position { x: 1.0, y: 2.0 }, Health { value: 50 }), None);
///
/// let tables: &Tables = world.tables();
///
/// // Two entities of the same archetype share one table; the always-
/// // present empty table (entities without components) makes two total.
/// assert_eq!(tables.len(), 2);
///
/// for table in tables.iter() {
///     // Every row in this table has exactly `table.components()`.
///     for &_entity in table.entities() {
///         // ...
///     }
/// }
/// ```
///
/// [`Tables`]: crate::table::Tables
#[repr(C)]
pub struct Table {
    columns: Box<[Column]>,
    components: &'static [ComponentId],
    mapper: TypeMap<TableCol>,
    entities: Vec<EntityId>,
    id: TableId,
    // Cached component hooks.
    on_add: &'static [HookItem],
    on_clone: &'static [HookItem],
    on_insert: &'static [HookItem],
    on_remove: &'static [HookItem],
    on_discard: &'static [HookItem],
    on_despawn: &'static [HookItem],
    // Cached archetype transitions for bundle insertion/removal.
    after_insert: HashMap<BundleId, TableId, SparseState>,
    after_remove: HashMap<BundleId, TableId, SparseState>,
}

// -----------------------------------------------------------------------------
// Private

impl Table {
    #[cold]
    #[inline]
    pub(super) fn empty() -> Self {
        Self {
            id: TableId::EMPTY,
            columns: Box::new([]),
            components: &[],
            entities: Vec::new(),
            mapper: TypeMap::new(),
            on_add: &[],
            on_clone: &[],
            on_insert: &[],
            on_remove: &[],
            on_discard: &[],
            on_despawn: &[],
            after_insert: HashMap::with_hasher(SparseState),
            after_remove: HashMap::with_hasher(SparseState),
        }
    }

    pub(super) fn new(id: TableId, dbs: &Components, idents: &'static [ComponentId]) -> Self {
        debug_assert!(idents.is_sorted());
        let mut columns: Vec<Column> = Vec::with_capacity(idents.len());
        let mut mapper: TypeMap<TableCol> = TypeMap::with_capacity(idents.len());

        let mut on_add = Vec::new();
        let mut on_clone = Vec::new();
        let mut on_insert = Vec::new();
        let mut on_remove = Vec::new();
        let mut on_discard = Vec::new();
        let mut on_despawn = Vec::new();

        idents.iter().enumerate().for_each(|(index, &id)| {
            let info = unsafe { dbs.get_by_id(id).debug_checked_unwrap() };
            let type_id = info.type_id;
            let layout = info.layout;
            let dropper = info.dropper;
            mapper.insert(type_id, TableCol(index as u32));
            columns.push(unsafe { Column::new(layout, dropper) });
            if let Some(hk) = info.on_add {
                on_add.push((id, hk));
            }
            if let Some(hk) = info.on_clone {
                on_clone.push((id, hk));
            }
            if let Some(hk) = info.on_insert {
                on_insert.push((id, hk));
            }
            if let Some(hk) = info.on_remove {
                on_remove.push((id, hk));
            }
            if let Some(hk) = info.on_discard {
                on_discard.push((id, hk));
            }
            if let Some(hk) = info.on_despawn {
                on_despawn.push((id, hk));
            }
        });

        let on_add = SlicePool::component_hook(&on_add);
        let on_clone = SlicePool::component_hook(&on_clone);
        let on_insert = SlicePool::component_hook(&on_insert);
        let on_remove = SlicePool::component_hook(&on_remove);
        let on_discard = SlicePool::component_hook(&on_discard);
        let on_despawn = SlicePool::component_hook(&on_despawn);

        Self {
            id,
            columns: columns.into_boxed_slice(),
            components: idents,
            entities: Vec::new(),
            mapper,
            on_add,
            on_clone,
            on_insert,
            on_remove,
            on_discard,
            on_despawn,
            after_insert: HashMap::with_hasher(SparseState),
            after_remove: HashMap::with_hasher(SparseState),
        }
    }

    /// Returns the current allocation capacity of the table.
    #[inline(always)]
    fn capacity(&self) -> usize {
        self.entities.capacity()
    }

    /// Returns the number of entities currently stored in the table.
    #[inline(always)]
    fn entity_count(&self) -> usize {
        self.entities.len()
    }

    // Unordered TypeId
    #[inline(always)]
    pub(crate) fn types(&self) -> impl ExactSizeIterator<Item = TypeId> + '_ {
        self.mapper.keys().copied()
    }

    // Unordered TypeId
    #[inline(always)]
    pub(crate) fn type_cols(&self) -> impl ExactSizeIterator<Item = (TypeId, &TableCol)> + '_ {
        self.mapper.iter()
    }
}

// -----------------------------------------------------------------------------
// Basic Trait

impl Debug for Table {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Table")
            .field("id", &self.id)
            .field("components", &self.components)
            .field("entities", &self.entities)
            .finish()
    }
}

impl Drop for Table {
    fn drop(&mut self) {
        let len = self.entity_count();
        let current_capacity = self.capacity();

        self.columns.iter_mut().for_each(|c| unsafe {
            c.drop_slice(len);
            c.dealloc(current_capacity);
        });
    }
}

unsafe impl Sync for Table {}
unsafe impl Send for Table {}

impl UnwindSafe for Table {}
impl RefUnwindSafe for Table {}

// -----------------------------------------------------------------------------
// Basic Methods

impl Table {
    /// Returns this table's identifier.
    #[inline(always)]
    pub fn id(&self) -> TableId {
        self.id
    }

    /// Returns the entities currently stored in row order.
    ///
    /// The order is not stable across removals: swap-removal moves the last
    /// row into the vacated position, reordering the table.
    #[inline(always)]
    pub fn entities(&self) -> &[EntityId] {
        &self.entities
    }

    /// Returns the component schema of this table.
    ///
    /// All rows in this table contain exactly this component set.
    #[inline(always)]
    pub fn components(&self) -> &'static [ComponentId] {
        self.components
    }

    /// Finds the column index for a given component type.
    ///
    /// # Complexity
    /// O(1) (Hash TypeId)
    #[inline]
    pub fn get_type_col(&self, ty: TypeId) -> Option<TableCol> {
        self.mapper.get(ty).copied()
    }

    /// Finds the column index for a given component ID using binary search.
    ///
    /// # Complexity
    /// O(log n) where n is the number of component types
    #[inline]
    pub fn get_table_col(&self, id: ComponentId) -> Option<TableCol> {
        let index = self.components.binary_search(&id).ok()?;
        Some(TableCol(index as u32))
    }

    /// Finds the row index for a given entity using linear search.
    ///
    /// # Complexity
    /// O(n) where n is the number of entities
    ///
    /// Note: This is inefficient and should be avoided in hot paths.  Store
    /// the [`TableRow`] returned by [`Table::alloc_row`] (or by the entity
    /// location) instead.
    #[inline]
    pub fn get_table_row(&self, id: EntityId) -> Option<TableRow> {
        use crate::utils::position_entity;
        let index = position_entity(id, &self.entities)?;
        Some(TableRow(index as u32))
    }

    /// Clamps the change ticks of all components so that stale ticks do not
    /// read as newer than `now`.
    pub fn clamp_ticks(&mut self, now: Tick) {
        let len = self.entity_count();
        self.columns.iter_mut().for_each(|c| unsafe {
            c.clamp_ticks(len, now);
        });
    }

    /// Checks if this table contains a specific component type.
    pub fn contains_type(&self, ty: TypeId) -> bool {
        self.mapper.contains(ty)
    }

    /// Checks if this table contains a specific component.
    pub fn contains_component(&self, id: ComponentId) -> bool {
        // ComponentId is easy to optimize with SIMD, and linear search
        // is faster when the data is less than 100 (release + O3).
        if self.components.len() < 100 {
            crate::utils::contains_component(id, self.components)
        } else {
            ::core::hint::cold_path();
            self.components.binary_search(&id).is_ok()
        }
    }

    /// Checks if this table contains a specific entity.
    pub fn contains_entities(&self, id: EntityId) -> bool {
        crate::utils::contains_entity(id, &self.entities)
    }
}

// -----------------------------------------------------------------------------
// Hook

macro_rules! trigger_function {
    ($name1:ident, $name2:ident, $field:ident) => {
        #[doc = concat!("Returns cached hooks triggered when a component is `", stringify!($field), "`.")]
        #[inline(always)]
        pub fn $name1(&self) -> &'static [HookItem] {
            self.$field
        }

        #[doc = concat!("Triggers all `", stringify!($field), "` hooks for the given entity.")]
        #[inline(always)]
        pub fn $name2(&self, entity: EntityId, mut world: DeferredWorld, caller: DebugLocation) {
            for &(id, hook) in self.$field {
                hook(world.reborrow(), HookContext { id, entity, caller });
            }
        }
    };
}

impl Table {
    trigger_function!(on_add_hooks, trigger_on_add, on_add);
    trigger_function!(on_clone_hooks, trigger_on_clone, on_clone);
    trigger_function!(on_insert_hooks, trigger_on_insert, on_insert);
    trigger_function!(on_remove_hooks, trigger_on_remove, on_remove);
    trigger_function!(on_discard_hooks, trigger_on_discard, on_discard);
    trigger_function!(on_despawn_hooks, trigger_on_despawn, on_despawn);
}

// -----------------------------------------------------------------------------
// transition

impl Table {
    /// Returns the cached target table after inserting a bundle.
    pub fn after_insert(&self, bundle: BundleId) -> Option<TableId> {
        self.after_insert.get(&bundle).copied()
    }

    /// Returns the cached target table after removing a bundle.
    pub fn after_remove(&self, bundle: BundleId) -> Option<TableId> {
        self.after_remove.get(&bundle).copied()
    }

    /// Caches the target table for an insert-bundle transition.
    pub fn set_after_insert(&mut self, bundle: BundleId, arche: TableId) {
        self.after_insert.insert(bundle, arche);
    }

    /// Caches the target table for a remove-bundle transition.
    pub fn set_after_remove(&mut self, bundle: BundleId, arche: TableId) {
        self.after_remove.insert(bundle, arche);
    }
}

// -----------------------------------------------------------------------------
// Basic Methods 2

impl Table {
    /// Returns a reference to a column by its index.
    ///
    /// # Safety
    /// - `index` must be a valid column index for this table, as obtained
    ///   from [`Table::get_table_col`] or [`Table::get_type_col`]
    #[inline(always)]
    pub unsafe fn get_column(&self, index: TableCol) -> &Column {
        debug_assert!((index.0 as usize) < self.columns.len());
        unsafe { self.columns.get_unchecked(index.0 as usize) }
    }

    /// Returns a mutable reference to a column by its index.
    ///
    /// # Safety
    /// - `index` must be a valid column index for this table, as obtained
    ///   from [`Table::get_table_col`] or [`Table::get_type_col`]
    /// - No other references to the column may exist
    #[inline(always)]
    pub unsafe fn get_column_mut(&mut self, index: TableCol) -> &mut Column {
        debug_assert!((index.0 as usize) < self.columns.len());
        unsafe { self.columns.get_unchecked_mut(index.0 as usize) }
    }

    /// Return `EntityId` in the given row.
    ///
    /// # Safety
    /// - `table_row` must be a valid row index    
    #[inline(always)]
    pub unsafe fn get_entity(&self, table_row: TableRow) -> EntityId {
        debug_assert!((table_row.0 as usize) < self.entity_count());
        unsafe { *self.entities.get_unchecked(table_row.0 as usize) }
    }

    /// Returns a pointer to component data at the specified row and column.
    ///
    /// # Safety
    /// - `table_row` must be a valid row index
    /// - `table_col` must be a valid column index
    #[inline(always)]
    pub unsafe fn get_data(&self, table_row: TableRow, table_col: TableCol) -> Ptr<'_> {
        debug_assert!((table_row.0 as usize) < self.entity_count());
        unsafe {
            let col = self.get_column(table_col);
            col.get_data(table_row.0 as usize)
        }
    }

    /// Returns a pointer to component data at the specified row and column.
    ///
    /// # Safety
    /// - `table_row` must be a valid row index
    /// - `table_col` must be a valid column index
    #[inline(always)]
    pub unsafe fn get_data_mut(&mut self, table_row: TableRow, table_col: TableCol) -> PtrMut<'_> {
        debug_assert!((table_row.0 as usize) < self.entity_count());
        unsafe {
            let col = self.get_column_mut(table_col);
            col.get_data_mut(table_row.0 as usize)
        }
    }

    /// Returns the added tick for a component at the specified row and column.
    ///
    /// # Safety
    /// - `table_row` must be a valid row index
    /// - `table_col` must be a valid column index
    #[inline(always)]
    pub unsafe fn get_added(&self, table_row: TableRow, table_col: TableCol) -> Tick {
        debug_assert!((table_row.0 as usize) < self.entity_count());
        unsafe {
            let col = self.get_column(table_col);
            col.get_added(table_row.0 as usize)
        }
    }

    /// Returns the changed tick for a component at the specified row and column.
    ///
    /// # Safety
    /// - `table_row` must be a valid row index
    /// - `table_col` must be a valid column index
    #[inline(always)]
    pub unsafe fn get_changed(&self, table_row: TableRow, table_col: TableCol) -> Tick {
        debug_assert!((table_row.0 as usize) < self.entity_count());
        unsafe {
            let col = self.get_column(table_col);
            col.get_changed(table_row.0 as usize)
        }
    }

    /// Returns the added tick for a component at the specified row and column.
    ///
    /// # Safety
    /// - `table_row` must be a valid row index
    /// - `table_col` must be a valid column index
    #[inline(always)]
    pub unsafe fn get_added_mut(&mut self, table_row: TableRow, table_col: TableCol) -> &mut Tick {
        debug_assert!((table_row.0 as usize) < self.entity_count());
        unsafe {
            let col = self.get_column_mut(table_col);
            col.get_added_mut(table_row.0 as usize)
        }
    }

    /// Returns the changed tick for a component at the specified row and column.
    ///
    /// # Safety
    /// - `table_row` must be a valid row index
    /// - `table_col` must be a valid column index
    #[inline(always)]
    pub unsafe fn get_changed_mut(
        &mut self,
        table_row: TableRow,
        table_col: TableCol,
    ) -> &mut Tick {
        debug_assert!((table_row.0 as usize) < self.entity_count());
        unsafe {
            let col = self.get_column_mut(table_col);
            col.get_changed_mut(table_row.0 as usize)
        }
    }

    /// Returns a slice of added ticks for the entire column.
    ///
    /// # Safety
    /// - `table_col` must be a valid column index
    /// - The returned slice is only valid while the table is not mutated
    #[inline(always)]
    pub unsafe fn get_added_slice(&self, table_col: TableCol) -> &[Tick] {
        let len = self.entity_count();
        unsafe {
            let col = self.get_column(table_col);
            col.get_added_slice().deref(len)
        }
    }

    /// Returns a slice of changed ticks for the entire column.
    ///
    /// # Safety
    /// - `table_col` must be a valid column index
    /// - The returned slice is only valid while the table is not mutated
    #[inline(always)]
    pub unsafe fn get_changed_slice(&self, table_col: TableCol) -> &[Tick] {
        let len = self.entity_count();
        unsafe {
            let col = self.get_column(table_col);
            col.get_changed_slice().deref(len)
        }
    }

    /// Returns an untyped reference to a component with change tracking.
    ///
    /// # Safety
    /// - `table_row` and `table_col` must be valid
    /// - The component must be initialized at the given row
    #[inline]
    pub unsafe fn get_ref(
        &self,
        table_row: TableRow,
        table_col: TableCol,
        last_run: Tick,
        this_run: Tick,
    ) -> UntypedRef<'_> {
        debug_assert!((table_row.0 as usize) < self.entity_count());
        unsafe {
            let col = self.get_column(table_col);
            col.get_ref(table_row.0 as usize, last_run, this_run)
        }
    }

    /// Returns an untyped mutable reference to a component with change tracking.
    ///
    /// # Safety
    /// - `table_row` and `table_col` must be valid
    /// - The component must be initialized at the given row
    /// - No other references to the component may exist
    #[inline]
    pub unsafe fn get_mut(
        &mut self,
        table_row: TableRow,
        table_col: TableCol,
        last_run: Tick,
        this_run: Tick,
    ) -> UntypedMut<'_> {
        debug_assert!((table_row.0 as usize) < self.entity_count());
        unsafe {
            let col = self.get_column_mut(table_col);
            col.get_mut(table_row.0 as usize, last_run, this_run)
        }
    }

    /// Returns an untyped slice reference to an entire column with change tracking.
    ///
    /// # Safety
    /// - `table_col` must be a valid column index
    /// - All components in the column must be initialized
    #[inline]
    pub unsafe fn get_slice_ref(
        &self,
        table_col: TableCol,
        last_run: Tick,
        this_run: Tick,
    ) -> UntypedSliceRef<'_> {
        let len = self.entity_count();
        unsafe {
            let col = self.get_column(table_col);
            col.get_slice_ref(len, last_run, this_run)
        }
    }

    /// Returns an untyped mutable slice reference to an entire column with change tracking.
    ///
    /// # Safety
    /// - `table_col` must be a valid column index
    /// - All components in the column must be initialized
    /// - No other references to the column may exist
    #[inline]
    pub unsafe fn get_slice_mut(
        &mut self,
        table_col: TableCol,
        last_run: Tick,
        this_run: Tick,
    ) -> UntypedSliceMut<'_> {
        let len = self.entity_count();
        unsafe {
            let col = self.get_column_mut(table_col);
            col.get_slice_mut(len, last_run, this_run)
        }
    }

    /// Initializes a component at the specified row.
    ///
    /// # Safety
    /// - `table_row` and `table_col` must be valid
    /// - The component slot must be uninitialized
    /// - `data` must point to valid data matching the column's type
    #[inline]
    pub unsafe fn init_item(
        &mut self,
        table_col: TableCol,
        table_row: TableRow,
        data: OwningPtr<'_>,
        tick: Tick,
    ) {
        debug_assert!((table_row.0 as usize) < self.entity_count());

        unsafe {
            let column = self.get_column_mut(table_col);
            column.init_item(table_row.0 as usize, data, tick);
        }
    }

    /// Replaces an existing component at the specified row.
    ///
    /// # Safety
    /// - `table_row` and `table_col` must be valid
    /// - The component slot must be initialized
    /// - `data` must point to valid data matching the column's type
    #[inline]
    pub unsafe fn replace_item(
        &mut self,
        table_col: TableCol,
        table_row: TableRow,
        data: OwningPtr<'_>,
        tick: Tick,
    ) {
        debug_assert!((table_row.0 as usize) < self.entity_count());

        unsafe {
            let column = self.get_column_mut(table_col);
            column.replace_item(table_row.0 as usize, data, tick);
        }
    }

    /// Removes a component and returns ownership of its data.
    ///
    /// # Safety
    /// - `table_row` and `table_col` must be valid
    /// - The component slot must be initialized
    /// - Caller must ensure the returned `OwningPtr` is properly handled
    #[inline]
    #[must_use = "The returned pointer should be handled."]
    pub unsafe fn remove_item(
        &mut self,
        table_col: TableCol,
        table_row: TableRow,
    ) -> OwningPtr<'_> {
        debug_assert!((table_row.0 as usize) < self.entity_count());

        unsafe {
            let column = self.get_column_mut(table_col);
            column.remove_item(table_row.0 as usize)
        }
    }

    /// Drops the component data at the specified location without returning it.
    ///
    /// # Safety
    /// - `table_row` and `table_col` must be valid
    /// - The component slot must be initialized
    #[inline]
    pub unsafe fn drop_item(&mut self, table_col: TableCol, table_row: TableRow) {
        debug_assert!((table_row.0 as usize) < self.entity_count());

        unsafe {
            let column = self.get_column_mut(table_col);
            column.drop_item(table_row.0 as usize)
        }
    }
}

// -----------------------------------------------------------------------------
// Shrink

impl Table {
    /// Reduces the memory footprint of the table.
    ///
    /// The allocation is shrunk only when it is large enough to matter: if
    /// the table is empty the storage is fully deallocated, and otherwise
    /// the capacity is trimmed to a power-of-two near the current entity
    /// count.  This is a no-op for small tables.
    pub fn shrink(&mut self) {
        let len = self.entity_count();
        let cap = self.capacity();

        if len == 0 && cap != 0 {
            let abort_guard = AbortOnPanic;

            let current = unsafe { NonZeroUsize::new_unchecked(cap) };
            self.entities = Vec::new();

            for c in &mut self.columns {
                unsafe { c.shrink_to(current, 0) };
            }

            ::core::mem::forget(abort_guard);
            return;
        }

        if cap >= (len + (len << 1)).next_power_of_two() && cap >= 16 {
            let abort_guard = AbortOnPanic;

            let current = unsafe { NonZeroUsize::new_unchecked(cap) };
            let new_cap = len.next_power_of_two().max(len + (len >> 1));
            self.entities.shrink_to(new_cap);

            let new = self.entities.capacity();
            for c in &mut self.columns {
                unsafe { c.shrink_to(current, new) };
            }

            ::core::mem::forget(abort_guard);
        }
    }
}

// -----------------------------------------------------------------------------
// Alloc

impl Table {
    /// Allocates space for a new entity and returns its row index.
    ///
    /// The entity is appended to the table, growing the storage when
    /// necessary.  The component slots of the returned row are
    /// **uninitialized**: every component of this table must be written with
    /// [`Table::init_item`] before the row is observed by anything else.
    ///
    /// # Safety
    /// - The entity must not already be present in this table
    /// - The returned row is valid until the entity is removed or moved
    ///
    /// # Example
    ///
    /// ```ignore
    /// use zlim_core::prelude::*;
    /// use zlim_core::table::{Table, TableRow};
    ///
    /// // `alloc_row` is the low-level primitive behind spawning entities.
    /// // It is normally called by the entity storage layer, which already
    /// // holds a `&mut Table` from the world's `Tables` registry.
    /// let table: &mut Table = /* obtained from the registry */;
    /// let entity: EntityId = /* a freshly allocated entity */;
    ///
    /// // Append the entity and get its row.  The row's component slots are
    /// // uninitialized until every component is written.
    /// let row: TableRow = unsafe { table.alloc_row(entity) };
    /// ```
    #[inline]
    #[must_use = "The returned row should be initialized or dropped."]
    pub unsafe fn alloc_row(&mut self, entity: EntityId) -> TableRow {
        #[inline(never)]
        fn reserve_many(this: &mut Table) {
            let abort_guard = AbortOnPanic;

            let old_cap = this.entities.capacity();
            this.entities.reserve(1);
            let new_cap = this.entities.capacity();

            assert!(new_cap <= u32::MAX as usize, "too many entities in a Table");

            unsafe {
                let new_capacity = NonZeroUsize::new_unchecked(new_cap);
                if let Some(current) = NonZeroUsize::new(old_cap) {
                    this.columns.iter_mut().for_each(|col| {
                        col.realloc(current, new_capacity);
                    });
                } else {
                    this.columns
                        .iter_mut()
                        .for_each(|col| col.alloc(new_capacity));
                }
            }

            ::core::mem::forget(abort_guard);
        }

        let len = self.entities.len();
        if len == self.entities.capacity() {
            ::core::hint::cold_path();
            reserve_many(self);
        }

        self.entities.push(entity);
        // `0 < EntityId < u32::MAX`, so `len < u32::MAX`
        TableRow(len as u32)
    }
}

// -----------------------------------------------------------------------------
// Dealloc

impl Table {
    /// Removes an entity from the table by swapping it with the last row.
    ///
    /// The entity at `table_row` is removed using swap-remove semantics: the
    /// last row is moved into its place, so the removal is O(1) per column
    /// but reorders the table.  The returned [`MovedEntityRow`] reports which
    /// entity (if any) was displaced into `table_row`, so callers can update
    /// their row bookkeeping.
    ///
    /// The const parameter `DROP` controls what happens to the removed
    /// entity's components that exist only in this table:
    /// - `DROP = true` — the components are dropped in place;
    /// - `DROP = false` — the components are forgotten (not dropped); use
    ///   this when the components are being handled (or moved) elsewhere,
    ///   e.g. during despawn-without-drop.
    ///
    /// # Safety
    /// - `table_row` must be a valid, initialized row
    /// - After this operation, the row is no longer valid
    #[must_use = "The moved entity should be handled."]
    pub unsafe fn dealloc_row<const DROP: bool>(&mut self, table_row: TableRow) -> MovedEntityRow {
        let removal = table_row.0 as usize;
        let last = self.entity_count() - 1;
        debug_assert!(removal <= last);

        unsafe {
            if removal != last {
                let swapped = self.entities.move_last_to(last, removal);
                self.columns.iter_mut().for_each(|c| {
                    if DROP {
                        c.swap_drop_not_last(removal, last);
                    } else {
                        c.swap_forget_not_last(removal, last);
                    }
                });
                MovedEntityRow::Some {
                    entity: swapped,
                    new_row: table_row,
                }
            } else {
                core::hint::cold_path();
                self.entities.set_len(last);
                if DROP {
                    self.columns.iter_mut().for_each(|c| c.drop_item(last));
                }
                MovedEntityRow::None
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Move, Init data

impl Table {
    /// Moves an entity to another table.
    ///
    /// This is the low-level primitive behind component insertion and
    /// removal: the entity at `table_row` is swap-removed from this table,
    /// appended to `other`, and every component present in both tables is
    /// moved across.  Components that exist only in this table are either
    /// dropped (`DROP = true`) or forgotten (`DROP = false`), matching the
    /// semantics of [`Table::dealloc_row`].  Components of `other` that the
    /// source table lacks (e.g. newly inserted ones) are not touched here —
    /// the caller initializes them after the move.
    ///
    /// # Return value
    ///
    /// Returns a tuple `(MovedEntityRow, TableRow)`:
    /// - `MovedEntityRow` describes the displacement in the **source** table:
    ///   which entity was swapped into the vacated row (if any), so callers
    ///   can update the entity tree;
    /// - the `TableRow` is the entity's new row in `other`.
    ///
    /// # Safety
    /// - `table_row` must be a valid, initialized row in this table
    /// - `other` must be a different, valid table
    /// - `DROP` must match how the caller handles components that are absent
    ///   from `other` (dropped or forgotten)
    ///
    /// # Example
    ///
    /// ```ignore
    /// use zlim_core::prelude::*;
    /// use zlim_core::table::{MovedEntityRow, Table, TableRow};
    ///
    /// // `move_row` is normally called by the entity storage layer, which
    /// // resolves the destination table via the world's `Tables` registry
    /// // and holds `&mut Table` handles for both sides.
    /// let source: &mut Table = /* obtained from the registry */;
    /// let dest: &mut Table = /* obtained from the registry */;
    /// let row: TableRow = /* the entity's row in `source` */;
    ///
    /// let (moved, new_row): (MovedEntityRow, TableRow) =
    ///     unsafe { source.move_row::<true>(row, dest) };
    ///
    /// // `moved` tells us which entity (if any) now occupies the vacated
    /// // source row; `new_row` is the entity's row inside `dest`.
    /// ```
    #[must_use = "The moved entity should be handled."]
    pub unsafe fn move_row<const DROP: bool>(
        &mut self,
        table_row: TableRow,
        other: &mut Table,
    ) -> (MovedEntityRow, TableRow) {
        let src = table_row.0 as usize;
        let last = self.entity_count() - 1;
        debug_assert!(src <= last);

        unsafe {
            if src != last {
                let moved = *self.entities.get_unchecked(src);
                let swapped = self.entities.move_last_to(last, src);
                let new_row = other.alloc_row(moved);
                let dst = new_row.0 as usize;

                self.components
                    .iter()
                    .zip(self.columns.iter_mut())
                    .for_each(|(&id, col)| {
                        if let Some(table_col) = other.get_table_col(id) {
                            let other_col = other.get_column_mut(table_col);
                            col.move_item_to(other_col, src, dst);
                            col.swap_forget_not_last(src, last);
                        } else if DROP {
                            col.swap_drop_not_last(src, last);
                        } else {
                            col.swap_forget_not_last(src, last);
                        }
                    });

                (
                    MovedEntityRow::Some {
                        entity: swapped,
                        new_row: table_row,
                    },
                    new_row,
                )
            } else {
                core::hint::cold_path();
                let moved = self.entities.remove_last(last);
                let new_row = other.alloc_row(moved);
                let dst = new_row.0 as usize;

                self.components
                    .iter()
                    .zip(self.columns.iter_mut())
                    .for_each(|(&id, col)| {
                        if let Some(table_col) = other.get_table_col(id) {
                            let other_col = other.get_column_mut(table_col);
                            col.move_item_to(other_col, src, dst);
                        } else if DROP {
                            col.drop_item(last);
                        }
                    });

                (MovedEntityRow::None, new_row)
            }
        }
    }
}

// -----------------------------------------------------------------------------
