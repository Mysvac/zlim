//! Countdown timers.

use core::time::Duration;

// -----------------------------------------------------------------------------
// TimerMode & Timer

/// Controls whether a [`Timer`] repeats after finishing.
///
/// - [`Once`](TimerMode::Once) — runs once and stays finished.
/// - [`Repeating`](TimerMode::Repeating) — restarts from zero each time it
///   finishes, reporting how many times it wrapped via
///   [`Timer::times_finished_this_tick`].
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
pub enum TimerMode {
    /// The timer runs once and stays in the finished state.
    #[default]
    Once,
    /// The timer resets from zero each time it finishes.
    Repeating,
}

/// A countdown timer that tracks elapsed time against a target duration.
///
/// A `Timer` counts down (via [`tick`](Timer::tick)) towards its
/// [`duration`](Timer::duration).  It can be paused, reset, and — depending
/// on its [`TimerMode`] — either stops when finished or wraps around and
/// keeps repeating.
///
/// # Examples
///
/// ```rust
/// # use std::time::Duration;
/// # use zlim_core::time::{Timer, TimerMode};
/// #
/// // A one-shot timer.
/// let mut timer = Timer::new(Duration::from_secs(1), TimerMode::Once);
/// timer.tick(Duration::from_millis(500));
/// assert!(!timer.is_finished());
///
/// timer.tick(Duration::from_millis(500));
/// assert!(timer.just_finished()); // finished during this tick
/// assert!(timer.is_finished());
///
/// // A repeating timer wraps around and keeps counting.
/// let mut repeating = Timer::from_seconds(0.25, TimerMode::Repeating);
/// repeating.tick(Duration::from_millis(600));
/// assert_eq!(repeating.times_finished_this_tick(), 2);
/// assert_eq!(repeating.elapsed(), Duration::from_millis(100)); // remainder
/// ```
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct Timer {
    // inline `Stopwatch` fields for a
    // compact layout, reduce 8 bytes.
    elapsed: Duration,
    duration: Duration,
    times_finished_this_tick: u32,
    mode: TimerMode,
    finished: bool,
    is_paused: bool,
}

// -----------------------------------------------------------------------------
// Methods

impl Timer {
    /// Creates a new timer for the given `duration` and `mode`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use std::time::Duration;
    /// # use zlim_core::time::{Timer, TimerMode};
    /// #
    /// let timer = Timer::new(Duration::from_secs(2), TimerMode::Once);
    /// assert_eq!(timer.duration(), Duration::from_secs(2));
    /// assert_eq!(timer.mode(), TimerMode::Once);
    /// assert!(!timer.is_finished());
    /// ```
    pub fn new(duration: Duration, mode: TimerMode) -> Self {
        Self {
            duration,
            mode,
            ..Default::default()
        }
    }

    /// Creates a new timer with a duration of `duration` seconds.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use zlim_core::time::{Timer, TimerMode};
    /// #
    /// let timer = Timer::from_seconds(1.5, TimerMode::Repeating);
    /// assert_eq!(timer.mode(), TimerMode::Repeating);
    /// ```
    pub fn from_seconds(duration: f32, mode: TimerMode) -> Self {
        Self {
            duration: Duration::from_secs_f32(duration),
            mode,
            ..Default::default()
        }
    }

    /// Returns `true` if the timer has completed at least once.
    #[inline]
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Returns `true` if the timer finished during the current tick.
    ///
    /// Unlike [`is_finished`](Self::is_finished), this only holds for the
    /// tick in which the timer crossed its duration.
    #[inline]
    pub fn just_finished(&self) -> bool {
        self.times_finished_this_tick > 0
    }

    /// Returns the time elapsed since the timer last reset or started.
    #[inline]
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Returns [`elapsed`](Self::elapsed) as `f32` seconds.
    #[inline]
    pub fn elapsed_secs(&self) -> f32 {
        self.elapsed.as_secs_f32()
    }

    /// Returns [`elapsed`](Self::elapsed) as `f64` seconds.
    #[inline]
    pub fn elapsed_secs_f64(&self) -> f64 {
        self.elapsed.as_secs_f64()
    }

    /// Sets the elapsed time directly, bypassing normal ticking.
    #[inline]
    pub fn set_elapsed(&mut self, time: Duration) {
        self.elapsed = time;
    }

    /// Returns the target duration of this timer.
    #[inline]
    pub fn duration(&self) -> Duration {
        self.duration
    }

    /// Sets the target duration.
    #[inline]
    pub fn set_duration(&mut self, duration: Duration) {
        self.duration = duration;
    }

    /// Returns the time remaining until the timer finishes.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use std::time::Duration;
    /// # use zlim_core::time::{Timer, TimerMode};
    /// #
    /// let mut timer = Timer::new(Duration::from_secs(1), TimerMode::Once);
    /// timer.tick(Duration::from_millis(400));
    ///
    /// assert_eq!(timer.remaining(), Duration::from_millis(600));
    /// assert_eq!(timer.remaining_secs(), 0.6);
    /// ```
    #[inline]
    pub fn remaining(&self) -> Duration {
        self.duration().saturating_sub(self.elapsed())
    }

