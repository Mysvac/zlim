use core::alloc::Layout;
use core::ptr::NonNull;
use core::slice;

use super::{QueryData, ReadOnlyQueryData};
use crate::borrow::{Mut, Ref, SliceMut, SliceRef};
use crate::component::{Component, ComponentId};
use crate::entity::EntityId;
use crate::query::QuerySlice;
use crate::system::{ComponentAccess, FilterParamBuilder};
use crate::table::{Column, Table, TableRow};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------
// ComponentView

/// Per-execution cache for a single component column.
///
/// Holds an optional pointer to the current table's column (set by
/// [`QueryData::update_table`]) together with the tick window of the current
/// system run, which is needed to attach change metadata to fetched
/// [`crate::borrow::Ref`]/[`crate::borrow::Mut`] items.
pub struct ComponentView {
    data: Option<NonNull<Column>>,
    last_run: Tick,
    this_run: Tick,
}

// -----------------------------------------------------------------------------
// &T

unsafe impl<T: Component> ReadOnlyQueryData for &T {}

unsafe impl<T: Component> QueryData for &T {
    type ReadOnly = Self;
    type State = ComponentId;
    type Cache<'world> = Option<NonNull<Column>>;
    type Item<'world> = &'world T;

    fn build_state(world: &World) -> Self::State {
        world.components.get::<T>().id
    }

    #[inline(always)]
    unsafe fn build_cache<'w>(
        _state: &Self::State,
        _world: WorldCell<'w>,
        _last_run: Tick,
        _this_run: Tick,
    ) -> Self::Cache<'w> {
        None
    }

    #[inline(never)]
    fn register_filter(state: &Self::State, out: &mut Vec<FilterParamBuilder>) {
        out.iter_mut().for_each(|param| param.with(*state));
    }

    #[inline(always)] // register_reading is already `inline(never)`
    fn register_access(state: &Self::State, out: &mut ComponentAccess) -> bool {
        out.register_reading(*state)
    }

    #[inline(never)]
    unsafe fn update_table<'w>(
        state: &Self::State,
        cache: &mut Self::Cache<'w>,
        table: &'w mut Table,
    ) {
        if let Some(col) = table.get_table_col(*state) {
            let column = unsafe { table.get_column(col) };
            *cache = Some(NonNull::from_ref(column));
        } else {
            *cache = None;
        }
    }

    #[cfg_attr(not(debug_assertions), inline)]
    #[cfg_attr(debug_assertions, inline(never))]
    unsafe fn fetch<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        _entity: EntityId,
        table_row: TableRow,
    ) -> Option<Self::Item<'w>> {
        let ptr = (*cache)?;
        let column = unsafe { &*ptr.as_ptr() };
        let row = table_row.0 as usize;
        let data = unsafe { column.get_data(row) };
        data.debug_assert_aligned::<T>();
        Some(unsafe { data.deref::<T>() })
    }
}

unsafe impl<T: Component> QuerySlice for &T {
    type SliceItem<'world> = &'world [T];

    type ReadOnlySlice = Self::ReadOnly;

    #[cfg_attr(not(debug_assertions), inline)]
    #[cfg_attr(debug_assertions, inline(never))]
    unsafe fn fetch_slice<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entities: &'w [EntityId],
    ) -> Option<Self::SliceItem<'w>> {
        let ptr = (*cache)?;
        let column = unsafe { &*ptr.as_ptr() };
        debug_assert_eq!(column.item_layout(), Layout::new::<T>());
        let data: &T = unsafe { column.get_data(0).deref::<T>() };
        Some(unsafe { slice::from_raw_parts::<T>(data, entities.len()) })
    }
}

// -----------------------------------------------------------------------------
// Option<&T>

unsafe impl<T: Component> ReadOnlyQueryData for Option<&T> {}

unsafe impl<T: Component> QueryData for Option<&T> {
    type ReadOnly = Self;
    type State = ComponentId;
    type Cache<'world> = Option<NonNull<Column>>;
    type Item<'world> = Option<&'world T>;

