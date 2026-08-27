use core::fmt::Debug;

use zlim_utils::debug::DebugName;
use zlim_utils::hash::{HashSet, NoopState};

use zlim_log as log;

use super::{QueryData, QueryFilter};
use crate::component::ComponentId;
use crate::query::{ArchetypeFilter, QuerySlice};
use crate::system::{AccessTable, ComponentAccess, FilterParam, FilterParamBuilder};
use crate::table::{TableId, Tables};
use crate::world::{World, WorldId};

// -----------------------------------------------------------------------------
// QueryState

/// Reusable query state for a specific query type.
///
/// `QueryState` roughly contains:
/// - The owning world ID
/// - A state version used for incremental updates
/// - The set of matched archetypes or tables at the current version
/// - Cached state for query data and query filters
///
/// # Incremental Updates
///
/// As described in [`Query`](crate::query::Query), query filtering happens in
/// two phases: table-based filtering and entity filtering. `QueryState`
/// caches the table-filtering result: the set of [`TableId`]s whose
/// component sets satisfy the query's filter parameters.
///
/// In `World`, the table count only grows and never shrinks, and each
/// generated table represents a fixed component set. Therefore, the table
/// count is used as a version number, and updates only need to process
/// newly added tables.
///
/// # Usage
///
/// `Query` is effectively a typed view over `QueryState`: it pairs the state
/// with a [`WorldCell`](crate::world::WorldCell) and a change-detection tick
/// window.  In most contexts you never touch `QueryState` directly — obtain
/// a `Query` through the [`Query`](crate::query::Query) system parameter,
/// [`World::query`](crate::world::World::query), or
/// [`World::query_mut`](crate::world::World::query_mut).  Build a
/// `QueryState` yourself only when you need to reuse one query across
/// multiple borrows or outside of systems.
///
/// # World Affinity
///
/// A `QueryState` is bound to the world it was built from.
/// Reusing it with another world is invalid and guarded by runtime checks.
#[repr(C)] // required for the pointer cast in `as_readonly`.
pub struct QueryState<D: QueryData, F: QueryFilter = ()> {
    pub(super) world_id: WorldId,
    pub(super) version: u32,
    pub(super) storages: Vec<TableId>,
    pub(super) access_data: ComponentAccess,
    pub(super) access_filter: Box<[FilterParam]>,
    pub(super) d_state: D::State,
    pub(super) f_state: F::State,
}

// -----------------------------------------------------------------------------
// Basic

impl<D: QueryData, F: QueryFilter> Clone for QueryState<D, F> {
    fn clone(&self) -> Self {
        Self {
            world_id: self.world_id,
            version: self.version,
            storages: self.storages.clone(),
            access_data: self.access_data.clone(),
            access_filter: self.access_filter.clone(),
            d_state: self.d_state.clone(),
            f_state: self.f_state.clone(),
        }
    }

    fn clone_from(&mut self, source: &Self) {
        self.world_id = source.world_id;
        self.version = source.version;
        self.storages.clone_from(&source.storages);
        self.access_data.clone_from(&source.access_data);
        self.access_filter.clone_from(&source.access_filter);
        self.d_state.clone_from(&source.d_state);
        self.f_state.clone_from(&source.f_state);
    }
}

impl<D: QueryData, F: QueryFilter> Debug for QueryState<D, F> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("QueryState")
            .field("name", &DebugName::type_name::<Self>())
            .field("world_id", &self.world_id)
            .field("storages", &self.storages)
            .finish_non_exhaustive()
    }
}

// -----------------------------------------------------------------------------
// Basic

impl<D: QueryData, F: QueryFilter> QueryState<D, F> {
    /// Returns the world ID this query state belongs to.
    pub fn world_id(&self) -> WorldId {
        self.world_id
    }
}

// -----------------------------------------------------------------------------
// Build

impl<D: QueryData, F: QueryFilter> QueryState<D, F> {
    #[cold]
    #[inline(never)]
    fn invalid_query_data(param: &ComponentAccess) {
        let info = param.display();
        log::warn! {
            "invalid query data `{}` in query `{}`: `{info}`",
            DebugName::type_name::<D>(),
            DebugName::type_name::<Self>(),
        }
    }

