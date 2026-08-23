#![expect(clippy::module_inception, reason = "For better structure.")]

//! The `Query` system parameter and its helper methods.

use core::fmt::Debug;
use core::mem::MaybeUninit;

use super::error::{QueryEntityError, QuerySingleError};
use super::iter::QueryIter;
use super::single::Single;
use super::{QueryData, QueryFilter, QueryState, ReadOnlyQueryData};
use crate::entity::{Entities, EntityId, Location};
use crate::query::{ArchetypeFilter, QuerySlice, QuerySliceIter};
use crate::system::{AccessTable, SystemParam, SystemParamError};
use crate::table::TableId;
use crate::tick::Tick;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------
// Query

/// A parameter for querying components and entities from the ECS world.
///
/// `Query` contains two type parameters: [`QueryData`] (what to fetch) and
/// [`QueryFilter`] (filtering conditions, defaults to no filtering).
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Foo;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Bar;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Baz;
///
/// // Basic component query
/// fn system1(query: Query<&Foo>) {
///     for _ in query {
///         /* ... */
///     }
/// }
///
/// // Query with tuple and filter
/// fn system2(query: Query<(EntityId, &Foo), With<Bar>>) {
///     for _ in query {
///         /* ... */
///     }
/// }
///
/// // Complex filter composition
/// fn system3(query: Query<(EntityId, &Foo), And<(With<Bar>, Without<Baz>, Changed<Foo>)>>) {
///     for _ in query {
///         /* ... */
///     }
/// }
///
/// let mut world = World::alloc();
/// let first = world.spawn((Foo, Bar), None);
/// let first_id = first.id();
/// drop(first);
/// world.spawn((Foo, Bar, Baz), None);
///
/// assert_eq!(world.query::<&Foo, ()>().iter().count(), 2);
/// assert!(world.query::<&Foo, With<Bar>>().contains(first_id));
/// assert_eq!(world.query::<&Foo, Without<Baz>>().iter().count(), 1);
/// assert_eq!(world.query::<(EntityId, &Foo), With<Bar>>().iter().count(), 2);
/// assert_eq!(
///     world
///         .query::<&Foo, And<(With<Bar>, Without<Baz>, Changed<Foo>)>>()
///         .iter()
///         .count(),
///     1
/// );
/// ```
///
/// # Query Data Types
///
/// The following types can be used as query data (implement [`QueryData`]):
///
/// - **Entity handles**: `EntityId`
/// - **Component references**: `&T`, `&mut T`, `Ref<T>`, `Mut<T>` where `T` is a component type
/// - **Optional components**: `Option<&T>`, `Option<&mut T>`, `Option<Ref<T>>`, `Option<Mut<T>>`
///
/// Mutable forms (`&mut T`, `Option<&mut T>`) yield [`crate::borrow::Mut`] at
/// iteration/fetch time, so change-tracking metadata is preserved.
///
/// # Query Filter Types
///
/// The following filters are available (implement [`QueryFilter`]):
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
/// For custom implementations, refer to the [`QueryData`] and [`QueryFilter`] traits.
///
/// # Implementation & Optimization
///
/// Query execution follows a two-phase filtering strategy:
///
/// 1. **Table-based filtering**: quickly eliminates entire tables (archetypes)
///    that cannot possibly match the query criteria.
/// 2. **Entity-based filtering**: performs fine-grained filtering on individual
///    entities during iteration.
///
/// ## Optimizations
///
/// 1. **Table caching**: [`QueryState`] caches the results of table-based
///    filtering, eliminating repeated table traversal. The cache is
///    maintained incrementally as tables are created.
///
/// 2. **Thin handle**: [`Query`] itself is a lightweight handle (essentially
///    a pointer to [`QueryState`]) that doesn't perform entity-level
///    filtering. For read-only queries (`D: ReadOnlyQueryData`) [`Query`] is
///    [`Copy`]. Use [`Query::as_readonly`] to obtain a read-only view from a
///    mutable query at zero cost.
///
/// 3. **Filter elimination**: simple filters (like `With`/`Without`) can be
///    evaluated entirely at the table level. If no complex filters (e.g.
///    `Changed`/`Added`) are present, the entity-level filtering loop is
///    skipped entirely.
///
/// ## Slice iteration
///
/// When the filter is archetype-level (an [`ArchetypeFilter`], i.e. it can be
/// evaluated per table without inspecting individual entities), iteration can
/// be performed with [`Query::iter_slice`] / [`Query::iter_slice_mut`]
/// instead of the regular [`Query::iter`] / [`Query::iter_mut`].
///
/// Slice iteration does not yield one item per entity; each step yields the
/// whole contiguous component slice of the current table, so the contents
/// can be accessed directly through the slice.  This is more efficient than
/// the regular `iter`, which fetches and filters entities one at a time.
///
/// `iter_slice` requires the query data to implement [`QuerySlice`].
///
/// [`QueryIter`]: crate::query::QueryIter
/// [`QueryState`]: crate::query::QueryState
/// [`ArchetypeFilter`]: crate::query::ArchetypeFilter
/// [`QuerySlice`]: crate::query::QuerySlice
pub struct Query<'world, 'state, D: QueryData, F: QueryFilter = ()> {
    pub(super) world: WorldCell<'world>,
    pub(super) state: &'state QueryState<D, F>,
    pub(super) last_run: Tick,
    pub(super) this_run: Tick,
}