    fn build_state(world: &World) -> Self::State {
        world.components.get::<T>().id
    }

    #[inline(always)]
    unsafe fn build_cache<'w>(
        _state: &Self::State,
        _world: WorldCell<'w>,
        _last_run: Tick,
        _this_run: Tick,
    ) -> Self::Cache<'w> {
        None
    }

    #[inline(always)]
    fn register_filter(_state: &Self::State, _out: &mut Vec<FilterParamBuilder>) {
        // Because `Option`, we do not set filter.
    }

    #[inline(always)] // register_reading is already `inline(never)`
    fn register_access(state: &Self::State, out: &mut ComponentAccess) -> bool {
        out.register_reading(*state)
    }

    #[inline(never)]
    unsafe fn update_table<'w>(
        state: &Self::State,
        cache: &mut Self::Cache<'w>,
        table: &'w mut Table,
    ) {
        if let Some(col) = table.get_table_col(*state) {
            let column = unsafe { table.get_column(col) };
            *cache = Some(NonNull::from_ref(column));
        } else {
            *cache = None;
        }
    }

    #[cfg_attr(not(debug_assertions), inline)]
    #[cfg_attr(debug_assertions, inline(never))]
    unsafe fn fetch<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        _entity: EntityId,
        table_row: TableRow,
    ) -> Option<Self::Item<'w>> {
        let Some(ptr) = *cache else {
            return Some(None);
        };
        let column = unsafe { &*ptr.as_ptr() };
        let row = table_row.0 as usize;
        let data = unsafe { column.get_data(row) };
        data.debug_assert_aligned::<T>();
        Some(Some(unsafe { data.deref::<T>() }))
    }
}

unsafe impl<T: Component> QuerySlice for Option<&T> {
    type SliceItem<'world> = Option<&'world [T]>;

    type ReadOnlySlice = Self::ReadOnly;

    #[cfg_attr(not(debug_assertions), inline)]
    #[cfg_attr(debug_assertions, inline(never))]
    unsafe fn fetch_slice<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entities: &'w [EntityId],
    ) -> Option<Self::SliceItem<'w>> {
        let Some(ptr) = *cache else {
            return Some(None);
        };
        let column = unsafe { &*ptr.as_ptr() };
        debug_assert_eq!(column.item_layout(), Layout::new::<T>());
        let data: &T = unsafe { column.get_data(0).deref::<T>() };
        Some(Some(unsafe {
            slice::from_raw_parts::<T>(data, entities.len())
        }))
    }
}

// -----------------------------------------------------------------------------
// Ref

unsafe impl<T: Component> ReadOnlyQueryData for Ref<'_, T> {}

unsafe impl<T: Component> QueryData for Ref<'_, T> {
    type ReadOnly = Self;
    type State = ComponentId;
    type Cache<'world> = ComponentView;
    type Item<'world> = Ref<'world, T>;

    fn build_state(world: &World) -> Self::State {
        world.components.get::<T>().id
    }

    #[inline(always)]
    unsafe fn build_cache<'w>(
        _state: &Self::State,
        _world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Self::Cache<'w> {
        ComponentView {
            data: None,
            last_run,
            this_run,
        }
    }

    #[inline(never)]
    fn register_filter(state: &Self::State, out: &mut Vec<FilterParamBuilder>) {
        out.iter_mut().for_each(|param| param.with(*state));
    }

    #[inline(always)] // register_reading is already `inline(never)`
    fn register_access(state: &Self::State, out: &mut ComponentAccess) -> bool {
        out.register_reading(*state)
    }

    #[inline(never)]
    unsafe fn update_table<'w>(
        state: &Self::State,
        cache: &mut Self::Cache<'w>,
        table: &'w mut Table,
    ) {
        if let Some(col) = table.get_table_col(*state) {
            let column = unsafe { table.get_column(col) };
            cache.data = Some(NonNull::from_ref(column));
        } else {
            cache.data = None;
        }
    }

    #[cfg_attr(not(debug_assertions), inline)]
    #[cfg_attr(debug_assertions, inline(never))]
    unsafe fn fetch<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        _entity: EntityId,
        table_row: TableRow,
    ) -> Option<Self::Item<'w>> {
        let last_run = cache.last_run;
        let this_run = cache.this_run;
        let ptr = cache.data?;
        let column = unsafe { &*ptr.as_ptr() };
        let row = table_row.0 as usize;
        let data = unsafe { column.get_ref(row, last_run, this_run) };
        Some(unsafe { data.with_type::<T>() })
    }
}