    /// Builds a new query state from the given world.
    ///
    /// This initializes query/filter internal states, computes filter params,
    /// and collects the initial matched storage set.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::query::QueryState;
    ///
    /// #[derive(TypePath, Component, Clone)]
    /// struct Position { x: f32, y: f32 }
    ///
    /// #[derive(TypePath, Component, Clone)]
    /// struct Player;
    ///
    /// let mut world = World::alloc();
    /// world.spawn(Position { x: 1.0, y: 2.0 }, None);
    ///
    /// // Build the state once; it is up to date immediately.
    /// let mut state = QueryState::<&Position>::build(&world);
    /// assert_eq!(state.world_id(), world.id());
    /// assert!(!state.should_update(&world));
    ///
    /// // Registering a new archetype adds a table, invalidating the state.
    /// world.spawn(Player, None);
    /// assert!(state.should_update(&world));
    /// state.update(&world);
    /// assert!(!state.should_update(&world));
    /// ```
    pub fn build(world: &World) -> Self {
        let d_state = D::build_state(world);
        let f_state = F::build_state(world);

        Self::build_internal(world, d_state, f_state)
    }

    fn build_internal(world: &World, d_state: D::State, f_state: F::State) -> Self {
        let world_id = world.id();

        let mut access_data = ComponentAccess::new();
        if !D::register_access(&d_state, &mut access_data) {
            Self::invalid_query_data(&access_data);
        }

        // `F::build_access` function must be called after `D::build_access`.
        // Because the filter is read-only and does not conflict with data
        // access simultaneously. At this point, the filter is forced to read.
        F::register_access(&f_state, &mut access_data);

        let mut builders = Vec::<FilterParamBuilder>::new();
        // `F::build_filter` function must be called before `D::build_filter`.
        // Filters are responsible for constructing filtering parameters, while
        // visitors make modifications only, will not create new `FilterParamBuilder`.
        // If the visitor advances, it will be `no-op` because `builders` is empty now.
        F::register_filter(&f_state, &mut builders);
        D::register_filter(&d_state, &mut builders);
        let access_filter: Box<[FilterParam]> = collect_filter(builders);

        let mut version: u32 = 0;
        let mut storages: Vec<TableId> = Vec::new();

        let tables = &world.tables;
        let size_hint = (tables.len() >> 3).next_power_of_two() >> 1;
        storages.reserve(size_hint);
        update_table_state(&mut version, &mut storages, &access_filter, tables);

        QueryState {
            world_id,
            version,
            storages,
            access_data,
            access_filter,
            d_state,
            f_state,
        }
    }

    /// Return `true` if this QueryState should update.
    pub fn should_update(&self, world: &World) -> bool {
        debug_assert!(self.world_id == world.id());
        // SAFETY: It must be '>'. When it is `=`, it **cannot** be updated.
        world.tables.len() > self.version as usize
    }

    /// Incrementally updates cached storage matches against the current world.
    ///
    /// Only tables added since the last recorded version are processed.
    ///
    /// # Panics
    ///
    /// In debug builds, panics if `world` does not match
    /// [`QueryState::world_id`].
    pub fn update(&mut self, world: &World) {
        if self.should_update(world) {
            ::core::hint::cold_path();
            update_table_state(
                &mut self.version,
                &mut self.storages,
                &self.access_filter,
                &world.tables,
            );
        }
    }

    /// Records this query's access requirements into an [`AccessTable`].
    ///
    /// Returns `false` when access conflicts are detected.
    pub fn register_access(&self, access_table: &mut AccessTable, strict: bool) -> bool {
        let data: &ComponentAccess = &self.access_data;
        let filter: &[FilterParam] = &self.access_filter;
        // Not return in advance(if error), we hope to provide complete information.
        let valid = access_table.register_component_access(data, filter, strict);
        F::modify_access_table(&self.f_state, access_table, valid & strict) & valid
    }
}

// not_inline: accelerate compilation.
#[inline(never)]
fn collect_filter(builders: Vec<FilterParamBuilder>) -> Box<[FilterParam]> {
    // We use NoopHash because FilterParam is pre-hased.
    let mut params: HashSet<FilterParam, NoopState> =
        HashSet::with_capacity_and_hasher(builders.len(), NoopState);

    builders.into_iter().for_each(|builder| {
        if let Some(param) = builder.build() {
            params.insert(param);
        }
    });

    params.into_iter().collect()
}

