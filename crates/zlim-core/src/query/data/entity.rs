use core::ptr::NonNull;

use super::{QueryData, ReadOnlyQueryData};
use crate::entity::{EntityId, Location};
use crate::ops::{EntityMut, EntityRef};
use crate::query::QuerySlice;
use crate::system::{ComponentAccess, FilterParamBuilder};
use crate::table::{Table, TableRow};
use crate::tick::Tick;
use crate::utils::DebugCheckedUnwrap;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------
// Entity

unsafe impl QueryData for EntityId {
    type ReadOnly = Self;
    type State = ();
    type Cache<'world> = ();
    type Item<'world> = EntityId;

    #[inline(always)]
    fn build_state(_world: &World) -> Self::State {}

    #[inline(always)]
    unsafe fn build_cache<'w>(
        _: &Self::State,
        _: WorldCell<'w>,
        _: Tick,
        _: Tick,
    ) -> Self::Cache<'w> {
    }

    #[inline(always)]
    fn register_filter(_state: &Self::State, _out: &mut Vec<FilterParamBuilder>) {}

    #[inline(always)]
    fn register_access(_state: &Self::State, _out: &mut ComponentAccess) -> bool {
        true // We did not access any components
    }

    #[inline(always)]
    unsafe fn update_table<'w>(_: &Self::State, _: &mut Self::Cache<'w>, _: &'w mut Table) {}

    #[inline(always)]
    unsafe fn fetch<'w>(
        _state: &Self::State,
        _cache: &mut Self::Cache<'w>,
        entity: EntityId,
        _table_row: TableRow,
    ) -> Option<Self::Item<'w>> {
        Some(entity)
    }
}

unsafe impl ReadOnlyQueryData for EntityId {}

unsafe impl QuerySlice for EntityId {
    type SliceItem<'world> = &'world [EntityId];

    type ReadOnlySlice = Self::ReadOnly;

    #[inline(always)]
    unsafe fn fetch_slice<'w>(
        _state: &Self::State,
        _cache: &mut Self::Cache<'w>,
        entities: &'w [EntityId],
    ) -> Option<Self::SliceItem<'w>> {
        Some(entities)
    }
}

// -----------------------------------------------------------------------------
// EntityRef

/// Per-execution cache for entity handles.
///
/// Holds an optional pointer to the current table (set by
/// [`QueryData::update_table`]) together with the tick window of the current
/// system run, which is needed to attach change metadata to the fetched
/// [`EntityRef`]/[`EntityMut`] handles.
pub struct EntityView {
    table: Option<NonNull<Table>>,
    last_run: Tick,
    this_run: Tick,
}

unsafe impl QueryData for EntityRef<'_> {
    type ReadOnly = Self;
    type State = ();
    type Cache<'world> = EntityView;
    type Item<'world> = EntityRef<'world>;

    #[inline(always)]
    fn build_state(_world: &World) -> Self::State {}

    #[inline(always)]
    unsafe fn build_cache<'w>(
        _state: &Self::State,
        _world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Self::Cache<'w> {
        EntityView {
            table: None,
            last_run,
            this_run,
        }
    }

    #[inline(always)]
    fn register_filter(_state: &Self::State, _out: &mut Vec<FilterParamBuilder>) {}

    #[inline(always)]
    fn register_access(_state: &Self::State, out: &mut ComponentAccess) -> bool {
        out.register_entity_ref()
    }

    #[inline(always)]
    unsafe fn update_table<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        table: &'w mut Table,
    ) {
        cache.table = Some(NonNull::from_ref(table));
    }

    unsafe fn fetch<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entity: EntityId,
        table_row: TableRow,
    ) -> Option<Self::Item<'w>> {
        // SAFETY: `update_table` should be called.
        let table: &Table = unsafe { cache.table.debug_checked_unwrap().as_ref() };

        debug_assert_eq!(entity, unsafe { table.get_entity(table_row) });
        let table_id = table.id();
        let location = Location {
            table_id,
            table_row,
        };

        Some(EntityRef {
            id: entity,
            table,
            location,
            last_run: cache.last_run,
            this_run: cache.this_run,
        })
    }
}

unsafe impl ReadOnlyQueryData for EntityRef<'_> {}

// -----------------------------------------------------------------------------
// EntityMut

unsafe impl QueryData for EntityMut<'_> {
    type ReadOnly = EntityRef<'static>;
    type State = ();
    type Cache<'world> = EntityView;
    type Item<'world> = EntityMut<'world>;

    #[inline(always)]
    fn build_state(_world: &World) -> Self::State {}

    #[inline(always)]
    unsafe fn build_cache<'w>(
        _state: &Self::State,
        _world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Self::Cache<'w> {
        EntityView {
            table: None,
            last_run,
            this_run,
        }
    }

    #[inline(always)]
    fn register_filter(_state: &Self::State, _out: &mut Vec<FilterParamBuilder>) {}

    #[inline(always)]
    fn register_access(_state: &Self::State, out: &mut ComponentAccess) -> bool {
        out.register_entity_mut()
    }

    #[inline(always)]
    unsafe fn update_table<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        table: &'w mut Table,
    ) {
        cache.table = Some(NonNull::from_mut(table));
    }

    unsafe fn fetch<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entity: EntityId,
        table_row: TableRow,
    ) -> Option<Self::Item<'w>> {
        // SAFETY: `update_table` should be called.
        let table: &mut Table = unsafe { cache.table.debug_checked_unwrap().as_mut() };

        debug_assert_eq!(entity, unsafe { table.get_entity(table_row) });
        let table_id = table.id();
        let location = Location {
            table_id,
            table_row,
        };

        Some(EntityMut {
            id: entity,
            table,
            location,
            last_run: cache.last_run,
            this_run: cache.this_run,
        })
    }
}

// -----------------------------------------------------------------------------