unsafe impl<T: Component> QuerySlice for Ref<'_, T> {
    type SliceItem<'world> = SliceRef<'world, T>;

    type ReadOnlySlice = Self::ReadOnly;

    #[cfg_attr(not(debug_assertions), inline)]
    #[cfg_attr(debug_assertions, inline(never))]
    unsafe fn fetch_slice<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entities: &'w [EntityId],
    ) -> Option<Self::SliceItem<'w>> {
        let last_run = cache.last_run;
        let this_run = cache.this_run;
        let ptr = cache.data?;
        let column = unsafe { &*ptr.as_ptr() };
        let data = unsafe { column.get_slice_ref(entities.len(), last_run, this_run) };
        Some(unsafe { data.with_type::<T>() })
    }
}

// -----------------------------------------------------------------------------
// Option<Ref<'_, T>>

unsafe impl<T: Component> ReadOnlyQueryData for Option<Ref<'_, T>> {}

unsafe impl<T: Component> QueryData for Option<Ref<'_, T>> {
    type ReadOnly = Self;
    type State = ComponentId;
    type Cache<'world> = ComponentView;
    type Item<'world> = Option<Ref<'world, T>>;

    fn build_state(world: &World) -> Self::State {
        world.components.get::<T>().id
    }

    #[inline(always)]
    unsafe fn build_cache<'w>(
        _state: &Self::State,
        _world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Self::Cache<'w> {
        ComponentView {
            data: None,
            last_run,
            this_run,
        }
    }

    #[inline(always)]
    fn register_filter(_state: &Self::State, _out: &mut Vec<FilterParamBuilder>) {
        // Because `Option`, we do not set filter.
    }

    #[inline(always)] // register_reading is already `inline(never)`
    fn register_access(state: &Self::State, out: &mut ComponentAccess) -> bool {
        out.register_reading(*state)
    }

    #[inline(never)]
    unsafe fn update_table<'w>(
        state: &Self::State,
        cache: &mut Self::Cache<'w>,
        table: &'w mut Table,
    ) {
        if let Some(col) = table.get_table_col(*state) {
            let column = unsafe { table.get_column(col) };
            cache.data = Some(NonNull::from_ref(column));
        } else {
            cache.data = None;
        }
    }

    #[cfg_attr(not(debug_assertions), inline)]
    #[cfg_attr(debug_assertions, inline(never))]
    unsafe fn fetch<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        _entity: EntityId,
        table_row: TableRow,
    ) -> Option<Self::Item<'w>> {
        let last_run = cache.last_run;
        let this_run = cache.this_run;
        let Some(ptr) = cache.data else {
            return Some(None);
        };
        let column = unsafe { &*ptr.as_ptr() };
        let row = table_row.0 as usize;
        let data = unsafe { column.get_ref(row, last_run, this_run) };
        Some(Some(unsafe { data.with_type::<T>() }))
    }
}

