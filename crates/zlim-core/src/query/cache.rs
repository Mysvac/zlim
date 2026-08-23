//! Per-world [`QueryState`] caching and the query entry points on [`World`]
//! and [`DeferredWorld`].
//!
//! Building a [`QueryState`] is comparatively expensive, so [`QueryCache`]
//! memoises one state per `(D, F)` pair per world.  `World::query` /
//! `World::single` (and their `_mut` variants) reuse the cached state
//! instead of rebuilding it on every call.
//!
//! # Locking design
//!
//! The cache must serve both exclusive (`&mut World`) and shared
//! (`&World`, possibly multi-threaded) access:
//!
//! - **`&mut World`** — no locking at all: exclusive access reaches the
//!   cache through [`Mutex::get_mut`], and the cached state is refreshed
//!   unconditionally.
//! - **`&World`** — a two-layer lock scheme:
//!
//!   1. The **outer lock** guards the cache map itself and is only needed
//!      for *inserting* a new [`QueryState`].
//!   2. The **inner lock** guards one slot and is only needed for
//!      *updating* that state.
//!
//!   Each slot lives behind a `Box`, so growing the outer `TypeMap` never
//!   moves the inner `Mutex`.  The cache only grows during a `&World`
//!   borrow (slots are never removed), so the outer lock can be released as
//!   soon as a slot is pinned down.
//!
//!   Tables cannot change without `&mut World`, so a [`QueryState`] that has
//!   been updated once stays up to date for the rest of the `&World` borrow.
//!   Like a `OnceLock`, the inner lock only needs to be held during the
//!   (usually no-op) update; afterwards the state is handed out lock-free.

use core::any::{Any, TypeId};
use core::fmt::Debug;
use std::sync::{Mutex, PoisonError};

use zlim_utils::ext::TypeMap;

use crate::query::{Query, QueryData, QueryFilter, QuerySingleError};
use crate::query::{QueryState, ReadOnlyQueryData, Single};
use crate::utils::DebugCheckedUnwrap;
use crate::world::{DeferredWorld, World};

// -----------------------------------------------------------------------------
// Slot
// -----------------------------------------------------------------------------

/// One cached slot: a type-erased [`QueryState`] behind a per-slot mutex.
///
/// - The outer `Box` keeps the [`Mutex`] at a stable address when the
///   enclosing `TypeMap` grows (reallocation moves the `Box` pointers, not
///   the boxes themselves).
/// - The inner `Box<dyn Any>` keeps the [`QueryState`] at a stable address,
///   so the state reference returned by [`query_state`] stays valid after
///   the inner guard is released.
type Slot = Mutex<Box<dyn Any + Send + Sync + 'static>>;

// -----------------------------------------------------------------------------
// QueryCache
// -----------------------------------------------------------------------------

/// Per-world memoisation of [`QueryState`] instances, keyed by
/// `TypeId::of::<QueryState<D, F>>()`.
///
/// Owned by [`World`]; see the module-level documentation for the locking
/// design.
pub struct QueryCache {
    pub(crate) cache: Mutex<TypeMap<Box<Slot>>>,
}

impl Debug for QueryCache {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("QueryCache").finish_non_exhaustive()
    }
}

impl QueryCache {
    /// Creates an empty cache.
    pub(crate) const fn new() -> Self {
        Self {
            cache: Mutex::new(TypeMap::new()),
        }
    }
}

// -----------------------------------------------------------------------------
// Shared (`&World`) access
// -----------------------------------------------------------------------------

