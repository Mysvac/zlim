//! Virtual game time with pausing and speed scaling.

use core::time::Duration;

use zlim_reflect::derive::TypePath;

use super::{Time, TimeContext};

// -----------------------------------------------------------------------------
// Virtual

/// Context for virtual game time that supports pausing and speed scaling.
///
/// `Virtual` is the context type of [`Time<Virtual>`] — the clock most games
/// actually drive.  Unlike [`Real`], it can be paused, and its speed can be
/// scaled (slow motion, bullet time, …).
///
/// # Examples
///
/// ```rust
/// # use std::time::Duration;
/// # use zlim_core::prelude::*;
/// # use zlim_core::time::{Time, TimeUpdateStrategy, Virtual};
/// #
/// let mut world = World::alloc();
/// let strategy = TimeUpdateStrategy::ManualDuration(Duration::from_millis(100));
/// world.set_time_strategy(strategy);
///
/// World::refresh_metadata(&mut world); // baseline
/// World::refresh_metadata(&mut world); // 100ms of real time
///
/// // Scale to half speed: the next 100ms frame only advances 50ms.
/// world.resource_mut::<Time<Virtual>>().set_relative_speed(0.5);
/// World::refresh_metadata(&mut world);
/// assert_eq!(world.resource::<Time<Virtual>>().elapsed(), Duration::from_millis(150));
///
/// // Pausing freezes the clock entirely.
/// world.resource_mut::<Time<Virtual>>().pause();
/// World::refresh_metadata(&mut world);
/// assert_eq!(world.resource::<Time<Virtual>>().elapsed(), Duration::from_millis(150));
/// ```
///
/// [`Real`]: crate::time::Real
/// [`Time<Virtual>`]: Time
#[derive(TypePath, Debug, Copy, Clone, PartialEq)]
#[type_path = "zlim_core::time::Virtual"]
pub struct Virtual {
    max_delta: Duration,
    paused: bool,
    relative_speed: f64,
    effective_speed: f64,
}

// -----------------------------------------------------------------------------
// Trait

impl TimeContext for Virtual {}

impl Default for Virtual {
    fn default() -> Self {
        Self {
            max_delta: Time::<Virtual>::DEFAULT_MAX_DELTA,
            paused: false,
            relative_speed: 1.0,
            effective_speed: 1.0,
        }
    }
}

// -----------------------------------------------------------------------------
// Time Implementation

impl Time<Virtual> {
    const DEFAULT_MAX_DELTA: Duration = Duration::from_millis(250);

    /// Creates a `Time<Virtual>` with a custom maximum per-tick delta cap.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use std::time::Duration;
    /// # use zlim_core::time::{Time, Virtual};
    /// #
    /// let virt = Time::<Virtual>::from_max_delta(Duration::from_millis(50));
    /// assert_eq!(virt.max_delta(), Duration::from_millis(50));
    /// ```
    pub fn from_max_delta(max_delta: Duration) -> Self {
        let mut ret = Self::default();
        ret.set_max_delta(max_delta);
        ret
    }

    /// Returns the maximum delta that virtual time will advance in a single
    /// tick.
    #[inline]
    pub fn max_delta(&self) -> Duration {
        self.context().max_delta
    }

    /// Sets the maximum per-tick delta cap; panics if `max_delta` is zero.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use std::time::Duration;
    /// # use zlim_core::time::{Time, Virtual};
    /// #
    /// let mut virt = Time::<Virtual>::default();
    /// virt.set_max_delta(Duration::from_millis(100));
    /// assert_eq!(virt.max_delta(), Duration::from_millis(100));
    /// ```
    #[inline]
    pub fn set_max_delta(&mut self, max_delta: Duration) {
        assert_ne!(max_delta, Duration::ZERO, "tried to set max delta to zero");
        self.context_mut().max_delta = max_delta;
    }

    /// Returns the current time scale as `f32`.
    #[inline]
    pub fn relative_speed(&self) -> f32 {
        self.relative_speed_f64() as f32
    }

    /// Returns the current time scale as `f64`.
    #[inline]
    pub fn relative_speed_f64(&self) -> f64 {
        self.context().relative_speed
    }

    /// Returns the speed applied during the last tick as `f32`; `0.0` when
    /// paused.
    ///
    /// Unlike [`relative_speed`](Self::relative_speed), this reflects the
    /// speed that was actually used in the previous tick — `0.0` after a
    /// pause, regardless of the configured scale.
    #[inline]
    pub fn effective_speed(&self) -> f32 {
        self.context().effective_speed as f32
    }

    /// Returns the speed applied during the last tick as `f64`; `0.0` when
    /// paused.
    #[inline]
    pub fn effective_speed_f64(&self) -> f64 {
        self.context().effective_speed
    }