// -----------------------------------------------------------------------------
// Query -> SystemParam

unsafe impl<D: QueryData + 'static, F: QueryFilter + 'static> SystemParam for Query<'_, '_, D, F> {
    type State = QueryState<D, F>;
    type Item<'world, 'state> = Query<'world, 'state, D, F>;

    const DEFERRED: bool = false;
    const NON_SEND: bool = false;
    const EXCLUSIVE: bool = false;

    fn init_state(world: &World) -> Self::State {
        QueryState::build(world)
    }

    fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        state.register_access(table, strict)
    }

    unsafe fn build_param<'w, 's>(
        state: &'s mut Self::State,
        world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        state.update(unsafe { world.read_only() });
        Ok(Query {
            world,
            state,
            last_run,
            this_run,
        })
    }
}

impl<D: QueryData, F: QueryFilter> Debug for Query<'_, '_, D, F> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Query")
            .field("state", &self.state)
            .field("last_run", &self.last_run)
            .field("this_run", &self.this_run)
            .finish()
    }
}

// -----------------------------------------------------------------------------
// IntoIterator

impl<'w, 's, D: QueryData, F: QueryFilter> IntoIterator for Query<'w, 's, D, F> {
    type Item = D::Item<'w>;
    type IntoIter = QueryIter<'w, 's, D, F>;

    fn into_iter(self) -> Self::IntoIter {
        unsafe { QueryIter::new(self.world, self.state, self.last_run, self.this_run) }
    }
}

impl<'a, 'w: 'a, 's, D: ReadOnlyQueryData, F: QueryFilter> IntoIterator
    for &'a Query<'w, 's, D, F>
{
    type Item = D::Item<'a>;
    type IntoIter = QueryIter<'a, 's, D, F>;

    fn into_iter(self) -> Self::IntoIter {
        unsafe { QueryIter::new(self.world, self.state, self.last_run, self.this_run) }
    }
}

impl<'a, 'w: 'a, 's, D: QueryData, F: QueryFilter> IntoIterator for &'a mut Query<'w, 's, D, F> {
    type Item = D::Item<'a>;
    type IntoIter = QueryIter<'a, 's, D, F>;

    fn into_iter(self) -> Self::IntoIter {
        unsafe { QueryIter::new(self.world, self.state, self.last_run, self.this_run) }
    }
}

// -----------------------------------------------------------------------------
// Clone Copy

impl<D: ReadOnlyQueryData, F: QueryFilter> Copy for Query<'_, '_, D, F> {}

impl<D: ReadOnlyQueryData, F: QueryFilter> Clone for Query<'_, '_, D, F> {
    fn clone(&self) -> Self {
        *self
    }
}

// -----------------------------------------------------------------------------
// New

