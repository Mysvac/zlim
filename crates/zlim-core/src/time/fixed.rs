//! Fixed-timestep time.

use core::time::Duration;

use zlim_reflect::derive::TypePath;

use super::{Time, TimeContext};

// -----------------------------------------------------------------------------
// Fixed

/// Context for fixed-timestep time, tracking the step duration.
///
/// `Fixed` is the context type of [`Time<Fixed>`]: a clock that advances in
/// whole fixed-size steps.  The accumulation of real time into whole steps is
/// owned by the per-frame driver ([`World::refresh_metadata`]), which settles
/// steps and maintains the `prev` / `curr` interpolation pair of each [`TimeSnapshot`].
///
/// # Examples
///
/// ```rust
/// # use zlim_core::prelude::*;
/// # use std::time::Duration;
/// let fixed = Time::<Fixed>::from_hz(64.0);
/// assert_eq!(fixed.timestep(), Duration::from_micros(15_625));
/// ```
///
/// [`Time<Fixed>`]: Time
/// [`TimeSnapshot`]: crate::time::TimeSnapshot
/// [`World::refresh_metadata`]: crate::world::World::refresh_metadata
#[derive(TypePath, Debug, Copy, Clone, PartialEq)]
pub struct Fixed {
    timestep: Duration,
}

impl TimeContext for Fixed {}

impl Default for Fixed {
    fn default() -> Self {
        Self {
            timestep: Time::<Fixed>::DEFAULT_TIMESTEP,
        }
    }
}

impl Time<Fixed> {
    const DEFAULT_TIMESTEP: Duration = Duration::from_micros(15625);

    /// Creates a `Time<Fixed>` with the given timestep duration.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use zlim_core::time::{Fixed, Time};
    ///
    /// let fixed = Time::<Fixed>::from_duration(Duration::from_millis(16));
    /// assert_eq!(fixed.timestep(), Duration::from_millis(16));
    /// ```
    pub fn from_duration(timestep: Duration) -> Self {
        let mut ret = Self::default();
        ret.set_timestep(timestep);
        ret
    }

    /// Creates a `Time<Fixed>` with a timestep of `seconds` seconds.
    pub fn from_seconds(seconds: f64) -> Self {
        let mut ret = Self::default();
        ret.set_timestep_seconds(seconds);
        ret
    }

    /// Creates a `Time<Fixed>` with a timestep matching the given frequency
    /// in Hz.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::time::{Fixed, Time};
    ///
    /// // 64 Hz is the classic fixed-update rate.
    /// let fixed = Time::<Fixed>::from_hz(64.0);
    /// assert_eq!(fixed.timestep(), core::time::Duration::from_micros(15_625));
    /// ```
    pub fn from_hz(hz: f64) -> Self {
        let mut ret = Self::default();
        ret.set_timestep_hz(hz);
        ret
    }

    /// Returns the fixed timestep duration.
    #[inline]
    pub fn timestep(&self) -> Duration {
        self.context().timestep
    }

    /// Sets the fixed timestep duration; panics if zero.
    #[inline]
    pub fn set_timestep(&mut self, timestep: Duration) {
        assert_ne!(
            timestep,
            Duration::ZERO,
            "attempted to set fixed timestep to zero"
        );
        self.context_mut().timestep = timestep;
    }

    /// Sets the fixed timestep from seconds; panics if not positive or not
    /// finite.
    #[inline]
    pub fn set_timestep_seconds(&mut self, seconds: f64) {
        assert!(seconds > 0.0, "seconds must be positive and non-zero");
        assert!(seconds.is_finite(), "seconds is infinite");
        self.set_timestep(Duration::from_secs_f64(seconds));
    }

    /// Sets the fixed timestep from a frequency in Hz; panics if not positive
    /// or not finite.
    #[inline]
    pub fn set_timestep_hz(&mut self, hz: f64) {
        assert!(hz > 0.0, "Hz must be positive and non-zero");
        assert!(hz.is_finite(), "Hz is infinite");
        self.set_timestep_seconds(1.0 / hz);
    }
}

// -----------------------------------------------------------------------------
