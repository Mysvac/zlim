use core::iter::FusedIterator;

use super::{QueryData, QueryFilter, QueryState};
use crate::entity::EntityId;
use crate::query::{ArchetypeFilter, QuerySlice};
use crate::table::{Table, TableId, TableRow};
use crate::tick::Tick;
use crate::world::WorldCell;

// -----------------------------------------------------------------------------
// QueryIter

/// Row-by-row iterator over the entities matched by a query.
///
/// Created by [`Query`](crate::query::Query) (via `into_iter` / [`iter`])
/// or directly by [`QueryIter::new`].  Iterates each matched table in order,
/// refreshing the [`QueryData`]/[`QueryFilter`] caches per table and yielding
/// one [`QueryData::Item`] per entity that passes the filters.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Position { x: f32, y: f32 }
///
/// fn total_x(query: Query<&Position>) -> f32 {
///     // Both `query.iter()` and `for pos in query` return a `QueryIter`.
///     query.iter().map(|pos| pos.x).sum()
/// }
///
/// let mut world = World::alloc();
/// world.spawn(Position { x: 1.0, y: 2.0 }, None);
/// world.spawn(Position { x: 3.0, y: 4.0 }, None);
///
/// assert_eq!(total_x(world.query::<&Position, ()>()), 4.0);
/// ```
///
/// [`iter`]: crate::query::Query::iter
pub struct QueryIter<'w, 's, D: QueryData, F: QueryFilter> {
    world: WorldCell<'w>,
    state: &'s QueryState<D, F>,
    d_cache: D::Cache<'w>,
    f_cache: F::Cache<'w>,
    storages: core::slice::Iter<'s, TableId>,
    entities: &'w [EntityId],
    row: usize,
}

impl<D: QueryData, F: QueryFilter> QueryIter<'_, '_, D, F> {
    /// # Safety
    /// - `world` must be the same world used to build `state`.
    /// - `state` must have access registrations compatible with `D`/`F`.
    /// - `last_run`/`this_run` must belong to the same world tick stream.
    /// - Caller must ensure no aliasing violations are introduced through
    ///   concurrent mutable iteration paths.
    pub unsafe fn new<'w, 's>(
        world: WorldCell<'w>,
        state: &'s QueryState<D, F>,
        last_run: Tick,
        this_run: Tick,
    ) -> QueryIter<'w, 's, D, F> {
        unsafe {
            QueryIter {
                world,
                state,
                d_cache: D::build_cache(&state.d_state, world, last_run, this_run),
                f_cache: F::build_cache(&state.f_state, world, last_run, this_run),
                storages: state.storages.iter(),
                entities: &[],
                row: 0,
            }
        }
    }

    /// Advances to the next non-empty storage slice and refreshes caches.
    ///
    /// Returns `None` when no storage remains.
    #[inline(never)]
    fn update_slice(&mut self) -> Option<()> {
        self.row = 0;
        loop {
            let table_id: TableId = *self.storages.next()?;
            let tables = unsafe { &mut self.world.data_mut().tables };
            let table = unsafe { tables.get_unchecked_mut(table_id) };
            let ptr = table as *mut Table;
            self.entities = unsafe { (&*ptr).entities() };
            if !self.entities.is_empty() {
                unsafe {
                    D::update_table(&self.state.d_state, &mut self.d_cache, &mut *ptr);
                    F::update_table(&self.state.f_state, &mut self.f_cache, &*ptr);
                }
                return Some(());
            }
        }
    }
}

impl<'w, D: QueryData, F: QueryFilter> Iterator for QueryIter<'w, '_, D, F> {
    type Item = D::Item<'w>;

    fn next(&mut self) -> Option<Self::Item> {
        'looper: loop {
            if self.row >= self.entities.len() {
                // If there is no entities, `update_slice` will return None.
                // otherwise `self.entities` is not empty after this function.
                core::hint::cold_path();
                self.update_slice()?;
            }
            // we cannot storage old_row before `update_slice`,
            // because it will reset `self.row` always.
            let old_row = self.row;
            let table_row = TableRow(old_row as u32);

            let entity = unsafe { *self.entities.get_unchecked(old_row) };
            // the number of entities < u32::MAX, the row will never overflow.
            self.row += 1;

            // Important optimization: skip entity filtering when the filter
            // type guarantees no entity-level checks are needed.
            if F::ENABLE_ENTITY_FILTER {
                let f_state = &self.state.f_state;
                let f_cache = &mut self.f_cache;
                if unsafe { !F::filter(f_state, f_cache, entity, table_row) } {
                    continue 'looper;
                }
            }

            let d_state = &self.state.d_state;
            let d_cache = &mut self.d_cache;
            if let Some(data) = unsafe { D::fetch(d_state, d_cache, entity, table_row) } {
                return Some(data);
            }
        }
    }
}

