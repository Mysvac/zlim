//! A simple stopwatch.

use core::time::Duration;

// -----------------------------------------------------------------------------
// Stopwatch

/// A Stopwatch is a struct that tracks elapsed time when started.
///
/// Note that in order to advance the stopwatch [`tick`] **MUST** be called.
///
/// # Examples
///
/// ```
/// use zlim_core::time::*;
/// use core::time::Duration;
///
/// let mut stopwatch = Stopwatch::new();
/// assert_eq!(stopwatch.elapsed_secs(), 0.0);
///
/// stopwatch.tick(Duration::from_secs_f32(1.0)); // tick one second
/// assert_eq!(stopwatch.elapsed_secs(), 1.0);
///
/// stopwatch.pause();
/// stopwatch.tick(Duration::from_secs_f32(1.0)); // paused stopwatches don't tick
/// assert_eq!(stopwatch.elapsed_secs(), 1.0);
///
/// stopwatch.reset(); // reset the stopwatch
/// assert!(stopwatch.is_paused());
/// assert_eq!(stopwatch.elapsed_secs(), 0.0);
/// ```
///
/// [`tick`]: Stopwatch::tick
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stopwatch {
    elapsed: Duration,
    is_paused: bool,
}

// -----------------------------------------------------------------------------
// Methods

impl Stopwatch {
    /// Creates a new, unpaused stopwatch at zero elapsed time.
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns the total elapsed time.
    #[inline]
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Returns [`elapsed`](Self::elapsed) as `f32` seconds.
    #[inline]
    pub fn elapsed_secs(&self) -> f32 {
        self.elapsed().as_secs_f32()
    }

    /// Returns [`elapsed`](Self::elapsed) as `f64` seconds.
    #[inline]
    pub fn elapsed_secs_f64(&self) -> f64 {
        self.elapsed().as_secs_f64()
    }

    /// Sets the elapsed time directly.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_core::time::Stopwatch;
    /// use core::time::Duration;
    ///
    /// let mut stopwatch = Stopwatch::new();
    /// stopwatch.set_elapsed(Duration::from_secs(5));
    /// assert_eq!(stopwatch.elapsed(), Duration::from_secs(5));
    /// ```
    #[inline]
    pub fn set_elapsed(&mut self, time: Duration) {
        self.elapsed = time;
    }

    /// Pauses the stopwatch; subsequent ticks will not advance elapsed time.
    #[inline]
    pub fn pause(&mut self) {
        self.is_paused = true;
    }

    /// Resumes the stopwatch.
    #[inline]
    pub fn unpause(&mut self) {
        self.is_paused = false;
    }

    /// Returns `true` if the stopwatch is currently paused.
    #[inline]
    pub fn is_paused(&self) -> bool {
        self.is_paused
    }

    /// Resets elapsed time to zero without changing the pause state.
    #[inline]
    pub fn reset(&mut self) {
        self.elapsed = Default::default();
    }

    /// Advances elapsed time by `delta` unless paused; returns `&Self`.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_core::time::Stopwatch;
    /// use core::time::Duration;
    ///
    /// let mut stopwatch = Stopwatch::new();
    /// stopwatch.tick(Duration::from_secs(1));
    /// stopwatch.tick(Duration::from_secs(2));
    /// assert_eq!(stopwatch.elapsed(), Duration::from_secs(3));
    /// ```
    #[inline]
    pub fn tick(&mut self, delta: Duration) -> &Self {
        if !self.is_paused {
            self.elapsed = self.elapsed.saturating_add(delta);
        }
        self
    }
}

// -----------------------------------------------------------------------------
