//! The [`SystemTick`] parameter exposing change-detection ticks.

use crate::system::{AccessTable, SystemParam, SystemParamError};
use crate::tick::Tick;
use crate::world::{DeferredWorld, World, WorldCell};

/// A [`SystemParam`] exposing the change-detection ticks for the current run.
///
/// `last_run` and `this_run` delimit the change-detection window: a component
/// is considered changed when its write tick falls in the interval
/// `(last_run, this_run]`.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::system::SystemTick;
///
/// fn report_ticks(ticks: SystemTick) {
///     // A never-run system starts with a baseline of tick 0, and the
///     // first run advances the world's clock to tick 1.
///     assert_eq!(ticks.last_run, Tick::new(0));
///     assert_eq!(ticks.this_run, Tick::new(1));
/// }
///
/// let mut world = World::alloc();
/// world.run_once(report_ticks, ()).unwrap();
/// ```
#[derive(Debug, Clone, Copy)]
pub struct SystemTick {
    /// The tick when the system last completed a run.
    pub last_run: Tick,
    /// The tick for the current run.
    pub this_run: Tick,
}

// SAFETY: `SystemTick` doesn't require any world access
unsafe impl SystemParam for SystemTick {
    type State = ();
    type Item<'world, 'state> = SystemTick;

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
        last_run: Tick,
        this_run: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        Ok(Self { last_run, this_run })
    }

    #[inline(always)]
    fn queue_deferred(_: &mut Self::State, _: DeferredWorld) {}

    #[inline(always)]
    fn apply_deferred(_: &mut Self::State, _: &mut World) {}
}