impl<'w, 's, D: QueryData, F: QueryFilter> Query<'w, 's, D, F> {
    /// Creates a query handle from a world view and cached state.
    ///
    /// # Safety
    /// - `world` must be the same world used to build `state`.
    /// - `state` must have access registrations compatible with `D`/`F`.
    /// - `last_run`/`this_run` must belong to the same world tick stream.
    /// - The caller must ensure no aliasing violations are introduced
    ///   through concurrent mutable query paths.
    ///
    /// This is normally called by [`World::query`](crate::world::World::query)
    /// and the [`SystemParam`](crate::system::SystemParam) machinery; most
    /// users should obtain `Query` handles through those entry points instead
    /// of calling this method directly.
    #[inline]
    pub unsafe fn new(
        world: WorldCell<'w>,
        state: &'s QueryState<D, F>,
        last_run: Tick,
        this_run: Tick,
    ) -> Self {
        Query {
            world,
            state,
            last_run,
            this_run,
        }
    }

    /// Returns a reborrowed query with a shorter world lifetime.
    ///
    /// This is mainly useful when the query contains mutable borrows and you
    /// need to pass a temporary query handle to helper functions while
    /// keeping the original query available afterward.
    ///
    /// If the query is read-only, [`Query`] itself implements [`Copy`], so
    /// reborrowing is usually unnecessary.
    pub fn reborrow(&mut self) -> Query<'_, 's, D, F> {
        Query {
            world: self.world,
            state: self.state,
            last_run: self.last_run,
            this_run: self.this_run,
        }
    }

    /// Returns a read-only view of this query.
    ///
    /// Mutable accessors are downgraded to their read-only counterparts:
    /// `&mut T` / `Mut<T>` → `Ref<T>`.  This is zero-cost — no data is
    /// copied.
    ///
    /// The returned query carries the full `'w` world lifetime, so it can
    /// outlive the `&self` borrow that was used to call this method.  For
    /// already-read-only queries (`D: ReadOnlyQueryData`), [`Query`] is
    /// [`Copy`] so this method is equivalent to a copy.
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
    /// fn read_then_write(mut query: Query<&mut Position>) {
    ///     // Downgrade to a read-only view without copying any data.
    ///     let readonly = query.as_readonly();
    ///     let total: f32 = readonly.iter().map(|p| p.into_inner().x).sum();
    ///     // The original mutable query is still usable afterwards.
    ///     for position in query.iter_mut() {
    ///         position.into_inner().x += total;
    ///     }
    /// }
    ///
    /// let mut world = World::alloc();
    /// world.spawn(Position { x: 1.0, y: 2.0 }, None);
    ///
    /// read_then_write(world.query_mut::<&mut Position, ()>());
    /// assert_eq!(world.query::<&Position, ()>().iter().next().unwrap().x, 2.0);
    /// ```
    pub fn as_readonly(&self) -> Query<'w, 's, D::ReadOnly, F> {
        Query {
            world: self.world,
            state: self.state.as_readonly(),
            last_run: self.last_run,
            this_run: self.this_run,
        }
    }
}

// -----------------------------------------------------------------------------
// Iter

impl<'w, 's, D: QueryData, F: QueryFilter> Query<'w, 's, D, F> {
    /// Returns a mutable iterator over query results.
    ///
    /// Each item is fetched as [`crate::borrow::Mut`] (for `&mut T` query
    /// data), so change-tick metadata is preserved.  Because the iterator
    /// borrows the query mutably, the query cannot be used again until the
    /// iteration has finished.
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
    /// fn move_all(mut query: Query<&mut Position>) {
    ///     for position in query.iter_mut() {
    ///         position.into_inner().x += 1.0;
    ///     }
    /// }
    ///
    /// let mut world = World::alloc();
    /// world.spawn(Position { x: 1.0, y: 2.0 }, None);
    ///
    /// move_all(world.query_mut::<&mut Position, ()>());
    /// assert_eq!(world.query::<&Position, ()>().iter().next().unwrap().x, 2.0);
    /// ```
    pub fn iter_mut(&mut self) -> QueryIter<'_, 's, D, F> {
        unsafe { QueryIter::new(self.world, self.state, self.last_run, self.this_run) }
    }

