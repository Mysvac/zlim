//! Marker [`SystemParam`]s for exclusive and non-send systems.

use core::marker::PhantomData;

use crate::system::{AccessTable, SystemParam, SystemParamError};
use crate::tick::Tick;
use crate::world::{DeferredWorld, World, WorldCell};

unsafe impl<T: ?Sized> SystemParam for PhantomData<T> {
    type State = ();
    type Item<'world, 'state> = PhantomData<T>;

    const DEFERRED: bool = false;
    const NON_SEND: bool = false;
    const EXCLUSIVE: bool = false;

    #[inline(always)]
    fn init_state(_: &World) -> Self::State {}

    #[inline(always)]
    fn register_access(_: &Self::State, _: &mut AccessTable, _: bool) -> bool {
        true
    }

    #[inline(always)]
    unsafe fn build_param<'w, 's>(
        _state: &'s mut Self::State,
        _world: WorldCell<'w>,
        _last_run: Tick,
        _this_run: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        Ok(PhantomData)
    }

    #[inline(always)]
    fn queue_deferred(_: &mut Self::State, _: DeferredWorld) {}

    #[inline(always)]
    fn apply_deferred(_: &mut Self::State, _: &mut World) {}
}

/// A zero-sized [`SystemParam`] that marks a system as thread-affine (`NON_SEND`).
///
/// Adding this parameter to a system signature tells the scheduler the system
/// must run on the thread where it was created; the parameter itself fetches
/// no data.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// fn main_thread_system(_: NonSendMarker) -> bool {
///     // The scheduler only runs this system on its creation thread.
///     true
/// }
///
/// let mut world = World::alloc();
/// assert!(world.invoke_once(main_thread_system, ()).unwrap());
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct NonSendMarker;

unsafe impl SystemParam for NonSendMarker {
    type State = ();
    type Item<'world, 'state> = NonSendMarker;

    const DEFERRED: bool = false;
    const NON_SEND: bool = true;
    const EXCLUSIVE: bool = false;

    #[inline(always)]
    fn init_state(_: &World) -> Self::State {}

    #[inline(always)]
    fn register_access(_: &Self::State, _: &mut AccessTable, _: bool) -> bool {
        true
    }

    #[inline(always)]
    unsafe fn build_param<'w, 's>(
        _state: &'s mut Self::State,
        _world: WorldCell<'w>,
        _last_run: Tick,
        _this_run: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        Ok(NonSendMarker)
    }

    #[inline(always)]
    fn queue_deferred(_: &mut Self::State, _: DeferredWorld) {}

    #[inline(always)]
    fn apply_deferred(_: &mut Self::State, _: &mut World) {}
}

/// A zero-sized [`SystemParam`] that marks a system as requiring exclusive
/// world access.
///
/// Adding this parameter to a system signature tells the scheduler the system
/// must run with exclusive access to the whole [`World`]; the parameter itself
/// fetches no data.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// fn exclusive_system(_: ExclusiveMarker) -> bool {
///     // No other system runs in parallel with this one.
///     true
/// }
///
/// let mut world = World::alloc();
/// assert!(world.invoke_once(exclusive_system, ()).unwrap());
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct ExclusiveMarker;

unsafe impl SystemParam for ExclusiveMarker {
    type State = ();
    type Item<'world, 'state> = ExclusiveMarker;

    const DEFERRED: bool = false;
    const NON_SEND: bool = false;
    const EXCLUSIVE: bool = true;

    #[inline(always)]
    fn init_state(_: &World) -> Self::State {}

    #[inline(always)]
    fn register_access(_: &Self::State, _: &mut AccessTable, _: bool) -> bool {
        true
    }

    #[inline(always)]
    unsafe fn build_param<'w, 's>(
        _state: &'s mut Self::State,
        _world: WorldCell<'w>,
        _last_run: Tick,
        _this_run: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        Ok(ExclusiveMarker)
    }

    #[inline(always)]
    fn queue_deferred(_: &mut Self::State, _: DeferredWorld) {}

    #[inline(always)]
    fn apply_deferred(_: &mut Self::State, _: &mut World) {}
}
