//! The time clock resources, the per-world driver, and per-frame snapshots.

use crate::borrow::ResMut;
use crate::world::World;

use crate::time::{Fixed, Real, Time, TimeState, Virtual};
use crate::time::{TimeSnapshot, TimeUpdateStrategy};

// -----------------------------------------------------------------------------
// Basic

impl World {
    /// Returns the world's game clock ([`Time`]), if present.
    ///
    /// The clock resources are installed at [`World::alloc`] time and
    /// advanced once per frame by [`World::refresh_metadata`].  Systems
    /// usually read this clock as a resource (`Res<Time>`); this accessor
    /// serves the same data through the world's internal `TimeCache`.
    ///
    /// Returns `None` only if the resource was explicitly removed.
    #[inline]
    pub fn time(&self) -> Option<&Time> {
        self.time_cache.time()
    }

    /// Returns the wall-clock resource ([`Time<Real>`]), if present.
    ///
    /// Real time is never scaled or paused; see [`Time<Virtual>`] for game
    /// time with pausing and speed control.
    ///
    /// [`Time<Real>`]: crate::time::Real
    /// [`Time<Virtual>`]: crate::time::Virtual
    #[inline]
    pub fn real_time(&self) -> Option<&Time<Real>> {
        self.time_cache.real_time()
    }

    /// Returns the fixed-timestep clock ([`Time<Fixed>`]), if present.
    ///
    /// The fixed clock advances only in whole timesteps, one at a time via
    /// [`World::step_fixed`].
    ///
    /// [`Time<Fixed>`]: crate::time::Fixed
    #[inline]
    pub fn fixed_time(&self) -> Option<&Time<Fixed>> {
        self.time_cache.fixed_time()
    }

    /// Returns the virtual clock ([`Time<Virtual>`]), if present.
    ///
    /// Virtual time supports pausing and speed scaling; the game clock
    /// ([`Time`]) mirrors it every frame.
    ///
    /// [`Time<Virtual>`]: crate::time::Virtual
    #[inline]
    pub fn virtual_time(&self) -> Option<&Time<Virtual>> {
        self.time_cache.virtual_time()
    }

    /// Returns the engine's per-frame time state ([`TimeState`]), if present.
    ///
    /// `frame` counts the frames driven by [`World::refresh_metadata`];
    /// `accumulator` holds the fixed-step remainder left after the last
    /// [`World::step_fixed`].
    #[inline]
    pub fn time_state(&self) -> Option<&TimeState> {
        self.time_cache.state()
    }

    /// Returns the current fixed-interpolation snapshot ([`TimeSnapshot`]),
    /// if present.
    ///
    /// The snapshot is refreshed by each [`World::step_fixed`] call and
    /// carries the `prev` / `curr` / `alpha` triple used to interpolate
    /// between fixed steps (e.g. in a render world).
    #[inline]
    pub fn time_snapshot(&self) -> Option<&TimeSnapshot> {
        self.time_cache.snapshot()
    }

    /// Assign the given value to the current world's [`Time`].
    ///
    /// In most scenarios, [`Time`] is a mirror of [`Time<Virtual>`].
    ///
    /// If you need to adjust the meaning of [`Time`], you can use this function
    /// to assign values. This is faster than accessing through resource api.
    #[inline]
    pub fn apply_time(&mut self, time: &Time) {
        *self.time_mut::<true>().into_inner() = *time;
    }
}

// -----------------------------------------------------------------------------
// update_times

impl World {
    #[inline]
    pub(crate) fn time_mut<const INIT: bool>(&mut self) -> ResMut<'_, Time> {
        let last_run = self.last_run();
        let this_run = self.this_run_fast();
        self.time_cache.time_mut::<INIT>(last_run, this_run)
    }

    #[inline]
    pub(crate) fn real_time_mut<const INIT: bool>(&mut self) -> ResMut<'_, Time<Real>> {
        let last_run = self.last_run();
        let this_run = self.this_run_fast();
        self.time_cache.real_time_mut::<INIT>(last_run, this_run)
    }

    #[inline]
    pub(crate) fn fixed_time_mut<const INIT: bool>(&mut self) -> ResMut<'_, Time<Fixed>> {
        let last_run = self.last_run();
        let this_run = self.this_run_fast();
        self.time_cache.fixed_time_mut::<INIT>(last_run, this_run)
    }

    #[inline]
    pub(crate) fn virtual_time_mut<const INIT: bool>(&mut self) -> ResMut<'_, Time<Virtual>> {
        let last_run = self.last_run();
        let this_run = self.this_run_fast();
        self.time_cache.virtual_time_mut::<INIT>(last_run, this_run)
    }

    #[inline]
    pub(crate) fn time_state_mut<const INIT: bool>(&mut self) -> ResMut<'_, TimeState> {
        let last_run = self.last_run();
        let this_run = self.this_run_fast();
        self.time_cache.state_mut::<INIT>(last_run, this_run)
    }

    #[inline]
    pub(crate) fn time_snapshot_mut<const INIT: bool>(&mut self) -> ResMut<'_, TimeSnapshot> {
        let last_run = self.last_run();
        let this_run = self.this_run_fast();
        self.time_cache.snapshot_mut::<INIT>(last_run, this_run)
    }
}

