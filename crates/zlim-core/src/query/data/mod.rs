//! [`QueryData`] implementations — the fetch half of a query.
//!
//! `QueryData` controls what data is retrieved from matching entities:
//! component references (`&T`, `&mut T`), entity identity, and tuple
//! compositions that combine multiple fetch items in one query.

mod comp;
mod entity;
mod tuples;

// -----------------------------------------------------------------------------
// QueryData

use crate::entity::EntityId;
use crate::system::{ComponentAccess, FilterParamBuilder};
use crate::table::{Table, TableRow};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

/// Core trait for types that can be fetched from entities in a query.
///
/// This trait defines how a query accesses data from entities. It is implemented
/// for component references, tuples of components, and other data sources.
///
/// # Derive macro usage
///
/// Prefer `#[derive(QueryData)]` (from `zlim_core_derive`) for custom
/// query-data structs.
///
/// Rules enforced by the derive:
/// - The struct may have no lifetime params, or exactly one lifetime named `'w`.
/// - For mutable fields, use [`crate::borrow::Mut`] instead of `&'w mut T`.
/// - Add `#[query_data(readonly)]` when the derived type is read-only and
///   should also implement [`ReadOnlyQueryData`].
///
/// Example:
///
/// ```rust
/// use zlim_core::borrow::Mut;
/// use zlim_core::derive::{Component, QueryData};
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Position { x: f32, y: f32 }
///
/// #[derive(TypePath, Component, Clone)]
/// struct Velocity { x: f32, y: f32 }
///
/// #[derive(QueryData)]
/// #[query_data(readonly)]
/// struct ReadVelocity<'w> {
///     velocity: &'w Velocity,
/// }
///
/// #[derive(QueryData)]
/// struct MoveData<'w> {
///     position: Mut<'w, Position>,
///     velocity: &'w Velocity,
/// }
/// ```
///
/// # Available Params
///
/// The following query data forms are supported:
///
/// - **Entity handles**: `EntityId`, `EntityRef`, `EntityMut`
/// - **Component references**: `&T`, `&mut T`, `Ref<T>`, `Mut<T>` where `T` is a component type
/// - **Optional components**: `Option<&T>`, `Option<&mut T>`, `Option<Ref<T>>`, `Option<Mut<T>>`
///
/// For mutable component forms, `Item<'world>` is [`crate::borrow::Mut`] (or
/// `Option<Mut<_>>`) rather than raw `&mut T`, so change ticks remain attached.
///
/// Tuples composed from these forms are also valid, for example `(&Foo, &mut Bar)`.
///
/// # Aliasing rules
///
/// `QueryData` must obey Rust aliasing rules. For example, `(&Foo, &mut Foo)` is
/// invalid and will panic at runtime.
///
/// Also note the difference between entity-only and entity-wide access:
/// - `EntityId` carries only an entity ID and does not access components.
/// - `EntityRef` represents shared access to all components on that entity.
/// - `EntityMut` represents exclusive access to all components on that entity.
///
/// Therefore, `(EntityRef, &Foo)` is valid, while `(EntityRef, &mut Foo)` and
/// `(EntityMut, &Foo)` are invalid and will panic at runtime.
///
/// # Safety
///
/// Implementing this trait requires careful attention to memory safety and
/// component access patterns. See trait methods for specific safety requirements.
///
/// The `QueryData::Item` should not need `Drop`.
///
/// Implementations should treat `register_filter` and `register_access` as
/// separate concerns:
/// - filter describes which storages/entities are candidates,
/// - access describes what data might be read or written for scheduler conflict checks.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a query data",
    label = "invalid query data",
    note = "Consider annotating `{Self}` with `#[derive(QueryData)]`."
)]
pub unsafe trait QueryData {
    /// The read-only form of this query data.
    ///
    /// Mutable accessors are downgraded to their read-only counterparts:
    /// - `&mut T` and `Mut<T>` become `Ref<T>` (preserving change-tick metadata).
    /// - `EntityMut` becomes `EntityRef`.
    /// - Already-read-only types keep `ReadOnly = Self`.
    ///
    /// The constraint `ReadOnly::State = Self::State` guarantees that
    /// `QueryState<D, F>` can be reinterpreted as `QueryState<D::ReadOnly, F>`
    /// via a pointer cast, because every stored field has the same type.
    type ReadOnly: ReadOnlyQueryData<State = Self::State>;

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

    /// The type returned when fetching data for a single entity.
    ///
    /// The data should be lightweight and not contain any content
    /// that needs to be dropped.
    type Item<'world>;

    /// Builds the static state for this query data.
    ///
    /// This is called once when the query is first created. The state is
    /// shared across all query executions and contains metadata needed for
    /// future cache building and fetching.
    fn build_state(world: &World) -> Self::State;

