//! Timing-based run conditions.
//!
//! Each function returns a system that reads a time resource and reports
//! whether the condition currently holds.  They are designed to be plugged
//! into run-condition APIs (`If` and friends) or used directly as
//! boolean-returning systems.

use core::time::Duration;

use crate::borrow::Res;

use super::{Real, Time, Timer, TimerMode, Virtual};

/// Run condition that is active on a regular time interval.
///
/// Using [`Time`] to advance the timer. The timer ticks at the rate
/// of [`Time<Virtual>::relative_speed`].
///
/// # Examples
///
/// The returned value is a system, that can be used as a job condition:
///
/// ```rust, no_run
/// # use std::time::Duration;
/// # use zlim_core::prelude::*;
/// # use zlim_core::time::on_timer;
/// #
/// #[job_fn(type = Example, run_if = on_timer(Duration::from_secs(1)))]
/// fn example_job() {
///     std::println!("Hello World!");
/// }
/// ```
pub fn on_timer(duration: Duration) -> impl FnMut(Res<Time>) -> bool + Clone {
    let mut timer = Timer::new(duration, TimerMode::Repeating);
    move |time: Res<Time>| -> bool {
        timer.tick(time.delta());
        timer.just_finished()
    }
}

/// Run condition that is active on a regular time interval.
///
/// Using [`Time<Real>`] to advance the timer. The timer ticks are not scaled.
///
/// # Examples
///
/// The returned value is a system, that can be used as a job condition:
///
/// ```rust, no_run
/// # use std::time::Duration;
/// # use zlim_core::prelude::*;
/// # use zlim_core::time::on_real_timer;
/// #
/// #[job_fn(type = Example, run_if = on_real_timer(Duration::from_secs(1)))]
/// fn example_job() {
///     std::println!("Hello World!");
/// }
/// ```
pub fn on_real_timer(duration: Duration) -> impl FnMut(Res<Time<Real>>) -> bool + Clone {
    let mut timer = Timer::new(duration, TimerMode::Repeating);
    move |time: Res<Time<Real>>| {
        timer.tick(time.delta());
        timer.just_finished()
    }
}

/// Run condition that is active *once* after the specified delay.
///
/// Using [`Time`] to advance the timer.
/// The timer ticks at the rate of [`Time<Virtual>::relative_speed`].
///
/// # Examples
///
/// ```rust, no_run
/// # use std::time::Duration;
/// # use zlim_core::prelude::*;
/// # use zlim_core::time::once_after_delay;
/// #
/// #[job_fn(type = Example, run_if = once_after_delay(Duration::from_secs(1)))]
/// fn example_job() {
///     std::println!("Hello World!");
/// }
/// ```
pub fn once_after_delay(duration: Duration) -> impl FnMut(Res<Time>) -> bool + Clone {
    let mut timer = Timer::new(duration, TimerMode::Once);
    move |time: Res<Time>| -> bool {
        timer.tick(time.delta());
        timer.just_finished()
    }
}

/// Run condition that is active *once* after the specified delay.
///
/// Using [`Time<Real>`] to advance the timer. The timer ticks are not scaled.
///
/// Behaves like [`once_after_delay`], but driven by the real clock.
///
/// # Examples
///
/// ```rust, no_run
/// # use std::time::Duration;
/// # use zlim_core::prelude::*;
/// # use zlim_core::time::once_after_real_delay;
/// #
/// #[job_fn(type = Example, run_if = once_after_real_delay(Duration::from_secs(1)))]
/// fn example_job() {
///     std::println!("Hello World!");
/// }
/// ```
pub fn once_after_real_delay(duration: Duration) -> impl FnMut(Res<Time<Real>>) -> bool + Clone {
    let mut timer = Timer::new(duration, TimerMode::Once);
    move |time: Res<Time<Real>>| {
        timer.tick(time.delta());
        timer.just_finished()
    }
}

/// Run condition that is active *indefinitely* after the specified delay.
///
/// Using [`Time`] to advance the timer.
/// The timer ticks at the rate of [`Time<Virtual>::relative_speed`].
///
/// # Examples
///
/// ```rust, no_run
/// # use std::time::Duration;
/// # use zlim_core::prelude::*;
/// # use zlim_core::time::repeating_after_delay;
/// #
/// #[job_fn(type = Example, run_if = repeating_after_delay(Duration::from_secs(1)))]
/// fn example_job() {
///     std::println!("Hello World!");
/// }
/// ```
pub fn repeating_after_delay(duration: Duration) -> impl FnMut(Res<Time>) -> bool + Clone {
    let mut timer = Timer::new(duration, TimerMode::Once);
    move |time: Res<Time>| {
        timer.tick(time.delta());
        timer.is_finished()
    }
}

