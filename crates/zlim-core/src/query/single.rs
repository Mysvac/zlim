//! The `Single` system parameter — guarantees exactly one query match.

use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

use super::iter::QueryIter;
use super::{QueryData, QueryFilter, QuerySingleError, QueryState};
use crate::error::Severity;
use crate::system::{AccessTable, SystemParam, SystemParamError};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

/// A system parameter that guarantees exactly one matching query item.
///
/// This wraps query single-target access and fails parameter construction
/// with [`SystemParamError`] when the query has zero or multiple matches.
///
/// Use this when your system semantics require one and only one target.
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::query::Single;
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Player { health: u32 }
///
/// fn update_player(player: Single<&mut Player>) {
///     let player = player.into_inner().into_inner();
///     player.health = player.health.saturating_sub(1);
/// }
///
/// let mut world = World::alloc();
/// world.spawn(Player { health: 10 }, None);
///
/// update_player(world.single_mut::<&mut Player, ()>().unwrap());
/// assert_eq!(world.query::<&Player, ()>().iter().next().unwrap().health, 9);
/// ```
///
/// When no entity matches the query — or more than one matches — the
/// corresponding system returns a `Warning`-level error and is skipped.
/// The default error handler logs the error instead of panicking.
///
/// If the entity may not exist, or you want to run some logic when the
/// condition is not satisfied, wrap the parameter in an `Option`:
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::query::Single;
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Player { health: u32 }
///
/// fn update_player(player: Option<Single<&mut Player>>) {
///     if let Some(player) = player {
///         let player = player.into_inner().into_inner();
///         player.health = player.health.saturating_sub(1);
///     }
/// }
///
/// let mut world = World::alloc();
/// world.spawn(Player { health: 10 }, None);
///
/// update_player(world.single_mut::<&mut Player, ()>().ok());
/// assert_eq!(world.query::<&Player, ()>().iter().next().unwrap().health, 9);
/// ```
///
/// In that case, when no entity matches (or more than one matches), the
/// parameter resolves to `None` and the system runs normally.
#[repr(transparent)]
pub struct Single<'world, D: QueryData, F: QueryFilter = ()> {
    pub(super) item: D::Item<'world>,
    pub(super) _marker: PhantomData<F>,
}

// -----------------------------------------------------------------------------
// Construction

impl<'w, D: QueryData, F: QueryFilter> Single<'w, D, F> {
    /// Builds the single item for this query.
    ///
    /// Returns [`QuerySingleError::NoEntities`] when nothing matches and
    /// [`QuerySingleError::MultipleEntities`] when more than one entity
    /// matches.
    ///
    /// # Safety
    /// - `world` must be the same world used to build `state`.
    /// - The caller must ensure no aliasing violations are introduced.
    #[inline(never)]
    pub unsafe fn new<'s>(
        world: WorldCell<'w>,
        state: &'s QueryState<D, F>,
        last_run: Tick,
        this_run: Tick,
    ) -> Result<Self, QuerySingleError> {
        let mut iter = unsafe { QueryIter::new(world, state, last_run, this_run) };

        let Some(item) = iter.next() else {
            return Err(QuerySingleError::NoEntities);
        };

        if iter.next().is_some() {
            return Err(QuerySingleError::MultipleEntities);
        }

        Ok(Single {
            item,
            _marker: PhantomData,
        })
    }
}

// -----------------------------------------------------------------------------
// SystemParam

unsafe impl<D: QueryData + 'static, F: QueryFilter + 'static> SystemParam for Single<'_, D, F> {
    type State = QueryState<D, F>;
    type Item<'world, 'state> = Single<'world, D, F>;

    const DEFERRED: bool = false;
    const NON_SEND: bool = false;
    const EXCLUSIVE: bool = false;

    fn init_state(world: &World) -> Self::State {
        QueryState::build(world)
    }

    fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        state.register_access(table, strict)
    }

    #[inline(never)]
    unsafe fn build_param<'w, 's>(
        state: &'s mut Self::State,
        world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        state.update(unsafe { world.read_only() });

        match unsafe { Single::new(world, state, last_run, this_run) } {
            Ok(ret) => Ok(ret),
            Err(e) => {
                core::hint::cold_path();
                let info = e.to_string();
                Err(SystemParamError::new::<Self>(info).with_severity(Severity::Warning))
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Basic

unsafe impl<D: QueryData + 'static, F: QueryFilter + 'static> SystemParam
    for Option<Single<'_, D, F>>
{
    type State = QueryState<D, F>;
    type Item<'world, 'state> = Option<Single<'world, D, F>>;

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

        match unsafe { Single::new(world, state, last_run, this_run) } {
            Ok(ret) => Ok(Some(ret)),
            Err(_) => Ok(None),
        }
    }
}

// -----------------------------------------------------------------------------
// Basic

impl<'w, D: QueryData, F: QueryFilter> Single<'w, D, F> {
    /// Consumes this wrapper and returns the inner query item.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::query::Single;
    /// use zlim_reflect::derive::TypePath;
    ///
    /// #[derive(TypePath, Component, Clone)]
    /// struct Player { health: u32 }
    ///
    /// fn read_health(player: Single<&Player>) -> u32 {
    ///     player.into_inner().health
    /// }
    ///
    /// let mut world = World::alloc();
    /// world.spawn(Player { health: 10 }, None);
    ///
    /// assert_eq!(read_health(world.single::<&Player, ()>().unwrap()), 10);
    /// ```
    #[inline(always)]
    pub fn into_inner(self) -> D::Item<'w> {
        self.item
    }
}

impl<'world, D: QueryData, F: QueryFilter> Deref for Single<'world, D, F> {
    type Target = D::Item<'world>;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

impl<'world, D: QueryData, F: QueryFilter> DerefMut for Single<'world, D, F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.item
    }
}

// -----------------------------------------------------------------------------