    /// Returns a read-only iterator over query results.
    ///
    /// Mutable query data is downgraded to its read-only counterpart (see
    /// [`Query::as_readonly`]), so this method only needs a shared reference
    /// and the query stays usable afterwards.
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
    ///     query.iter().map(|p| p.x).sum()
    /// }
    ///
    /// let mut world = World::alloc();
    /// world.spawn(Position { x: 1.0, y: 2.0 }, None);
    /// world.spawn(Position { x: 3.0, y: 4.0 }, None);
    ///
    /// assert_eq!(total_x(world.query::<&Position, ()>()), 4.0);
    /// ```
    pub fn iter(&self) -> QueryIter<'_, 's, D::ReadOnly, F> {
        unsafe {
            QueryIter::new(
                self.world,
                self.state.as_readonly(),
                self.last_run,
                self.this_run,
            )
        }
    }
}

impl<'w, 's, D: QuerySlice, F: ArchetypeFilter> Query<'w, 's, D, F> {
    /// Returns a mutable iterator over query results.
    ///
    /// Each step yields the whole contiguous component column of the current
    /// table (e.g. [`crate::borrow::SliceMut`] for `&mut T` query data), so
    /// the whole table can be processed in bulk.
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
    /// fn heal_all(mut query: Query<&mut Health>) {
    ///     for mut healths in query.iter_slice_mut() {
    ///         for health in healths.iter_mut() {
    ///             health.0 = health.0.saturating_add(1);
    ///         }
    ///     }
    /// }
    ///
    /// let mut world = World::alloc();
    /// world.spawn(Health(10), None);
    ///
    /// heal_all(world.query_mut::<&mut Health, ()>());
    /// assert_eq!(world.query::<&Health, ()>().iter().next().unwrap().0, 11);
    /// ```
    pub fn iter_slice_mut(&mut self) -> QuerySliceIter<'_, 's, D, F> {
        unsafe { QuerySliceIter::new(self.world, self.state, self.last_run, self.this_run) }
    }

    /// Returns a read-only iterator over query results.
    ///
    /// Unlike [`Query::iter`], each step yields the whole contiguous
    /// component column of the current table as one slice, which is faster
    /// for bulk processing.  Requires the filter to be an
    /// [`ArchetypeFilter`] and the query data to implement [`QuerySlice`].
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
    /// struct Player;
    ///
    /// fn total_health(query: Query<&Health, With<Player>>) -> u32 {
    ///     query
    ///         .iter_slice()
    ///         .flat_map(|healths| healths.iter())
    ///         .map(|h| h.0)
    ///         .sum()
    /// }
    ///
    /// let mut world = World::alloc();
    /// world.spawn((Health(10), Player), None);
    /// world.spawn(Health(5), None);
    ///
    /// assert_eq!(total_health(world.query::<&Health, With<Player>>()), 10);
    /// ```
    pub fn iter_slice(&self) -> QuerySliceIter<'_, 's, D::ReadOnlySlice, F> {
        unsafe {
            QuerySliceIter::new(
                self.world,
                self.state.as_readonly_slice(),
                self.last_run,
                self.this_run,
            )
        }
    }
}

// -----------------------------------------------------------------------------
// Single