/// Run condition that is active *indefinitely* after the specified delay.
///
/// using [`Time<Real>`] to advance the timer. The timer ticks are not scaled.
///
/// Behaves like [`repeating_after_delay`], but driven by the real clock.
///
/// # Examples
///
/// ```rust, no_run
/// # use std::time::Duration;
/// # use zlim_core::prelude::*;
/// # use zlim_core::time::repeating_after_real_delay;
/// #
/// #[job_fn(type = Example, run_if = repeating_after_real_delay(Duration::from_secs(1)))]
/// fn example_job() {
///     std::println!("Hello World!");
/// }
/// ```
pub fn repeating_after_real_delay(
    duration: Duration,
) -> impl FnMut(Res<Time<Real>>) -> bool + Clone {
    let mut timer = Timer::new(duration, TimerMode::Once);
    move |time: Res<Time<Real>>| {
        timer.tick(time.delta());
        timer.is_finished()
    }
}

/// Run condition that is active when the [`Time<Virtual>`] clock is paused.
///
/// # Examples
///
/// ```rust, no_run
/// # use zlim_core::prelude::*;
/// # use zlim_core::time::paused;
/// #
/// #[job_fn(type = Example, run_if = paused)]
/// fn example_job() {
///     std::println!("Hello World!");
/// }
/// ```
pub fn paused(time: Res<Time<Virtual>>) -> bool {
    time.is_paused()
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::world::World;

    use super::super::Time;
    use super::super::{Real, Virtual};
    use super::{
        on_real_timer, on_timer, once_after_delay, once_after_real_delay, paused,
        repeating_after_delay, repeating_after_real_delay,
    };

    #[test]
    fn on_timer_fires_on_interval() {
        let mut world = World::alloc();
        world.init_resource::<Time>();

        // Reports `true` on the frame where the 1s interval elapses.
        let condition = on_timer(Duration::from_secs(1));

        for _ in 0..5 {
            world
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(100));
            assert!(!world.invoke(condition.clone(), ()).unwrap());
        }

        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(600));
        assert!(world.invoke(condition.clone(), ()).unwrap());
    }

    #[test]
    fn on_real_timer_fires_on_interval() {
        let mut world = World::alloc();
        world.insert_resource::<Time<Real>>(Time::<Real>::default());

        let condition = on_real_timer(Duration::from_secs(1));

        for _ in 0..5 {
            world
                .resource_mut::<Time<Real>>()
                .update_with_duration(Duration::from_millis(100));
            assert!(!world.invoke(condition.clone(), ()).unwrap());
        }

        world
            .resource_mut::<Time<Real>>()
            .update_with_duration(Duration::from_millis(600));
        assert!(world.invoke(condition.clone(), ()).unwrap());
    }

    #[test]
    fn once_after_delay_fires_once() {
        let mut world = World::alloc();
        world.insert_resource::<Time>(Time::default());

        let condition = once_after_delay(Duration::from_secs(1));
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(1100));
        assert!(world.invoke(condition.clone(), ()).unwrap());

        // It fires exactly once, not indefinitely.
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(100));
        assert!(!world.invoke(condition.clone(), ()).unwrap());
    }

    #[test]
    fn once_after_real_delay_fires_once() {
        let mut world = World::alloc();
        world.insert_resource::<Time<Real>>(Time::<Real>::default());

        // The first update establishes the baseline; the second advances.
        let condition = once_after_real_delay(Duration::from_secs(1));
        world
            .resource_mut::<Time<Real>>()
            .update_with_duration(Duration::from_millis(1100));
        assert!(!world.invoke(condition.clone(), ()).unwrap());
        world
            .resource_mut::<Time<Real>>()
            .update_with_duration(Duration::from_millis(1100));
        assert!(world.invoke(condition.clone(), ()).unwrap());

        world
            .resource_mut::<Time<Real>>()
            .update_with_duration(Duration::from_millis(100));
        assert!(!world.invoke(condition.clone(), ()).unwrap());
    }

    #[test]
    fn repeating_after_delay_stays_active() {
        let mut world = World::alloc();
        world.insert_resource::<Time>(Time::default());

        let condition = repeating_after_delay(Duration::from_secs(1));
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(500));
        assert!(!world.invoke(condition.clone(), ()).unwrap());

        // Once the delay has passed, the condition stays active forever.
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(600));
        assert!(world.invoke(condition.clone(), ()).unwrap());
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(10));
        assert!(world.invoke(condition.clone(), ()).unwrap());
    }

    #[test]
    fn repeating_after_real_delay_stays_active() {
        let mut world = World::alloc();
        world.insert_resource::<Time<Real>>(Time::<Real>::default());

        let condition = repeating_after_real_delay(Duration::from_secs(1));
        world
            .resource_mut::<Time<Real>>()
            .update_with_duration(Duration::from_millis(500));
        assert!(!world.invoke(condition.clone(), ()).unwrap());

        world
            .resource_mut::<Time<Real>>()
            .update_with_duration(Duration::from_millis(600));
        assert!(!world.invoke(condition.clone(), ()).unwrap());

        world
            .resource_mut::<Time<Real>>()
            .update_with_duration(Duration::from_secs(10));
        assert!(world.invoke(condition.clone(), ()).unwrap());
    }

    #[test]
    fn paused_reports_virtual_pause() {
        let mut world = World::alloc();
        world.insert_resource::<Time<Virtual>>(Time::<Virtual>::default());

        assert!(!world.invoke(paused, ()).unwrap());

        world.resource_mut::<Time<Virtual>>().pause();
        assert!(world.invoke(paused, ()).unwrap());
    }
}