    /// Sets the time scale; panics if the value is negative or non-finite.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::time::{Time, Virtual};
    ///
    /// let mut virt = Time::<Virtual>::default();
    /// virt.set_relative_speed(2.0);
    /// assert_eq!(virt.relative_speed(), 2.0);
    /// ```
    #[inline]
    pub fn set_relative_speed(&mut self, ratio: f32) {
        self.set_relative_speed_f64(ratio as f64);
    }

    /// Sets the time scale as `f64`; panics if the value is negative or
    /// non-finite.
    #[inline]
    pub fn set_relative_speed_f64(&mut self, ratio: f64) {
        assert!(ratio.is_finite(), "tried to go infinitely fast");
        assert!(ratio >= 0.0, "tried to go back in time");
        self.context_mut().relative_speed = ratio;
    }

    /// Toggles the pause state.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::time::{Time, Virtual};
    ///
    /// let mut virt = Time::<Virtual>::default();
    /// virt.toggle();
    /// assert!(virt.is_paused());
    /// virt.toggle();
    /// assert!(!virt.is_paused());
    /// ```
    #[inline]
    pub fn toggle(&mut self) {
        self.context_mut().paused ^= true;
    }

    /// Pauses virtual time; the clock stops advancing until unpaused.
    #[inline]
    pub fn pause(&mut self) {
        self.context_mut().paused = true;
    }

    /// Resumes virtual time after a pause.
    #[inline]
    pub fn unpause(&mut self) {
        self.context_mut().paused = false;
    }

    /// Returns `true` if the clock is currently paused.
    #[inline]
    pub fn is_paused(&self) -> bool {
        self.context().paused
    }

    /// Returns `true` if the clock was paused during the last tick
    /// (effective speed was `0.0`).
    #[inline]
    pub fn was_paused(&self) -> bool {
        self.context().effective_speed == 0.0
    }

    #[inline]
    pub(crate) fn advance_with_raw_delta(&mut self, raw_delta: Duration) {
        let max_delta = self.context().max_delta;

        let clamped_delta = if raw_delta > max_delta {
            zlim_log::debug!(
                "delta time larger than maximum delta, clamping delta to {:?} and skipping {:?}",
                max_delta,
                raw_delta - max_delta
            );
            max_delta
        } else {
            raw_delta
        };

        let effective_speed = if self.context().paused {
            0.0
        } else {
            self.context().relative_speed
        };

        let delta = if effective_speed != 1.0 {
            clamped_delta.mul_f64(effective_speed)
        } else {
            clamped_delta
        };

        self.context_mut().effective_speed = effective_speed;
        self.advance_by(delta);
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use super::*;
    use crate::time::Real;

    #[test]
    fn virtual_time_scales_and_pauses() {
        let mut real = Time::<Real>::default();
        let mut virt = Time::<Virtual>::default();

        // Establish the real clock baseline.
        real.update_with_duration(Duration::from_millis(100));
        virt.advance_with_raw_delta(real.delta());
        assert_eq!(virt.elapsed(), Duration::ZERO);

        real.update_with_duration(Duration::from_millis(100));
        virt.advance_with_raw_delta(real.delta());
        assert_eq!(virt.elapsed(), Duration::from_millis(100));
        // The generic mirror reflects the virtual clock.
        assert_eq!(virt.as_generic().elapsed(), Duration::from_millis(100));
        assert_eq!(virt.effective_speed_f64(), 1.0);

        // Speed scaling.
        virt.set_relative_speed(2.0);
        real.update_with_duration(Duration::from_millis(100));
        virt.advance_with_raw_delta(real.delta());
        assert_eq!(virt.elapsed(), Duration::from_millis(300));

        // Pausing stops virtual time and zeroes the effective speed.
        virt.pause();
        assert!(virt.is_paused());
        real.update_with_duration(Duration::from_millis(100));
        virt.advance_with_raw_delta(real.delta());
        assert_eq!(virt.elapsed(), Duration::from_millis(300));
        assert_eq!(virt.effective_speed(), 0.0);
        assert!(virt.was_paused());

        virt.unpause();
        assert!(!virt.is_paused());
    }

    #[test]
    fn virtual_time_clamps_large_deltas() {
        let mut virt = Time::<Virtual>::from_max_delta(Duration::from_millis(50));
        let mut real = Time::<Real>::default();

        real.update_with_duration(Duration::from_secs(1));
        virt.advance_with_raw_delta(real.delta());

        real.update_with_duration(Duration::from_secs(1));
        virt.advance_with_raw_delta(real.delta());
        assert_eq!(virt.elapsed(), Duration::from_millis(50));
    }
}
