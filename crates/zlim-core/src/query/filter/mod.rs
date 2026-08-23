//! [`QueryFilter`] implementations — the predicate half of a query.
//!
//! Filters refine which entities a query visits without changing what data
//! is fetched: `With<T>`, `Without<T>`, `Added<T>`, `Changed<T>`, and
//! logical combinators `And`/`Or`.

mod added;
mod and;
mod changed;
mod or;
mod with;
mod without;

pub use added::Added;
pub use and::And;
pub use changed::Changed;
pub use or::Or;
pub use with::With;
pub use without::Without;

// -----------------------------------------------------------------------------
// QueryFilter

use crate::entity::EntityId;
use crate::system::{AccessTable, ComponentAccess, FilterParamBuilder};
use crate::table::{Table, TableRow};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

/// Core trait for types that can filter entities in a query.
///
/// Filters can participate in two levels:
/// - storage-level prefiltering (`register_filter`) for fast coarse pruning,
/// - entity-level checks (`filter`) for per-row decisions.
///
/// Implementations should push as much work as possible into storage-level
/// filtering to reduce per-entity overhead.
///
/// The following filters are available:
///
/// | Filter | Description |
/// |--------|-------------|
/// | `And<(F1, F2, ...)>` | Logical AND - all inner filters must be satisfied |
/// | `Or<(F1, F2, ...)>` | Logical OR - at least one inner filter must be satisfied |
/// | `With<C>` | Requires the entity to have component `C` |
/// | `With<(C1, C2, ...)>` | Requires the entity to have all specified components |
/// | `Without<C>` | Requires the entity to NOT have component `C` |
/// | `Without<(C1, C2, ...)>` | Requires the entity to have none of the specified components |
/// | `Changed<C>` | Component `C` must have been modified in the interval `(last_run, this_run]` |
/// | `Added<C>` | Component `C` must have been added in the interval `(last_run, this_run]` |
///
/// # Type Parameters
///
/// - [`QueryFilter::State`] - Static data shared across all query instances
/// - [`QueryFilter::Cache`] - Per-execution cached data for a specific world state
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
/// #[derive(TypePath, Component, Clone)]
/// struct Enemy;
///
/// // `With<Enemy>` restricts the query to entities that have `Enemy`.
/// fn damage_enemies(mut query: Query<&mut Health, With<Enemy>>) {
///     for health in query.iter_mut() {
///         let health = health.into_inner();
///         health.0 = health.0.saturating_sub(1);
///     }
/// }
///
/// let mut world = World::alloc();
/// world.spawn((Health(10), Enemy), None);
/// world.spawn(Health(20), None);
///
/// damage_enemies(world.query_mut::<&mut Health, With<Enemy>>());
/// assert_eq!(world.query::<&Health, With<Enemy>>().iter().next().unwrap().0, 9);
/// ```
///
/// # Safety
///
/// Implementing this trait requires careful attention to memory safety and
/// component access patterns. See trait methods for specific safety requirements.
pub unsafe trait QueryFilter {
    /// Static data shared across all query instances.
    ///
    /// This is typically built once during query construction and contains
    /// information like component IDs that don't change over the query's lifetime.
    type State: Clone + Send + Sync + 'static;

    /// Per-query cached data for a specific world state.
    ///
    /// This cache is rebuilt each time the query is executed and may contain
    /// world-specific data like component pointers or pre-computed lookup tables.
    type Cache<'world>;

    /// Indicates whether this filter performs per-entity filtering.
    ///
    /// If `false`, the filter can be fully evaluated at the archetype/table level,
    /// allowing for optimizations like skipping the per-entity filter loop.
    ///
    /// Example: `With<T>` usually does not require per-entity checks, while
    /// tick-based predicates like `Changed<T>` generally do.
    const ENABLE_ENTITY_FILTER: bool;

    /// Builds the static state for this filter.
    ///
    /// This is called once when the query is first created. The state is
    /// shared across all query executions.
    fn build_state(world: &World) -> Self::State;

    /// Builds a per-execution cache for this filter.
    ///
    /// This is called at the beginning of each query execution to prepare
    /// world-specific data needed for filtering.
    ///
    /// # Safety
    /// - The returned cache must remain valid for the duration of the query
    /// - World access must follow the provided tick parameters
    unsafe fn build_cache<'w>(
        state: &Self::State,
        world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Self::Cache<'w>;