unsafe impl<T: Component> QuerySlice for Option<Ref<'_, T>> {
    type SliceItem<'world> = Option<SliceRef<'world, T>>;

    type ReadOnlySlice = Self::ReadOnly;

    #[cfg_attr(not(debug_assertions), inline)]
    #[cfg_attr(debug_assertions, inline(never))]
    unsafe fn fetch_slice<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entities: &'w [EntityId],
    ) -> Option<Self::SliceItem<'w>> {
        let last_run = cache.last_run;
        let this_run = cache.this_run;
        let Some(ptr) = cache.data else {
            return Some(None);
        };
        let column = unsafe { &*ptr.as_ptr() };
        let data = unsafe { column.get_slice_ref(entities.len(), last_run, this_run) };
        Some(Some(unsafe { data.with_type::<T>() }))
    }
}

// -----------------------------------------------------------------------------
// &mut T

unsafe impl<T: Component> QueryData for &mut T {
    // Downgrade to Ref<T> to preserve change-tick metadata in read-only mode.
    type ReadOnly = Ref<'static, T>;
    type State = ComponentId;
    type Cache<'world> = ComponentView;
    type Item<'world> = Mut<'world, T>;

    fn build_state(world: &World) -> Self::State {
        world.components.get::<T>().id
    }

    #[inline(always)]
    unsafe fn build_cache<'w>(
        _state: &Self::State,
        _world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Self::Cache<'w> {
        ComponentView {
            data: None,
            last_run,
            this_run,
        }
    }

    #[inline(never)]
    fn register_filter(state: &Self::State, out: &mut Vec<FilterParamBuilder>) {
        out.iter_mut().for_each(|param| param.with(*state));
    }

    #[inline(always)] // register_reading is already `inline(never)`
    fn register_access(state: &Self::State, out: &mut ComponentAccess) -> bool {
        out.register_writing(*state)
    }

    #[inline(never)]
    unsafe fn update_table<'w>(
        state: &Self::State,
        cache: &mut Self::Cache<'w>,
        table: &'w mut Table,
    ) {
        if let Some(col) = table.get_table_col(*state) {
            let column = unsafe { table.get_column_mut(col) };
            cache.data = Some(NonNull::from_mut(column));
        } else {
            cache.data = None;
        }
    }

    #[cfg_attr(not(debug_assertions), inline)]
    #[cfg_attr(debug_assertions, inline(never))]
    unsafe fn fetch<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        _entity: EntityId,
        table_row: TableRow,
    ) -> Option<Self::Item<'w>> {
        let last_run = cache.last_run;
        let this_run = cache.this_run;
        let ptr = cache.data?;
        let column = unsafe { &mut *ptr.as_ptr() };
        let row = table_row.0 as usize;
        let data = unsafe { column.get_mut(row, last_run, this_run) };
        Some(unsafe { data.with_type::<T>() })
    }
}

unsafe impl<T: Component> QuerySlice for &mut T {
    type SliceItem<'world> = SliceMut<'world, T>;

    type ReadOnlySlice = Self::ReadOnly;

    #[cfg_attr(not(debug_assertions), inline)]
    #[cfg_attr(debug_assertions, inline(never))]
    unsafe fn fetch_slice<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entities: &'w [EntityId],
    ) -> Option<Self::SliceItem<'w>> {
        let last_run = cache.last_run;
        let this_run = cache.this_run;
        let ptr = cache.data?;
        let column = unsafe { &mut *ptr.as_ptr() };
        let data = unsafe { column.get_slice_mut(entities.len(), last_run, this_run) };
        Some(unsafe { data.with_type::<T>() })
    }
}

// -----------------------------------------------------------------------------
// Option<&mut T>

unsafe impl<T: Component> QueryData for Option<&mut T> {
    type ReadOnly = Option<Ref<'static, T>>;
    type State = ComponentId;
    type Cache<'world> = ComponentView;
    type Item<'world> = Option<Mut<'world, T>>;

    fn build_state(world: &World) -> Self::State {
        world.components.get::<T>().id
    }

