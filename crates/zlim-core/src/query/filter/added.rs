//! The `Added` query filter — matches entities whose component was added.

use super::QueryFilter;
use crate::component::{Component, ComponentId};
use crate::entity::EntityId;
use crate::system::{AccessTable, ComponentAccess, FilterParamBuilder};
use crate::table::{Table, TableRow};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------
// Added

/// Query filter that matches entities whose component `T` was added in the
/// current system run interval.
///
/// This checks whether the component's added tick is newer than
/// `(last_run, this_run]`.
///
/// Notes:
/// - The filter only matches entities that currently contain `T`.
/// - It applies entity-level filtering at iteration time.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Health(u32);
///
/// fn only_new_health(query: Query<EntityId, Added<Health>>) {
///     for _ in query {
///         // Entities where `Health` was added since last run.
///     }
/// }
///
/// let mut world = World::alloc();
/// world.spawn(Health(10), None);
///
/// only_new_health(world.query::<EntityId, Added<Health>>());
/// assert_eq!(world.query::<EntityId, Added<Health>>().iter().count(), 1);
/// ```
pub struct Added<T: Component>(T);

// -----------------------------------------------------------------------------
// QueryFilter implementation

/// Per-execution cache for [`Added`] — the table's added-tick slice.
pub struct AddedView<'w> {
    /// Added-tick column of the current table, `None` if `T` is absent.
    ticks: Option<&'w [Tick]>,
    last_run: Tick,
    this_run: Tick,
}

unsafe impl<T: Component> QueryFilter for Added<T> {
    type State = ComponentId;
    type Cache<'world> = AddedView<'world>;

    const ENABLE_ENTITY_FILTER: bool = true;

    fn build_state(world: &World) -> Self::State {
        world.components.get::<T>().id
    }

    unsafe fn build_cache<'w>(
        _state: &Self::State,
        _world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Self::Cache<'w> {
        AddedView {
            ticks: None,
            last_run,
            this_run,
        }
    }

    fn register_filter(state: &Self::State, out: &mut Vec<FilterParamBuilder>) {
        let mut builder = FilterParamBuilder::new();
        builder.with(*state);
        out.push(builder);
    }

    fn register_access(state: &Self::State, out: &mut ComponentAccess) {
        // Reading the added-tick metadata of `T`; tick reads never conflict
        // with data access, so it is force-registered.
        out.force_reading(*state);
    }

    #[inline(always)]
    fn modify_access_table(_: &Self::State, _: &mut AccessTable, _: bool) -> bool {
        true
    }

    unsafe fn update_table<'w>(state: &Self::State, cache: &mut Self::Cache<'w>, table: &'w Table) {
        let Some(col) = table.get_table_col(*state) else {
            cache.ticks = None;
            return;
        };

        // SAFETY: `col` is a valid table column for this table (obtained from
        // `get_table_col`), and the returned slice borrows from `table`.
        cache.ticks = Some(unsafe { table.get_added_slice(col) });
    }

    unsafe fn filter<'w>(
        _state: &Self::State,
        cache: &mut Self::Cache<'w>,
        _entity: EntityId,
        table_row: TableRow,
    ) -> bool {
        let Some(ticks) = cache.ticks else {
            return false;
        };

        // SAFETY: the query iterator only visits rows within the current
        // table's bounds.
        let added = unsafe { *ticks.get_unchecked(table_row.0 as usize) };
        added.is_newer_than(cache.last_run, cache.this_run)
    }
}

// -----------------------------------------------------------------------------