/// Returns the cached [`QueryState`] for `(D, F)`, building and inserting it
/// on first access.
///
/// Under the shared `&World` borrow the cache is guarded by the two lock
/// layers described in the [module docs](self).  A state is only refreshed
/// when new tables have been registered since the last update
/// ([`QueryState::should_update`]); once up to date, it stays valid for the
/// rest of the borrow and is returned without any lock held.
#[inline(never)]
fn query_state<D, F>(world: &World) -> &QueryState<D, F>
where
    D: ReadOnlyQueryData + 'static,
    F: QueryFilter + 'static,
{
    let type_id = TypeId::of::<QueryState<D, F>>();

    // Pin the slot down under the outer lock, then release it.
    //
    // SAFETY: while `&World` is live, slots are only ever inserted, never
    // removed or replaced, and `**slot` is a `Box<Slot>` whose allocation
    // does not move when the `TypeMap` grows.  The address taken here
    // therefore stays valid after the outer guard is dropped.
    let slot: &Slot = {
        let guard = world
            .query_cache
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = guard.get(type_id) {
            let slot_ptr: *const Slot = &raw const **slot;
            ::core::mem::drop(guard);
            unsafe { &*slot_ptr }
        } else {
            // Build outside the lock: constructing a `QueryState` is
            // expensive and must not block other readers.
            ::core::mem::drop(guard);
            let state = QueryState::<D, F>::build(world);
            let new_slot: Box<Slot> = Box::new(Mutex::new(Box::new(state)));

            let mut guard = world
                .query_cache
                .cache
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            // Another thread may have inserted the same slot while we were
            // building; keep whichever one won the race.
            let slot = guard.get_or_insert(type_id, || new_slot);
            let slot_ptr: *const Slot = &raw const **slot;
            ::core::mem::drop(guard);
            unsafe { &*slot_ptr }
        }
    };

    // Update under the inner lock, then hand the state out lock-free.
    let mut guard = slot.lock().unwrap_or_else(PoisonError::into_inner);
    // SAFETY: the slot was created with exactly
    // `TypeId::of::<QueryState<D, F>>()`.
    let state = unsafe {
        guard
            .downcast_mut::<QueryState<D, F>>()
            .debug_checked_unwrap()
    };

    // Tables cannot change during the `&World` borrow, so the state only
    // needs updating once; subsequent calls observe `should_update == false`.
    if state.should_update(world) {
        ::core::hint::cold_path();
        state.update(world);
    }

    // SAFETY: the state was just brought up to date, and tables cannot
    // change for the rest of the borrow; the inner `Box` keeps its address
    // stable, so the reference is valid after the guard is dropped.
    let ptr: *const QueryState<D, F> = &raw const *state;
    ::core::mem::drop(guard);

    unsafe { &*ptr }
}

// -----------------------------------------------------------------------------
// Exclusive (`&mut World`) access
// -----------------------------------------------------------------------------

/// Returns the cached [`QueryState`] for `(D, F)`, building and inserting it
/// on first access and refreshing it on every call.
///
/// The caller holds exclusive access (`&mut World`), so the cache is reached
/// through [`Mutex::get_mut`] without locking, and the state is updated
/// unconditionally to reflect any structural changes made since the last
/// call.
#[inline(never)]
fn query_state_mut<D, F>(world: &mut World) -> &QueryState<D, F>
where
    D: QueryData + 'static,
    F: QueryFilter + 'static,
{
    let type_id = TypeId::of::<QueryState<D, F>>();
    let cell = world.cell();
    let world = unsafe { cell.data_mut() };

    let cache = world
        .query_cache
        .cache
        .get_mut()
        .unwrap_or_else(PoisonError::into_inner);

    let slot: &mut Box<Slot> = cache.get_mut(type_id).unwrap_or_else(|| {
        let world = unsafe { cell.data_mut() };
        let cache = world
            .query_cache
            .cache
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner);
        let state = QueryState::<D, F>::build(unsafe { cell.read_only() });
        let new_slot: Box<Slot> = Box::new(Mutex::new(Box::new(state)));
        cache.get_or_insert(type_id, || new_slot)
    });

    // SAFETY: the slot was created with exactly
    // `TypeId::of::<QueryState<D, F>>()`.
    let state = unsafe {
        slot.get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .downcast_mut::<QueryState<D, F>>()
            .debug_checked_unwrap()
    };
    state.update(unsafe { cell.read_only() });

    state
}

// -----------------------------------------------------------------------------
// World — read-only queries
// -----------------------------------------------------------------------------

impl World {
    /// Creates a read-only [`Query`] over the world.
    ///
    /// The underlying [`QueryState`] is fetched from (or built into) the
    /// world's [`QueryCache`] and only refreshed when new tables have been
    /// registered since the last use.
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
    /// let mut world = World::alloc();
    /// world.spawn(Position { x: 1.0, y: 2.0 }, None);
    ///
    /// let total: f32 = world.query::<&Position, ()>().iter().map(|p| p.x).sum();
    /// assert_eq!(total, 1.0);
    /// ```
    #[inline(always)]
    pub fn query<D, F>(&self) -> Query<'_, '_, D, F>
    where
        D: ReadOnlyQueryData + 'static,
        F: QueryFilter + 'static,
    {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let world = self.cell();
        let state = query_state::<D, F>(self);
        // SAFETY: `world` is this world, `state` was built from this world's
        // cache with matching `D`/`F`, and the ticks belong to this world's
        // tick stream.
        unsafe { Query::new(world, state, last_run, this_run) }
    }

