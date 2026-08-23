//! The [`Local`] system-local state parameter.

use core::ops::{Deref, DerefMut};

use crate::system::access::AccessTable;
use crate::system::{SystemParam, SystemParamError};
use crate::world::{DeferredWorld, FromWorld, World, WorldCell};

/// A system-local variable.
///
/// When used as a system parameter, each compiled system instance owns one
/// independent value of `T`. This makes `Local<T>` a convenient alternative to
/// global `static` state for per-system counters, caches, and temporary state.
///
/// The value is initialized through [`FromWorld`] during system initialization
/// and then reused across subsequent runs of that system.  Because the value
/// lives in the system's persistent state, it survives across runs when the
/// system is cached in a world (see [`World::insert_system`]).
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// fn count_runs(mut runs: Local<u32>) -> u32 {
///     *runs += 1;
///     *runs
/// }
///
/// let mut world = World::alloc();
/// let handle = world.insert_system(count_runs);
/// assert_eq!(world.run_system_handle(handle, ()).unwrap(), 1);
/// assert_eq!(world.run_system_handle(handle, ()).unwrap(), 2); // `runs` is now 2
/// ```
///
/// [`World::insert_system`]: crate::world::World::insert_system
#[derive(Debug)]
#[repr(transparent)]
pub struct Local<'s, T: FromWorld + Send + Sync>(&'s mut T);

impl<'s, T: FromWorld + Send + Sync> Deref for Local<'s, T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<'s, T: FromWorld + Send + Sync> DerefMut for Local<'s, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

unsafe impl<T: FromWorld + Send + Sync> SystemParam for Local<'_, T> {
    type State = T;

    type Item<'world, 'state> = Local<'state, T>;

    const DEFERRED: bool = false;
    const NON_SEND: bool = false;
    const EXCLUSIVE: bool = false;

    fn init_state(world: &World) -> Self::State {
        T::from_world(world)
    }

    #[inline(always)]
    fn register_access(_: &Self::State, _: &mut AccessTable, _: bool) -> bool {
        true
    }

    #[inline(always)]
    unsafe fn build_param<'w, 's>(
        state: &'s mut Self::State,
        _world: WorldCell<'w>,
        _last_run: crate::tick::Tick,
        _this_run: crate::tick::Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        Ok(Local(state))
    }

    #[inline(always)]
    fn queue_deferred(_: &mut Self::State, _: DeferredWorld) {}

    #[inline(always)]
    fn apply_deferred(_: &mut Self::State, _: &mut World) {}
}
