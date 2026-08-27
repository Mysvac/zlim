//! Real (wall-clock) time.

use core::time::Duration;

use zlim_os::time::Instant;
use zlim_reflect::derive::TypePath;

use super::{Time, TimeContext};

// -----------------------------------------------------------------------------
// Real

/// Context for wall-clock time, tracking startup and update instants.
///
/// `Real` is the context type of [`Time<Real>`]: the resource that tracks
/// how much wall-clock time has passed since the program started.  It is
/// advanced by the engine driver ([`World::refresh_metadata`]) each frame;
/// in tests or headless contexts it can be advanced by hand.
///
/// The first update only set the baseline time (without setting delta and
/// elapsed), regardless of the input.
///
/// # Examples
///
/// ```rust
/// use std::time::Duration;
/// use zlim_core::time::{Real, Time};
///
/// let mut real = Time::<Real>::default();
///
/// // The first update establishes the baseline and produces no delta.
/// real.update_with_duration(Duration::from_millis(16));
/// assert_eq!(real.delta(), Duration::ZERO);
///
/// // Subsequent updates measure the elapsed duration.
/// real.update_with_duration(Duration::from_millis(16));
/// assert_eq!(real.delta(), Duration::from_millis(16));
/// assert_eq!(real.elapsed(), Duration::from_millis(16));
/// ```
///
/// [`Time<Real>`]: Time
/// [`World::refresh_metadata`]: crate::world::World::refresh_metadata
#[derive(TypePath, Debug, Copy, Clone, PartialEq)]
pub struct Real {
    startup: Instant,
    first_update: Option<Instant>,
    last_update: Option<Instant>,
}

// -----------------------------------------------------------------------------
// Traits

impl TimeContext for Real {}

impl Default for Real {
    fn default() -> Self {
        Self {
            startup: Instant::now(),
            first_update: None,
            last_update: None,
        }
    }
}

// -----------------------------------------------------------------------------
// Methods

impl Time<Real> {
    /// Creates a new `Time<Real>` with the given `startup` instant.
    pub fn new(startup: Instant) -> Self {
        let context = Real {
            startup,
            ..Default::default()
        };
        Self::new_with(context)
    }

    /// Advances real time using `Instant::now()`.
    ///
    /// This is what the per-frame driver ([`World::refresh_metadata`](crate::world::World::refresh_metadata))
    /// calls every frame; the delta is the wall time elapsed since the
    /// previous update.  Schedule systems should not call this directly.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::time::{Real, Time};
    ///
    /// let mut real = Time::<Real>::default();
    /// real.update(); // advances to `Instant::now()`
    /// assert!(real.last_update().is_some());
    /// ```
    pub fn update(&mut self) {
        self.update_with_instant(Instant::now());
    }

    /// Advances real time by a fixed `duration` from the last update instant.
    ///
    /// Used by
    /// [`TimeUpdateStrategy::ManualDuration`]-driven ([`World::refresh_metadata`]) loops and deterministic
    /// tests.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use zlim_core::time::{Real, Time};
    ///
    /// let mut real = Time::<Real>::default();
    /// real.update_with_duration(Duration::from_millis(16)); // baseline
    /// real.update_with_duration(Duration::from_millis(16));
    /// assert_eq!(real.delta(), Duration::from_millis(16));
    /// ```
    ///
    /// [`World::refresh_metadata`]: crate::world::World::refresh_metadata
    /// [`TimeUpdateStrategy::ManualDuration`]: crate::time::TimeUpdateStrategy::ManualDuration
    pub fn update_with_duration(&mut self, duration: Duration) {
        let last_update = self.context().last_update.unwrap_or(self.context().startup);
        self.update_with_instant(last_update + duration);
    }

    /// Advances real time to the given `instant`, computing delta since the
    /// last update.
    pub fn update_with_instant(&mut self, instant: Instant) {
        let Some(last_update) = self.context().last_update else {
            let context = self.context_mut();
            context.first_update = Some(instant);
            context.last_update = Some(instant);
            return;
        };

        let delta = instant.saturating_duration_since(last_update);
        self.advance_by(delta);
        self.context_mut().last_update = Some(instant);
    }

    /// Returns the `Instant` at which this clock was created.
    #[inline]
    pub fn startup(&self) -> Instant {
        self.context().startup
    }

    /// Returns the `Instant` of the first call to any update method, or
    /// `None` if not yet updated.
    #[inline]
    pub fn first_update(&self) -> Option<Instant> {
        self.context().first_update
    }

    /// Returns the `Instant` of the most recent update, or `None` if not yet
    /// updated.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use zlim_core::time::{Real, Time};
    ///
    /// let mut real = Time::<Real>::default();
    /// assert!(real.last_update().is_none());
    ///
    /// real.update_with_duration(Duration::from_secs(1));
    /// assert!(real.last_update().is_some());
    /// assert_eq!(
    ///     real.last_update().unwrap() - real.startup(),
    ///     Duration::from_secs(1),
    /// );
    /// ```
    #[inline]
    pub fn last_update(&self) -> Option<Instant> {
        self.context().last_update
    }
}

// -----------------------------------------------------------------------------