    /// Returns [`remaining`](Self::remaining) as `f32` seconds.
    #[inline]
    pub fn remaining_secs(&self) -> f32 {
        self.remaining().as_secs_f32()
    }

    /// Advances the timer to exactly finished in the current tick.
    #[inline]
    pub fn finish(&mut self) {
        let remaining = self.remaining();
        self.tick(remaining);
    }

    /// Advances the timer to one nanosecond before finished; no-op if already
    /// finished.
    #[inline]
    pub fn almost_finish(&mut self) {
        let remaining = self.remaining().saturating_sub(Duration::from_nanos(1));
        self.tick(remaining);
    }

    /// Returns the current [`TimerMode`].
    #[inline]
    pub fn mode(&self) -> TimerMode {
        self.mode
    }

    /// Sets the [`TimerMode`], resetting state if switching from `Once` to
    /// `Repeating` while finished.
    #[inline]
    pub fn set_mode(&mut self, mode: TimerMode) {
        if self.mode != TimerMode::Repeating && mode == TimerMode::Repeating && self.finished {
            self.elapsed = Duration::ZERO;
            self.finished = self.just_finished();
        }
        self.mode = mode;
    }

    /// Pauses the timer so it does not advance.
    #[inline]
    pub fn pause(&mut self) {
        self.is_paused = true;
    }

    /// Resumes the timer.
    #[inline]
    pub fn unpause(&mut self) {
        self.is_paused = false;
    }

    /// Returns `true` if the timer is currently paused.
    #[inline]
    pub fn is_paused(&self) -> bool {
        self.is_paused
    }

    /// Resets elapsed time and clears the finished state.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use std::time::Duration;
    /// # use zlim_core::time::{Timer, TimerMode};
    /// #
    /// let mut timer = Timer::new(Duration::from_secs(1), TimerMode::Once);
    /// timer.tick(Duration::from_secs(2));
    /// assert!(timer.is_finished());
    ///
    /// timer.reset();
    /// assert!(!timer.is_finished());
    /// assert_eq!(timer.elapsed(), Duration::ZERO);
    /// ```
    #[inline]
    pub fn reset(&mut self) {
        self.elapsed = Duration::ZERO;
        self.finished = false;
        self.times_finished_this_tick = 0;
    }

    /// Returns the completion progress as a value in `[0.0, 1.0]`.
    #[inline]
    pub fn fraction(&self) -> f32 {
        if self.duration == Duration::ZERO {
            1.0
        } else {
            self.elapsed().as_secs_f32() / self.duration().as_secs_f32()
        }
    }

    /// Returns the remaining fraction as `1.0 - fraction()`.
    #[inline]
    pub fn fraction_remaining(&self) -> f32 {
        1.0 - self.fraction()
    }

    /// Returns how many times the timer finished during the current tick.
    #[inline]
    pub fn times_finished_this_tick(&self) -> u32 {
        self.times_finished_this_tick
    }

    /// Advances the timer by `delta`, updating finished state and repeat
    /// tracking.
    ///
    /// Call this every frame with the time resource's
    /// [`delta`](crate::time::Time::delta).  Returns `&self` for chaining.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use std::time::Duration;
    /// # use zlim_core::time::{Timer, TimerMode};
    /// #
    /// let mut timer = Timer::new(Duration::from_millis(100), TimerMode::Repeating);
    /// timer.tick(Duration::from_millis(250));
    ///
    /// assert_eq!(timer.times_finished_this_tick(), 2);
    /// assert_eq!(timer.elapsed(), Duration::from_millis(50)); // remainder
    /// ```
    pub fn tick(&mut self, delta: Duration) -> &Self {
        self.times_finished_this_tick = 0;

        if self.is_paused() {
            if self.mode == TimerMode::Repeating {
                self.finished = false;
            }
            return self;
        }

        if self.mode != TimerMode::Repeating && self.is_finished() {
            return self;
        }

        self.elapsed = self.elapsed.saturating_add(delta);
        self.finished = self.elapsed() >= self.duration();

        if self.is_finished() {
            if self.mode == TimerMode::Repeating {
                self.times_finished_this_tick = self
                    .elapsed()
                    .as_nanos()
                    .checked_div(self.duration().as_nanos())
                    .map_or(u32::MAX, |x| x as u32);

                let elapsed = self
                    .elapsed()
                    .as_nanos()
                    .checked_rem(self.duration().as_nanos())
                    .map_or(Duration::ZERO, |x| Duration::from_nanos(x as u64));
                self.set_elapsed(elapsed);
            } else {
                self.times_finished_this_tick = 1;
                self.set_elapsed(self.duration());
            }
        }

        self
    }
}

// -----------------------------------------------------------------------------