    /// Runs the query and returns the single matching item.
    ///
    /// Returns [`QuerySingleError::NoEntities`] when nothing matches and
    /// [`QuerySingleError::MultipleEntities`] when more than one entity
    /// matches.  Wrap the parameter in an `Option` to make a missing (or
    /// ambiguous) match non-fatal — see [`Single`].
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
    /// let mut world = World::alloc();
    /// world.spawn(Player { health: 10 }, None);
    ///
    /// let player = world.single::<&Player, ()>().unwrap();
    /// assert_eq!(player.health, 10);
    /// ```
    #[inline(always)]
    pub fn single<D, F>(&self) -> Result<Single<'_, D, F>, QuerySingleError>
    where
        D: ReadOnlyQueryData + 'static,
        F: QueryFilter + 'static,
    {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let world = self.cell();
        let state = query_state::<D, F>(self);
        // SAFETY: same reasoning as [`World::query`](Self::query).
        unsafe { Single::new(world, state, last_run, this_run) }
    }
}

// -----------------------------------------------------------------------------
// World — mutable queries
// -----------------------------------------------------------------------------

impl World {
    /// Creates a [`Query`] with mutable access over the world.
    ///
    /// Unlike [`query`](Self::query), the cached [`QueryState`] is refreshed
    /// on every call, since the world may have changed structurally since
    /// the last use.  Requires exclusive access, so no locking is performed.
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
    /// let mut world = World::alloc();
    /// world.spawn(Position { x: 1.0, y: 2.0 }, None);
    ///
    /// let mut query = world.query_mut::<&mut Position, ()>();
    /// for position in query.iter_mut() {
    ///     position.into_inner().x += 1.0;
    /// }
    /// drop(query);
    ///
    /// assert_eq!(world.query::<&Position, ()>().iter().next().unwrap().x, 2.0);
    /// ```
    #[inline(always)]
    pub fn query_mut<D, F>(&mut self) -> Query<'_, '_, D, F>
    where
        D: QueryData + 'static,
        F: QueryFilter + 'static,
    {
        let last_run = self.last_run();
        let this_run = self.this_run_fast();
        let world = self.cell();
        let state = query_state_mut::<D, F>(unsafe { world.data_mut() });
        // SAFETY: exclusive access prevents aliasing violations.
        unsafe { Query::new(world, state, last_run, this_run) }
    }

    /// Runs the query with mutable access and returns the single matching
    /// item.
    ///
    /// Returns [`QuerySingleError::NoEntities`] when nothing matches and
    /// [`QuerySingleError::MultipleEntities`] when more than one entity
    /// matches.  Wrap the parameter in an `Option` to make a missing (or
    /// ambiguous) match non-fatal — see [`Single`].
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
    /// let mut world = World::alloc();
    /// world.spawn(Player { health: 10 }, None);
    ///
    /// let player = world.single_mut::<&mut Player, ()>().unwrap();
    /// player.into_inner().into_inner().health = 5;
    /// assert_eq!(world.query::<&Player, ()>().iter().next().unwrap().health, 5);
    /// ```
    #[inline(always)]
    pub fn single_mut<D, F>(&mut self) -> Result<Single<'_, D, F>, QuerySingleError>
    where
        D: QueryData + 'static,
        F: QueryFilter + 'static,
    {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let world = self.cell();
        let state = query_state_mut::<D, F>(unsafe { world.data_mut() });
        // SAFETY: exclusive access prevents aliasing violations.
        unsafe { Single::new(world, state, last_run, this_run) }
    }
}

// -----------------------------------------------------------------------------
// DeferredWorld — mutable queries
// -----------------------------------------------------------------------------

impl DeferredWorld<'_> {
    /// Creates a [`Query`] with mutable access over the deferred world.
    ///
    /// Like [`World::query_mut`], the cached [`QueryState`] is refreshed on
    /// every call.
    #[inline(always)]
    pub fn query_mut<D, F>(&mut self) -> Query<'_, '_, D, F>
    where
        D: QueryData + 'static,
        F: QueryFilter + 'static,
    {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let world = self.cell();
        let state = query_state_mut::<D, F>(unsafe { world.data_mut() });
        // SAFETY: exclusive access prevents aliasing violations.
        unsafe { Query::new(world, state, last_run, this_run) }
    }

    /// Runs the query with mutable access and returns the single matching
    /// item.
    ///
    /// Returns [`QuerySingleError::NoEntities`] when nothing matches and
    /// [`QuerySingleError::MultipleEntities`] when more than one entity
    /// matches.  Wrap the parameter in an `Option` to make a missing (or
    /// ambiguous) match non-fatal — see [`Single`].
    #[inline(always)]
    pub fn single_mut<D, F>(&mut self) -> Result<Single<'_, D, F>, QuerySingleError>
    where
        D: QueryData + 'static,
        F: QueryFilter + 'static,
    {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let world = self.cell();
        let state = query_state_mut::<D, F>(unsafe { world.data_mut() });
        // SAFETY: exclusive access prevents aliasing violations.
        unsafe { Single::new(world, state, last_run, this_run) }
    }
}

// -----------------------------------------------------------------------------