impl<'w, 's, D: QueryData, F: QueryFilter> Query<'w, 's, D, F> {
    /// Returns the single matching item with mutable query access.
    ///
    /// Returns [`QuerySingleError::NoEntities`] when nothing matches and
    /// [`QuerySingleError::MultipleEntities`] when more than one entity
    /// matches.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_reflect::derive::TypePath;
    ///
    /// #[derive(TypePath, Component, Clone)]
    /// struct Player { health: u32 }
    ///
    /// fn hurt_player(mut query: Query<&mut Player>) {
    ///     if let Ok(player) = query.single_mut() {
    ///         let player = player.into_inner().into_inner();
    ///         player.health = player.health.saturating_sub(1);
    ///     }
    /// }
    ///
    /// let mut world = World::alloc();
    /// world.spawn(Player { health: 10 }, None);
    ///
    /// hurt_player(world.query_mut::<&mut Player, ()>());
    /// assert_eq!(world.query::<&Player, ()>().iter().next().unwrap().health, 9);
    /// ```
    pub fn single_mut(&mut self) -> Result<Single<'_, D, F>, QuerySingleError> {
        unsafe { Single::new(self.world, self.state, self.last_run, self.this_run) }
    }

    /// Returns the single matching item with read-only query access.
    ///
    /// Mutable accessors are downgraded to their read-only counterparts
    /// (see [`Query::as_readonly`]); errors match [`Query::single_mut`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_reflect::derive::TypePath;
    ///
    /// #[derive(TypePath, Component, Clone)]
    /// struct Player { health: u32 }
    ///
    /// fn player_health(query: Query<&Player>) -> u32 {
    ///     query.single().map(|p| p.health).unwrap_or(0)
    /// }
    ///
    /// let mut world = World::alloc();
    /// world.spawn(Player { health: 10 }, None);
    ///
    /// assert_eq!(player_health(world.query::<&Player, ()>()), 10);
    /// ```
    pub fn single(&self) -> Result<Single<'w, D::ReadOnly, F>, QuerySingleError> {
        unsafe {
            Single::new(
                self.world,
                self.state.as_readonly(),
                self.last_run,
                self.this_run,
            )
        }
    }
}

// -----------------------------------------------------------------------------
// contains , is_empty

impl<'w, 's, D: QueryData, F: QueryFilter> Query<'w, 's, D, F> {
    /// Returns `true` if this query currently has no matches.
    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }

    /// Returns `true` if `entity` currently satisfies this query.
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
    /// #[derive(TypePath, Component, Clone)]
    /// struct Player;
    ///
    /// fn is_player(query: Query<&Position, With<Player>>, entity: EntityId) -> bool {
    ///     query.contains(entity)
    /// }
    ///
    /// let mut world = World::alloc();
    /// let hero = world.spawn((Position { x: 1.0, y: 2.0 }, Player), None);
    /// let hero_id = hero.id();
    /// drop(hero);
    ///
    /// assert!(is_player(world.query::<&Position, With<Player>>(), hero_id));
    /// ```
    pub fn contains(&self, entity: EntityId) -> bool {
        self.contains_impl(entity)
    }
}

// -----------------------------------------------------------------------------
// get