    /// Builds table-level filter parameters.
    ///
    /// This contributes constraints used during table filtering.
    /// The `out` vector is in disjunctive-normal-form style: each item is one
    /// `Or` branch, and the query matches if any branch is satisfied.
    ///
    /// # Note
    ///
    /// The caller must ensure that [`QueryFilter::register_filter`] is called
    /// **before** [`QueryData::register_filter`].
    ///
    /// For [`QueryFilter::register_filter`] implementations, new branches
    /// typically need to be added. By default, the input `out` is an empty
    /// vector, meaning no table would satisfy the filter conditions.
    ///
    /// [`QueryData::register_filter`]: crate::query::QueryData::register_filter
    fn register_filter(state: &Self::State, out: &mut Vec<FilterParamBuilder>);

    /// Builds the set of data this query may access.
    ///
    /// Unlike [`QueryFilter::register_filter`], which describes table matching,
    /// this method describes potential component/resource accesses for system
    /// safety checks (mutual exclusion and aliasing validation).
    ///
    /// For example, `Query<(..), Changed<Foo>>` needs to access the change
    /// tick of `Foo`.
    ///
    /// Because `QueryFilter` is read only and evaluated during iterator
    /// filtering, it's always valid.
    ///
    /// # Note
    ///
    /// The caller must ensure that [`QueryFilter::register_access`] is called
    /// **after** [`QueryData::register_access`].
    ///
    /// `QueryFilter` target accesses are evaluated during iterator filtering
    /// and do not conflict with `QueryData` target registration, so
    /// `QueryData` should register first.
    ///
    /// [`QueryData::register_access`]: crate::query::QueryData::register_access
    fn register_access(state: &Self::State, out: &mut ComponentAccess);

    /// Explicitly edit access table.
    ///
    /// This is usually only used for queries that require access to resources.
    ///
    /// ## TODO:
    /// It doesn't seem good to directly open this interface, a better approach
    /// is to provide an interface that only allows access to editing resources.
    fn modify_access_table(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool;

    /// Updates the cache for a specific table.
    ///
    /// Called when the query begins processing a new table. The filter
    /// can pre-compute table-level information to speed up later filtering.
    ///
    /// # Safety
    /// - The table must remain valid for the duration of the query
    /// - Cache updates must not invalidate existing data
    unsafe fn update_table<'w>(state: &Self::State, cache: &mut Self::Cache<'w>, table: &'w Table);

    /// Performs per-entity filtering.
    ///
    /// This is called for each entity that passes archetype/table-level checks.
    /// Returns `true` if the entity should be included in query results.
    ///
    /// # Safety
    /// - The entity must exist and be valid
    /// - The table row must be valid for the current table
    /// - Cache data must be properly set for the current archetype/table
    unsafe fn filter<'w>(
        state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entity: EntityId,
        table_row: TableRow,
    ) -> bool;
}

/// Marker for filters that can be fully evaluated at the table level.
///
/// Such filters never need per-entity checks, so queries can skip the
/// per-entity filtering loop entirely when the filter cache is set.
///
/// # Safety
///
/// Implementors must uphold the safety contracts of [`QueryFilter`] —
/// table-level evaluation must be equivalent to the entity-level predicate.
///
/// [`QueryFilter::ENABLE_ENTITY_FILTER`] must be `false`.
pub unsafe trait ArchetypeFilter: QueryFilter {}

// -----------------------------------------------------------------------------
// empty

unsafe impl QueryFilter for () {
    type State = ();
    type Cache<'world> = ();

    const ENABLE_ENTITY_FILTER: bool = false;

    #[inline(always)]
    fn build_state(_world: &World) -> Self::State {}

    #[inline(always)]
    unsafe fn build_cache<'w>(
        _state: &Self::State,
        _world: WorldCell<'w>,
        _last_run: Tick,
        _this_run: Tick,
    ) -> Self::Cache<'w> {
    }

    #[inline(always)]
    fn register_filter(_state: &Self::State, out: &mut Vec<FilterParamBuilder>) {
        out.push(FilterParamBuilder::new());
    }

    #[inline(always)]
    fn register_access(_state: &Self::State, _out: &mut ComponentAccess) {}

    #[inline(always)]
    fn modify_access_table(_state: &Self::State, _table: &mut AccessTable, _: bool) -> bool {
        true
    }

    #[inline(always)]
    unsafe fn update_table<'w>(_: &Self::State, _: &mut Self::Cache<'w>, _: &'w Table) {}

    #[inline(always)]
    unsafe fn filter<'w>(
        _state: &Self::State,
        _cache: &mut Self::Cache<'w>,
        _entity: EntityId,
        _table_row: TableRow,
    ) -> bool {
        true
    }
}

unsafe impl ArchetypeFilter for () {}