// not_inline: accelerate compilation.
#[inline(never)]
fn update_table_state(
    version: &mut u32,
    storages: &mut Vec<TableId>,
    access_filter: &[FilterParam],
    tables: &Tables,
) {
    let new_version = tables.len() as u32;

    for table_id in (*version)..new_version {
        let table_id = unsafe { TableId::new(table_id) };
        let table = unsafe { tables.get_unchecked(table_id) };
        let components = table.components();

        let matched = access_filter
            .iter()
            .any(|p| matches_sorted(components, p.with(), p.without()));

        if matched {
            storages.push(table_id);
        }
    }

    // The pushed table_ids are already sorted.
    *version = new_version;
}

/// Fast archetype matching requiring sorted input slices.
///
/// # Complexity
/// - Time: O(min(m + n, m * log n)) where m = len(with) + len(without), n = total components
/// - Space: O(1)
fn matches_sorted(
    components: &[ComponentId],
    with: &[ComponentId],
    without: &[ComponentId],
) -> bool {
    #[inline]
    fn jump_search(id: ComponentId, slice: &[ComponentId]) -> Result<usize, usize> {
        let mut index = 0usize;
        let len = slice.len();

        loop {
            if index >= len || slice[index] > id {
                return Err(index);
            }
            if slice[index] == id {
                return Ok(index);
            }

            let mut step = 1usize;
            loop {
                let offset = index + step;
                if offset < len && slice[offset] <= id {
                    step <<= 1;
                } else {
                    break;
                }
            }
            // index + (step >> 1) < len
            // index + max(step >> 1, 1) <= len
            index += core::cmp::max(step >> 1, 1);
        }
    }

    {
        // with
        let mut temp = components;
        let result = with.iter().all(|&id| {
            // `with` has been sorted and deduplicated, the `[..=idx]` can be skipped.
            // we can skip `[idx]` because it's `==` specific id.
            if let Ok(idx) = jump_search(id, temp) {
                temp = &temp[(idx + 1)..];
                true
            } else {
                false
            }
        });
        if !result {
            return false;
        }
    }
    {
        // without
        let mut temp = components;
        // `without` has been sorted and deduplicated, the `[..idx]` can be skipped.
        // cannot skip `[idx]` because it's `>` specific id (or the end of slice).
        without.iter().all(|&id| {
            if let Err(idx) = jump_search(id, temp) {
                temp = &temp[idx..];
                true
            } else {
                false
            }
        })
    }
}

// -----------------------------------------------------------------------------

impl<D: QueryData, F: QueryFilter> QueryState<D, F> {
    /// Returns a reference to this state reinterpreted as a read-only version.
    ///
    /// The cast is valid because `QueryState<D, F>` and `QueryState<D::ReadOnly, F>`
    /// have identical memory layouts: every field is the same type, in particular
    /// `d_state: D::State = D::ReadOnly::State` (enforced by the
    /// `ReadOnly: ReadOnlyQueryData<State = Self::State>` bound).
    #[inline(always)]
    pub fn as_readonly(&self) -> &QueryState<D::ReadOnly, F> {
        const { assert!(size_of::<Self>() == size_of::<QueryState<D::ReadOnly, F>>()) };
        unsafe { core::mem::transmute::<&Self, &QueryState<D::ReadOnly, F>>(self) }
    }
}

impl<D: QuerySlice, F: ArchetypeFilter> QueryState<D, F> {
    /// Returns a reference to this state reinterpreted as a read-only version.
    ///
    /// The cast is valid because `QueryState<D, F>` and `QueryState<D::ReadOnly, F>`
    /// have identical memory layouts: every field is the same type, in particular
    /// `d_state: D::State = D::ReadOnly::State` (enforced by the
    /// `ReadOnly: ReadOnlyQueryData<State = Self::State>` bound).
    #[inline(always)]
    pub fn as_readonly_slice(&self) -> &QueryState<D::ReadOnlySlice, F> {
        const { assert!(size_of::<Self>() == size_of::<QueryState<D::ReadOnlySlice, F>>()) };
        unsafe { core::mem::transmute::<&Self, &QueryState<D::ReadOnlySlice, F>>(self) }
    }
}

// -----------------------------------------------------------------------------