impl<'w, 's, D: QueryData, F: QueryFilter> Query<'w, 's, D, F> {
    /// Fetches one entity from this query with read-only query access.
    ///
    /// Returns [`QueryEntityError::NoSuchEntity`] if the entity is stale or
    /// despawned, and [`QueryEntityError::QueryMismatch`] if it does not
    /// satisfy this query.
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
    /// fn read_position(query: Query<&Position>, entity: EntityId) -> Option<f32> {
    ///     query.get(entity).ok().map(|p| p.x)
    /// }
    ///
    /// let mut world = World::alloc();
    /// let id = world.spawn(Position { x: 1.0, y: 2.0 }, None).id();
    ///
    /// assert_eq!(read_position(world.query::<&Position, ()>(), id), Some(1.0));
    /// ```
    pub fn get(
        &self,
        entity: EntityId,
    ) -> Result<<D::ReadOnly as QueryData>::Item<'w>, QueryEntityError> {
        self.as_readonly().get_impl(entity)
    }

    /// Fetches one entity from this query with mutable query access.
    ///
    /// Returns [`QueryEntityError::NoSuchEntity`] if the entity is stale or
    /// despawned, and [`QueryEntityError::QueryMismatch`] if it does not
    /// satisfy this query.
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
    /// fn move_entity(mut query: Query<&mut Position>, entity: EntityId) {
    ///     if let Ok(position) = query.get_mut(entity) {
    ///         position.into_inner().x += 1.0;
    ///     }
    /// }
    ///
    /// let mut world = World::alloc();
    /// let id = world.spawn(Position { x: 1.0, y: 2.0 }, None).id();
    ///
    /// move_entity(world.query_mut::<&mut Position, ()>(), id);
    /// assert_eq!(world.query::<&Position, ()>().iter().next().unwrap().x, 2.0);
    /// ```
    pub fn get_mut(&mut self, entity: EntityId) -> Result<D::Item<'_>, QueryEntityError> {
        self.get_impl(entity)
    }

    /// Fetches multiple entities from this query with mutable query access.
    ///
    /// Returns [`QueryEntityError::DuplicateEntity`] if any input entity is
    /// repeated.
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
    /// fn move_pair(mut query: Query<&mut Position>, a: EntityId, b: EntityId) {
    ///     if let Ok([pa, pb]) = query.get_many_mut([a, b]) {
    ///         pa.into_inner().x += 1.0;
    ///         pb.into_inner().y += 1.0;
    ///     }
    /// }
    ///
    /// let mut world = World::alloc();
    /// let a_id = world.spawn(Position { x: 1.0, y: 0.0 }, None).id();
    /// let b_id = world.spawn(Position { x: 0.0, y: 2.0 }, None).id();
    ///
    /// move_pair(world.query_mut::<&mut Position, ()>(), a_id, b_id);
    ///
    /// let sum_x: f32 = world.query::<&Position, ()>().iter().map(|p| p.x).sum();
    /// let sum_y: f32 = world.query::<&Position, ()>().iter().map(|p| p.y).sum();
    /// assert_eq!(sum_x, 2.0);
    /// assert_eq!(sum_y, 3.0);
    /// ```
    pub fn get_many_mut<const N: usize>(
        &mut self,
        entities: [EntityId; N],
    ) -> Result<[D::Item<'_>; N], QueryEntityError> {
        self.get_many_mut_impl(entities)
    }

    /// Fetches multiple entities from this query with read-only query access.
    pub fn get_many<const N: usize>(
        &self,
        entities: [EntityId; N],
    ) -> Result<[<D::ReadOnly as QueryData>::Item<'w>; N], QueryEntityError> {
        self.as_readonly().get_many_impl(&entities)
    }
}

// -----------------------------------------------------------------------------
// Helper

impl<'w, 's, D: QueryData, F: QueryFilter> Query<'w, 's, D, F> {
    #[inline(always)]
    fn contains_storage(&self, table_id: TableId) -> bool {
        #[inline(never)]
        fn split_impl(tables: &[TableId], id: TableId) -> bool {
            // The matched table ids are kept sorted, so a binary search is valid.
            tables.binary_search(&id).is_ok()
        }

        split_impl(self.state.storages.as_slice(), table_id)
    }

    #[inline(always)]
    fn locate_entity(&self, entity_id: EntityId) -> Result<Location, QueryEntityError> {
        #[inline(never)]
        fn split_impl(entities: &Entities, id: EntityId) -> Result<Location, QueryEntityError> {
            entities
                .locate(id)
                .map_err(|_| QueryEntityError::NoSuchEntity(id))
        }

        split_impl(unsafe { &self.world.read_only().entities }, entity_id)
    }

    #[inline]
    fn update_filter_cache(&self, f_cache: &mut F::Cache<'w>, location: Location) {
        let tables = unsafe { &mut self.world.data_mut().tables };
        let table = unsafe { tables.get_unchecked_mut(location.table_id) };
        unsafe { F::update_table(&self.state.f_state, f_cache, table) };
    }

    #[inline]
    fn update_data_cache(&self, d_cache: &mut D::Cache<'w>, location: Location) {
        let tables = unsafe { &mut self.world.data_mut().tables };
        let table = unsafe { tables.get_unchecked_mut(location.table_id) };
        unsafe { D::update_table(&self.state.d_state, d_cache, table) };
    }

    fn contains_impl(&self, entity: EntityId) -> bool {
        let world = self.world;
        let this_run = self.this_run;
        let last_run = self.last_run;

        let Ok(location) = self.locate_entity(entity) else {
            return false;
        };

        if !self.contains_storage(location.table_id) {
            return false;
        }

        if F::ENABLE_ENTITY_FILTER {
            unsafe {
                let mut f_cache = F::build_cache(&self.state.f_state, world, last_run, this_run);
                self.update_filter_cache(&mut f_cache, location);
                if !F::filter(
                    &self.state.f_state,
                    &mut f_cache,
                    entity,
                    location.table_row,
                ) {
                    return false;
                }
            }
        }

        unsafe {
            let mut d_cache = D::build_cache(&self.state.d_state, world, last_run, this_run);
            self.update_data_cache(&mut d_cache, location);
            D::fetch(
                &self.state.d_state,
                &mut d_cache,
                entity,
                location.table_row,
            )
            .is_some()
        }
    }