impl<D: QueryData, F: QueryFilter> FusedIterator for QueryIter<'_, '_, D, F> {}

// -----------------------------------------------------------------------------
// QuerySliceIter

/// Slice-based iterator over the entities matched by a query.
///
/// Created by [`Query::iter_slice`](crate::query::Query::iter_slice) (or
/// `iter_slice_mut`) for dense queries whose data implements [`QuerySlice`].
/// Instead of yielding one item per entity, each step yields the whole
/// contiguous component column of the current table as a slice, which is
/// faster for bulk processing.  Only [`ArchetypeFilter`]s are allowed, since
/// per-entity predicates cannot be applied to a whole-slice fetch.
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
/// fn max_health(query: Query<&Health>) -> u32 {
///     query
///         .iter_slice()
///         .flat_map(|healths| healths.iter())
///         .map(|h| h.0)
///         .max()
///         .unwrap_or(0)
/// }
///
/// let mut world = World::alloc();
/// world.spawn(Health(30), None);
/// world.spawn(Health(100), None);
///
/// assert_eq!(max_health(world.query::<&Health, ()>()), 100);
/// ```
pub struct QuerySliceIter<'w, 's, D: QuerySlice, F: ArchetypeFilter> {
    world: WorldCell<'w>,
    state: &'s QueryState<D, F>,
    d_cache: D::Cache<'w>,
    f_cache: F::Cache<'w>,
    storages: core::slice::Iter<'s, TableId>,
    entities: &'w [EntityId],
}

impl<D: QuerySlice, F: ArchetypeFilter> QuerySliceIter<'_, '_, D, F> {
    /// # Safety
    /// - `world` must be the same world used to build `state`.
    /// - `state` must have access registrations compatible with `D`/`F`.
    /// - `last_run`/`this_run` must belong to the same world tick stream.
    /// - Caller must ensure no aliasing violations are introduced through
    ///   concurrent mutable iteration paths.
    pub unsafe fn new<'w, 's>(
        world: WorldCell<'w>,
        state: &'s QueryState<D, F>,
        last_run: Tick,
        this_run: Tick,
    ) -> QuerySliceIter<'w, 's, D, F> {
        unsafe {
            QuerySliceIter {
                world,
                state,
                d_cache: D::build_cache(&state.d_state, world, last_run, this_run),
                f_cache: F::build_cache(&state.f_state, world, last_run, this_run),
                storages: state.storages.iter(),
                entities: &[],
            }
        }
    }

    /// Advances to the next non-empty storage slice and refreshes caches.
    ///
    /// Returns `None` when no storage remains.
    fn update_slice(&mut self) -> Option<()> {
        loop {
            let table_id: TableId = *self.storages.next()?;
            let tables = unsafe { &mut self.world.data_mut().tables };
            let table = unsafe { tables.get_unchecked_mut(table_id) };
            let ptr = table as *mut Table;
            self.entities = unsafe { (&*ptr).entities() };
            if !self.entities.is_empty() {
                unsafe {
                    D::update_table(&self.state.d_state, &mut self.d_cache, &mut *ptr);
                    F::update_table(&self.state.f_state, &mut self.f_cache, &*ptr);
                }
                return Some(());
            }
        }
    }
}

impl<'w, D: QuerySlice, F: ArchetypeFilter> Iterator for QuerySliceIter<'w, '_, D, F> {
    type Item = D::SliceItem<'w>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.update_slice()?;

            // Important optimization: skip entity filtering.
            debug_assert!(!F::ENABLE_ENTITY_FILTER);

            let d_state = &self.state.d_state;
            let d_cache = &mut self.d_cache;
            let entities = self.entities;
            if let Some(data) = unsafe { D::fetch_slice(d_state, d_cache, entities) } {
                return Some(data);
            }
        }
    }
}

impl<D: QuerySlice, F: ArchetypeFilter> FusedIterator for QuerySliceIter<'_, '_, D, F> {}

// -----------------------------------------------------------------------------