    /// Builds a per-execution cache for this query data.
    ///
    /// This is called at the beginning of each query execution to prepare
    /// world-specific data needed for fetching. The cache may contain direct
    /// pointers to component arrays or other performance-critical data.
    ///
    /// # Safety
    /// - The returned cache must remain valid for the duration of the query
    /// - World access must follow the provided tick parameters
    /// - Pointers stored in cache must remain valid while cache is alive
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
    /// Therefore, implementations of [`QueryData::register_filter`] usually
    /// add requirements to every existing branch, instead of creating new
    /// branches.
    ///
    /// [`QueryFilter::register_filter`]: crate::query::QueryFilter::register_filter
    fn register_filter(state: &Self::State, out: &mut Vec<FilterParamBuilder>);

    /// Builds the set of data this query may access.
    ///
    /// Unlike [`QueryData::register_filter`], which describes table matching,
    /// this method describes potential component/resource accesses for system
    /// safety checks (mutual exclusion and aliasing validation).
    ///
    /// For example, `Query<(&mut Foo, &Foo)>` is an invalid access target,
    /// and this function should return `false`.
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
    /// [`QueryFilter::register_access`]: crate::query::QueryFilter::register_access
    fn register_access(state: &Self::State, out: &mut ComponentAccess) -> bool;

    /// Prepares the cache for a specific table.
    ///
    /// Called when the query begins processing a new table. The implementation
    /// can pre-compute table-specific information to speed up later fetching.
    ///
    /// # Safety
    /// - The table must remain valid for the duration of the query
    /// - Cache updates must not invalidate existing data
    /// - Must correctly handle table column layout
    /// - Prohibit changing the structure of the table
    unsafe fn update_table<'w>(
        state: &Self::State,
        cache: &mut Self::Cache<'w>,
        table: &'w mut Table,
    );

    /// Fetches data for a single entity.
    ///
    /// This is called for each entity that passes all filter conditions.
    /// Returns `Some(item)` if the entity has the requested data, or `None`
    /// if the data is not available (for optional fetches).
    ///
    /// # Safety
    /// - The entity must exist and be valid
    /// - The table row must be valid for the current table
    /// - Cache must be properly set for the current archetype/table
    /// - Returned references must follow Rust's borrowing rules
    unsafe fn fetch<'w>(
        state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entity: EntityId,
        table_row: TableRow,
    ) -> Option<Self::Item<'w>>;
}

/// Marker for [`QueryData`] types that perform only shared (read-only) component access.
///
/// Implementing this trait unlocks two capabilities:
/// - `Query<D, F>` becomes [`Copy`] and [`Clone`], so it can be passed
///   around freely without consuming the original.
/// - `QueryState` accepts a shared `&World` in its `iter` / `get` /
///   `single` methods, avoiding the need for an exclusive borrow.
///
/// The supertrait bound `QueryData<ReadOnly = Self>` enforces the invariant that the read-only
/// form of a read-only type is itself — i.e., applying `as_readonly()` to an already-read-only
/// query is a no-op.
///
/// # Safety
///
/// The implementer must guarantee that no mutable component access occurs during
/// [`QueryData::fetch`]. Violating this causes undefined behavior due to aliasing with
/// other concurrent shared borrows.
pub unsafe trait ReadOnlyQueryData: QueryData<ReadOnly = Self> {}

/// Marker for [`QueryData`] types that can fetch a whole component slice in
/// one call.
///
/// Slice fetch is an optimization for dense queries: instead of fetching
/// one row at a time, the entire contiguous column of the current table is
/// returned as a slice of items.
///
/// # Safety
///
/// Implementors must uphold the [`QueryData`] safety contracts.  The
/// returned slice must reference the table's own storage and must not alias
/// any other active borrow for the lifetime of the query.
pub unsafe trait QuerySlice: QueryData {
    /// The type returned when fetching data for a slice of entities.
    ///
    /// The data should be lightweight and not contain any content that
    /// needs to be dropped.
    type SliceItem<'world>;

    /// The read-only slice form of this query data.
    ///
    /// Mirrors [`QueryData::ReadOnly`] but for slice fetching: mutable slice
    /// forms downgrade to their read-only counterparts (e.g. `SliceMut<T>` →
    /// `SliceRef<T>`).  The constraint `ReadOnlySlice::State = Self::State`
    /// guarantees that
    /// [`QueryState::as_readonly_slice`](crate::query::QueryState::as_readonly_slice)
    /// can reinterpret the state via a pointer cast, because every stored
    /// field has the same type.
    type ReadOnlySlice: ReadOnlyQueryData<State = Self::State> + QuerySlice<State = Self::State>;

    /// Fetches the component slice of the current table.
    ///
    /// # Safety
    /// - The cache must have been set for the current table (via
    ///   [`QueryData::update_table`]).
    /// - `entities` must be the current table's entity column.
    /// - The returned slice must stay valid for the duration of the query.
    unsafe fn fetch_slice<'w>(
        state: &Self::State,
        cache: &mut Self::Cache<'w>,
        entities: &'w [EntityId],
    ) -> Option<Self::SliceItem<'w>>;
}