    fn get_impl(&self, entity: EntityId) -> Result<D::Item<'w>, QueryEntityError> {
        let world = self.world;
        let this_run = self.this_run;
        let last_run = self.last_run;

        let location = self.locate_entity(entity)?;

        if !self.contains_storage(location.table_id) {
            return Err(QueryEntityError::QueryMismatch(entity));
        }

        if F::ENABLE_ENTITY_FILTER {
            unsafe {
                let mut f_cache = F::build_cache(&self.state.f_state, world, last_run, this_run);
                self.update_filter_cache(&mut f_cache, location);
                if !F::filter(
                    &self.state.f_state,
                    &mut f_cache,
                    entity,
                    location.table_row,
                ) {
                    return Err(QueryEntityError::QueryMismatch(entity));
                }
            }
        }

        unsafe {
            let mut d_cache = D::build_cache(&self.state.d_state, world, last_run, this_run);
            self.update_data_cache(&mut d_cache, location);
            D::fetch(
                &self.state.d_state,
                &mut d_cache,
                entity,
                location.table_row,
            )
            .ok_or(QueryEntityError::QueryMismatch(entity))
        }
    }

    fn get_many_mut_impl<const N: usize>(
        &self,
        entities: [EntityId; N],
    ) -> Result<[D::Item<'w>; N], QueryEntityError> {
        for i in 0..N {
            for j in 0..i {
                if entities[i] == entities[j] {
                    return Err(QueryEntityError::DuplicateEntity(entities[i]));
                }
            }
        }

        self.get_many_impl(&entities)
    }

    fn get_many_impl<const N: usize>(
        &self,
        entities: &[EntityId; N],
    ) -> Result<[D::Item<'w>; N], QueryEntityError> {
        let world = self.world;
        let this_run = self.this_run;
        let last_run = self.last_run;

        let mut values = [const { MaybeUninit::<D::Item<'w>>::uninit() }; N];

        let mut f_cache = unsafe { F::build_cache(&self.state.f_state, world, last_run, this_run) };
        let mut d_cache = unsafe { D::build_cache(&self.state.d_state, world, last_run, this_run) };

        for index in 0..N {
            let value = &mut values[index];
            let entity = entities[index];
            match self.get_with_cache_impl(entity, &mut f_cache, &mut d_cache) {
                Ok(item) => *value = MaybeUninit::new(item),
                Err(e) => {
                    // SAFETY: `values[..index]` were all initialized above.
                    for value in values.iter_mut().take(index) {
                        unsafe { ::core::ptr::drop_in_place(value.as_mut_ptr()) };
                    }
                    return Err(e);
                }
            }
        }

        unsafe { Ok(MaybeUninit::<[D::Item<'w>; N]>::from(values).assume_init()) }
    }

    fn get_with_cache_impl(
        &self,
        entity: EntityId,
        f_cache: &mut F::Cache<'w>,
        d_cache: &mut D::Cache<'w>,
    ) -> Result<D::Item<'w>, QueryEntityError> {
        let location = self.locate_entity(entity)?;

        if !self.contains_storage(location.table_id) {
            return Err(QueryEntityError::QueryMismatch(entity));
        }

        if F::ENABLE_ENTITY_FILTER {
            unsafe {
                self.update_filter_cache(f_cache, location);
                if !F::filter(&self.state.f_state, f_cache, entity, location.table_row) {
                    return Err(QueryEntityError::QueryMismatch(entity));
                }
            }
        }

        unsafe {
            self.update_data_cache(d_cache, location);
            D::fetch(&self.state.d_state, d_cache, entity, location.table_row)
                .ok_or(QueryEntityError::QueryMismatch(entity))
        }
    }
}

// -----------------------------------------------------------------------------