    #[inline(always)]
    unsafe fn build_cache<'w>(
        _state: &Self::State,
        _world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Self::Cache<'w> {
        ComponentView {
            data: None,
            last_run,
            this_run,
        }
    }

    #[inline(always)]
    fn register_filter(_state: &Self::State, _out: &mut Vec<FilterParamBuilder>) {
        // Because `Option`, we do not set filter.
    }

    #[inline(always)] // register_reading is already `inline(never)`
    fn register_access(state: &Self::State, out: &mut ComponentAccess) -> bool {
        out.register_writing(*state)
    }

    #[inline(never)]
    unsafe fn update_table<'w>(
        state: &Self::State,
        cache: &mut Self::Cache<'w>,
        table: &'w mut Table,
    ) {
        if let Some(col) = table.get_table_col(*state) {
            let column = unsafe { table.get_column_mut(col) };
            cache.data = Some(NonNull::from_mut(column));
        } else {
            cache.data = None;
        }
    }

    #[cfg_attr(not(debug_assertions), inline)]
    #[cfg_attr(debug_assertions, inline(never))]
    unsafe fn fetch<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        _entity: EntityId,
        table_row: TableRow,
    ) -> Option<Self::Item<'w>> {
        let last_run = cache.last_run;
        let this_run = cache.this_run;
        let Some(ptr) = cache.data else {
            return Some(None);
        };
        let column = unsafe { &mut *ptr.as_ptr() };
        let row = table_row.0 as usize;
        let data = unsafe { column.get_mut(row, last_run, this_run) };
        Some(Some(unsafe { data.with_type::<T>() }))
    }
}

unsafe impl<T: Component> QuerySlice for Option<&mut T> {
    type SliceItem<'world> = Option<SliceMut<'world, T>>;

    type ReadOnlySlice = Self::ReadOnly;

    #[cfg_attr(not(debug_assertions), inline)]
    #[cfg_attr(debug_assertions, inline(never))]
    unsafe fn fetch_slice<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entities: &'w [EntityId],
    ) -> Option<Self::SliceItem<'w>> {
        let last_run = cache.last_run;
        let this_run = cache.this_run;
        let Some(ptr) = cache.data else {
            return Some(None);
        };
        let column = unsafe { &mut *ptr.as_ptr() };
        let data = unsafe { column.get_slice_mut(entities.len(), last_run, this_run) };
        Some(Some(unsafe { data.with_type::<T>() }))
    }
}

// -----------------------------------------------------------------------------
// Mut

unsafe impl<T: Component> QueryData for Mut<'_, T> {
    type ReadOnly = Ref<'static, T>;
    type State = ComponentId;
    type Cache<'world> = ComponentView;
    type Item<'world> = Mut<'world, T>;

    fn build_state(world: &World) -> Self::State {
        world.components.get::<T>().id
    }

    #[inline(always)]
    unsafe fn build_cache<'w>(
        _state: &Self::State,
        _world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Self::Cache<'w> {
        ComponentView {
            data: None,
            last_run,
            this_run,
        }
    }

    #[inline(never)]
    fn register_filter(state: &Self::State, out: &mut Vec<FilterParamBuilder>) {
        out.iter_mut().for_each(|param| param.with(*state));
    }

    #[inline(always)] // register_reading is already `inline(never)`
    fn register_access(state: &Self::State, out: &mut ComponentAccess) -> bool {
        out.register_writing(*state)
    }

    #[inline(never)]
    unsafe fn update_table<'w>(
        state: &Self::State,
        cache: &mut Self::Cache<'w>,
        table: &'w mut Table,
    ) {
        if let Some(col) = table.get_table_col(*state) {
            let column = unsafe { table.get_column_mut(col) };
            cache.data = Some(NonNull::from_mut(column));
        } else {
            cache.data = None;
        }
    }

    #[cfg_attr(not(debug_assertions), inline)]
    #[cfg_attr(debug_assertions, inline(never))]
    unsafe fn fetch<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        _entity: EntityId,
        table_row: TableRow,
    ) -> Option<Self::Item<'w>> {
        let last_run = cache.last_run;
        let this_run = cache.this_run;
        let ptr = cache.data?;
        let column = unsafe { &mut *ptr.as_ptr() };
        let row = table_row.0 as usize;
        let data = unsafe { column.get_mut(row, last_run, this_run) };
        Some(unsafe { data.with_type::<T>() })
    }
}