impl World {
    /// Drives the world's clocks by one frame.
    ///
    /// Real time is advanced per the [`TimeUpdateStrategy`],
    ///
    /// Virtual time is derived from the real delta (clamped, scaled, paused),
    /// and the delta is accumulated into the fixed-step remainder.
    ///
    /// Called by [`World::refresh_metadata`]; does nothing when the strategy
    /// is [`TimeUpdateStrategy::None`] (worlds whose time is supplied externally,
    /// e.g. the render world).
    pub(crate) fn update_times(world: &mut World) {
        let strategy = world.time_strategy;

        if matches!(strategy, TimeUpdateStrategy::None) {
            return;
        }

        let timestep = world.fixed_time_mut::<true>().timestep();
        let mut real = world.real_time_mut::<true>();
        match strategy {
            TimeUpdateStrategy::None => unsafe { core::hint::unreachable_unchecked() },
            TimeUpdateStrategy::Automatic => real.update(),
            TimeUpdateStrategy::ManualInstant(instant) => real.update_with_instant(instant),
            TimeUpdateStrategy::ManualDuration(duration) => real.update_with_duration(duration),
            TimeUpdateStrategy::FixedTimesteps(factor) => {
                real.update_with_duration(timestep * factor)
            }
        }
        let real_delta = real.delta();

        let mut virt = world.virtual_time_mut::<true>();
        virt.advance_with_raw_delta(real_delta);
        let effective = virt.delta();

        *world.time_mut::<true>() = virt.as_generic();

        let state = world.time_state_mut::<true>().into_inner();
        state.frame += 1;
        state.accumulator += effective;
    }
}

// -----------------------------------------------------------------------------
// step_fixed

impl World {
    /// Advances the fixed clock by one whole step if the accumulated
    /// remainder allows it, refreshing the [`TimeSnapshot`] in the process.
    ///
    /// Returns `true` if a step was taken; `false` when the accumulator holds
    /// less than one timestep (call again after the next
    /// [`World::refresh_metadata`]).
    ///
    /// # Example
    ///
    /// ```rust
    /// # use std::time::Duration;
    /// # use zlim_core::prelude::*;
    /// # use zlim_core::time::{Fixed, Time, TimeUpdateStrategy};
    /// #
    /// let mut world = World::alloc();
    /// world.set_time_strategy(TimeUpdateStrategy::ManualDuration(
    ///     Duration::from_millis(16),
    /// ));
    ///
    /// // The clock resources hold no data until the first refresh (or an
    /// // explicit `resource_mut_or_init`), so set the timestep up front.
    /// world
    ///     .resource_mut_or_init::<Time<Fixed>>()
    ///     .set_timestep(Duration::from_millis(16));
    ///
    /// // The first refresh initializes the clocks and establishes the
    /// // baseline (producing no delta); the second advances by 16 ms.
    /// World::refresh_metadata(&mut world); // baseline
    /// World::refresh_metadata(&mut world); // +16 ms of virtual time
    ///
    /// assert!(World::step_fixed(&mut world));
    ///
    /// assert_eq!(
    ///     world.fixed_time().unwrap().elapsed(),
    ///     Duration::from_millis(16)
    /// );
    /// ```
    pub fn step_fixed(world: &mut World) -> bool {
        let cell = world.cell();
        let world_1 = unsafe { cell.data_mut() };
        let world_2 = unsafe { cell.data_mut() };
        let world_3 = unsafe { cell.data_mut() };

        let state = world_1.time_state_mut::<false>();
        let fixed = world_2.fixed_time_mut::<false>();
        let timestep = fixed.timestep();

        if state.accumulator < fixed.timestep() {
            return false;
        }

        let state: &mut TimeState = state.into_inner();
        let fixed: &mut Time<Fixed> = fixed.into_inner();

        state.prev = *fixed;
        state.accumulator -= fixed.timestep();

        fixed.advance_by(timestep);

        let snapshot = world_3.time_snapshot_mut::<true>();
        *snapshot.into_inner() = TimeSnapshot {
            prev: state.prev,
            curr: *fixed,
            frame: state.frame,
            alpha: state.accumulator.as_secs_f32() / fixed.timestep().as_secs_f32(),
        };

        true
    }
}

// -----------------------------------------------------------------------------