unsafe impl<T: Component> QuerySlice for Mut<'_, T> {
    type SliceItem<'world> = SliceMut<'world, T>;

    type ReadOnlySlice = Self::ReadOnly;

    #[cfg_attr(not(debug_assertions), inline)]
    #[cfg_attr(debug_assertions, inline(never))]
    unsafe fn fetch_slice<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entities: &'w [EntityId],
    ) -> Option<Self::SliceItem<'w>> {
        let last_run = cache.last_run;
        let this_run = cache.this_run;
        let ptr = cache.data?;
        let column = unsafe { &mut *ptr.as_ptr() };
        let data = unsafe { column.get_slice_mut(entities.len(), last_run, this_run) };
        Some(unsafe { data.with_type::<T>() })
    }
}

// -----------------------------------------------------------------------------
// Option<Mut<'_, T>>

unsafe impl<T: Component> QueryData for Option<Mut<'_, T>> {
    type ReadOnly = Option<Ref<'static, T>>;
    type State = ComponentId;
    type Cache<'world> = ComponentView;
    type Item<'world> = Option<Mut<'world, T>>;

    fn build_state(world: &World) -> Self::State {
        world.components.get::<T>().id
    }

    #[inline(always)]
    unsafe fn build_cache<'w>(
        _state: &Self::State,
        _world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Self::Cache<'w> {
        ComponentView {
            data: None,
            last_run,
            this_run,
        }
    }

    #[inline(always)]
    fn register_filter(_state: &Self::State, _out: &mut Vec<FilterParamBuilder>) {
        // Because `Option`, we do not set filter.
    }

    #[inline(always)] // register_reading is already `inline(never)`
    fn register_access(state: &Self::State, out: &mut ComponentAccess) -> bool {
        out.register_writing(*state)
    }

    #[inline(never)]
    unsafe fn update_table<'w>(
        state: &Self::State,
        cache: &mut Self::Cache<'w>,
        table: &'w mut Table,
    ) {
        if let Some(col) = table.get_table_col(*state) {
            let column = unsafe { table.get_column_mut(col) };
            cache.data = Some(NonNull::from_mut(column));
        } else {
            cache.data = None;
        }
    }

    #[cfg_attr(not(debug_assertions), inline)]
    #[cfg_attr(debug_assertions, inline(never))]
    unsafe fn fetch<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        _entity: EntityId,
        table_row: TableRow,
    ) -> Option<Self::Item<'w>> {
        let last_run = cache.last_run;
        let this_run = cache.this_run;
        let Some(ptr) = cache.data else {
            return Some(None);
        };
        let column = unsafe { &mut *ptr.as_ptr() };
        let row = table_row.0 as usize;
        let data = unsafe { column.get_mut(row, last_run, this_run) };
        Some(Some(unsafe { data.with_type::<T>() }))
    }
}

unsafe impl<T: Component> QuerySlice for Option<Mut<'_, T>> {
    type SliceItem<'world> = Option<SliceMut<'world, T>>;

    type ReadOnlySlice = Self::ReadOnly;

    #[cfg_attr(not(debug_assertions), inline)]
    #[cfg_attr(debug_assertions, inline(never))]
    unsafe fn fetch_slice<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entities: &'w [EntityId],
    ) -> Option<Self::SliceItem<'w>> {
        let last_run = cache.last_run;
        let this_run = cache.this_run;
        let Some(ptr) = cache.data else {
            return Some(None);
        };
        let column = unsafe { &mut *ptr.as_ptr() };
        let data = unsafe { column.get_slice_mut(entities.len(), last_run, this_run) };
        Some(Some(unsafe { data.with_type::<T>() }))
    }
}
