//! The generic [`Time`] clock.
#![expect(clippy::module_inception, reason = "For better structure.")]

use core::time::Duration;

use zlim_reflect::TypePath;

use crate::derive::Resource;

// -----------------------------------------------------------------------------
// TimeContext

/// Marks a type that can be used as a time context.
///
/// Time contexts identify different clocks (e.g., Real, Virtual, Fixed)
/// and may carry additional timing behavior.
pub trait TimeContext: Default + TypePath + Send + Sync + 'static {}

impl TimeContext for () {}

// -----------------------------------------------------------------------------
// Time

/// Generic time resource tracking delta and elapsed time for a context type `T`.
///
/// The default context (`T = ()`) mirrors the current virtual clock and is
/// the one most systems should read via `Res<Time>`.  Use `Res<Time<Real>>`,
/// `Res<Time<Virtual>>`, or `Res<Time<Fixed>>` for context-specific access.
///
/// `Time` is `Copy`, so reading it inside a system costs nothing beyond the
/// resource fetch itself.
///
/// # Examples
///
/// The clock starts at zero and advances in discrete steps:
///
/// ```rust
/// # use std::time::Duration;
/// # use zlim_core::time::Time;
/// #
/// let mut time: Time = Time::default();
/// time.advance_by(Duration::from_millis(16));
/// assert_eq!(time.delta(), Duration::from_millis(16));
/// assert_eq!(time.elapsed(), Duration::from_millis(16));
/// ```
///
/// Read it from a system like any other resource:
///
/// ```rust, no_run
/// # use zlim_core::borrow::Res;
/// # use zlim_core::time::Time;
/// #
/// fn system(time: Res<Time>) {
///     // Seconds since the clock started.
///     let _ = time.elapsed_secs();
///     // Seconds since the previous frame.
///     let _ = time.delta_secs();
/// }
/// ```
#[derive(TypePath, Resource, Debug, Copy, Clone, PartialEq)]
#[type_path = "zlim_core::time::Time"]
pub struct Time<T: TimeContext = ()> {
    // additional data
    context: T,
    // the time elapsed since the previous tick
    delta: Duration,
    delta_secs: f32,
    delta_secs_f64: f64,
    // the total time elapsed since this clock started.
    elapsed: Duration,
    elapsed_secs: f32,
    elapsed_secs_f64: f64,

    wrap_period: Duration,
    elapsed_wrapped: Duration,
    elapsed_secs_wrapped: f32,
    elapsed_secs_wrapped_f64: f64,
}

impl<T: TimeContext> Default for Time<T> {
    fn default() -> Self {
        Self {
            context: Default::default(),
            wrap_period: Self::DEFAULT_WRAP_PERIOD,
            delta: Duration::ZERO,
            delta_secs: 0.0,
            delta_secs_f64: 0.0,
            elapsed: Duration::ZERO,
            elapsed_secs: 0.0,
            elapsed_secs_f64: 0.0,
            elapsed_wrapped: Duration::ZERO,
            elapsed_secs_wrapped: 0.0,
            elapsed_secs_wrapped_f64: 0.0,
        }
    }
}

// -----------------------------------------------------------------------------
// Methods

impl<T: TimeContext> Time<T> {
    const DEFAULT_WRAP_PERIOD: Duration = Duration::from_secs(3600);

    /// Creates a new `Time` with the given `context`,
    /// using default values for all other fields.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::time::Time;
    ///
    /// let time = Time::new_with(());
    /// assert_eq!(time.context(), &());
    /// assert_eq!(time.elapsed(), core::time::Duration::ZERO);
    /// ```
    pub fn new_with(context: T) -> Self {
        Self {
            context,
            ..Default::default()
        }
    }

    /// Advances time by `delta`, updating all cached float
    /// and wrapped values.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use zlim_core::time::Time;
    ///
    /// let mut time: Time = Time::default();
    /// time.advance_by(Duration::from_secs(2));
    /// assert_eq!(time.delta(), Duration::from_secs(2));
    /// assert_eq!(time.elapsed(), Duration::from_secs(2));
    /// assert_eq!(time.elapsed_secs(), 2.0);
    /// ```
    pub fn advance_by(&mut self, delta: Duration) {
        #[inline]
        fn duration_rem(dividend: Duration, divisor: Duration) -> Duration {
            // Keep arithmetic in u128 to avoid overflow when elapsed is large or
            // wrap_period is small.  The remainder is always < divisor, so fits in
            // u64 for any reasonable wrap period.
            Duration::from_nanos((dividend.as_nanos() % divisor.as_nanos()) as u64)
        }

        self.delta = delta;
        self.delta_secs = delta.as_secs_f32();
        self.delta_secs_f64 = delta.as_secs_f64();
        self.elapsed += delta;
        self.elapsed_secs = self.elapsed.as_secs_f32();
        self.elapsed_secs_f64 = self.elapsed.as_secs_f64();
        self.elapsed_wrapped = duration_rem(self.elapsed, self.wrap_period);
        self.elapsed_secs_wrapped = self.elapsed_wrapped.as_secs_f32();
        self.elapsed_secs_wrapped_f64 = self.elapsed_wrapped.as_secs_f64();
    }

    /// Advances time to the specified absolute `elapsed` total.
    ///
    /// # Panic
    ///  Panics if `elapsed` is in the past.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use zlim_core::time::Time;
    ///
    /// let mut time: Time = Time::default();
    /// time.advance_to(Duration::from_secs(10));
    /// assert_eq!(time.elapsed(), Duration::from_secs(10));
    /// assert_eq!(time.delta(), Duration::from_secs(10));
    /// ```
    pub fn advance_to(&mut self, elapsed: Duration) {
        assert!(
            elapsed >= self.elapsed,
            "tried to move time backwards to an earlier elapsed moment"
        );
        self.advance_by(elapsed - self.elapsed);
    }

    /// Returns the period at which [`elapsed_wrapped`] wraps around.
    ///
    /// [`elapsed_wrapped`]: Self::elapsed_wrapped
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use zlim_core::time::Time;
    ///
    /// let time: Time = Time::default();
    /// assert_eq!(time.wrap_period(), Duration::from_secs(3600));
    /// ```
    #[inline]
    pub fn wrap_period(&self) -> Duration {
        self.wrap_period
    }

    /// Sets the wrap period; panics if `wrap_period` is zero.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use zlim_core::time::Time;
    ///
    /// let mut time: Time = Time::default();
    /// time.set_wrap_period(Duration::from_secs(60));
    /// time.advance_by(Duration::from_secs(90));
    /// assert_eq!(time.elapsed_wrapped(), Duration::from_secs(30));
    /// ```
    #[inline]
    pub fn set_wrap_period(&mut self, wrap_period: Duration) {
        assert!(!wrap_period.is_zero(), "division by zero");
        self.wrap_period = wrap_period;
    }

    /// Returns the time elapsed since the previous tick.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use zlim_core::time::Time;
    ///
    /// let mut time: Time = Time::default();
    /// time.advance_by(Duration::from_millis(16));
    /// assert_eq!(time.delta(), Duration::from_millis(16));
    /// ```
    #[inline]
    pub fn delta(&self) -> Duration {
        self.delta
    }

    /// Returns [`delta`](Self::delta) as `f32` seconds.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use zlim_core::time::Time;
    ///
    /// let mut time: Time = Time::default();
    /// time.advance_by(Duration::from_millis(500));
    /// assert_eq!(time.delta_secs(), 0.5);
    /// ```
    #[inline]
    pub fn delta_secs(&self) -> f32 {
        self.delta_secs
    }

    /// Returns [`delta`](Self::delta) as `f64` seconds.
    #[inline]
    pub fn delta_secs_f64(&self) -> f64 {
        self.delta_secs_f64
    }

    /// Returns the total time elapsed since this clock started.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use zlim_core::time::Time;
    ///
    /// let mut time: Time = Time::default();
    /// time.advance_by(Duration::from_secs(1));
    /// time.advance_by(Duration::from_secs(2));
    /// assert_eq!(time.elapsed(), Duration::from_secs(3));
    /// ```
    #[inline]
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Returns [`elapsed`](Self::elapsed) as `f32` seconds.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use zlim_core::time::Time;
    ///
    /// let mut time: Time = Time::default();
    /// time.advance_by(Duration::from_secs(2));
    /// assert_eq!(time.elapsed_secs(), 2.0);
    /// ```
    #[inline]
    pub fn elapsed_secs(&self) -> f32 {
        self.elapsed_secs
    }

    /// Returns [`elapsed`](Self::elapsed) as `f64` seconds.
    #[inline]
    pub fn elapsed_secs_f64(&self) -> f64 {
        self.elapsed_secs_f64
    }

    /// Returns [`elapsed`](Self::elapsed) modulo the
    /// [`wrap_period`](Self::wrap_period).
    ///
    /// This keeps long-running clocks bounded: after the wrap period the
    /// value restarts from zero instead of growing unboundedly.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use zlim_core::time::Time;
    ///
    /// let mut time: Time = Time::default();
    /// time.set_wrap_period(Duration::from_secs(10));
    /// time.advance_by(Duration::from_secs(25));
    /// assert_eq!(time.elapsed_wrapped(), Duration::from_secs(5));
    /// ```
    #[inline]
    pub fn elapsed_wrapped(&self) -> Duration {
        self.elapsed_wrapped
    }

    /// Returns [`elapsed_wrapped`](Self::elapsed_wrapped) as `f32` seconds.
    #[inline]
    pub fn elapsed_secs_wrapped(&self) -> f32 {
        self.elapsed_secs_wrapped
    }

    /// Returns [`elapsed_wrapped`](Self::elapsed_wrapped) as `f64` seconds.
    #[inline]
    pub fn elapsed_secs_wrapped_f64(&self) -> f64 {
        self.elapsed_secs_wrapped_f64
    }

    /// Returns a reference to the time context.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::time::Time;
    ///
    /// let time = Time::new_with(());
    /// assert_eq!(time.context(), &());
    /// ```
    #[inline]
    pub fn context(&self) -> &T {
        &self.context
    }

    /// Returns a mutable reference to the time context.
    #[inline]
    pub fn context_mut(&mut self) -> &mut T {
        &mut self.context
    }

    /// Returns a type-erased `Time<()>` copy of this clock's tick data,
    /// dropping the context.
    ///
    /// Useful to publish a context-specific clock (e.g. the fixed clock)
    /// through the generic `Time` resource.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use zlim_core::time::{Time, Virtual};
    ///
    /// let mut virt = Time::<Virtual>::default();
    /// virt.advance_by(Duration::from_millis(16));
    ///
    /// let current: Time = virt.as_generic();
    /// assert_eq!(current.elapsed(), Duration::from_millis(16));
    /// ```
    #[inline]
    pub fn as_generic(&self) -> Time<()> {
        Time {
            context: (),
            wrap_period: self.wrap_period,
            delta: self.delta,
            delta_secs: self.delta_secs,
            delta_secs_f64: self.delta_secs_f64,
            elapsed: self.elapsed,
            elapsed_secs: self.elapsed_secs,
            elapsed_secs_f64: self.elapsed_secs_f64,
            elapsed_wrapped: self.elapsed_wrapped,
            elapsed_secs_wrapped: self.elapsed_secs_wrapped,
            elapsed_secs_wrapped_f64: self.elapsed_secs_wrapped_f64,
        }
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_initial_state() {
        let time: Time = Time::default();

        assert_eq!(time.wrap_period(), Time::<()>::DEFAULT_WRAP_PERIOD);
        assert_eq!(time.delta(), Duration::ZERO);
        assert_eq!(time.delta_secs(), 0.0);
        assert_eq!(time.delta_secs_f64(), 0.0);
        assert_eq!(time.elapsed(), Duration::ZERO);
        assert_eq!(time.elapsed_secs(), 0.0);
        assert_eq!(time.elapsed_secs_f64(), 0.0);
        assert_eq!(time.elapsed_wrapped(), Duration::ZERO);
        assert_eq!(time.elapsed_secs_wrapped(), 0.0);
        assert_eq!(time.elapsed_secs_wrapped_f64(), 0.0);
    }

    #[test]
    fn test_advance_by() {
        let mut time: Time = Time::default();

        time.advance_by(Duration::from_millis(250));

        assert_eq!(time.delta(), Duration::from_millis(250));
        assert_eq!(time.delta_secs(), 0.25);
        assert_eq!(time.delta_secs_f64(), 0.25);
        assert_eq!(time.elapsed(), Duration::from_millis(250));
        assert_eq!(time.elapsed_secs(), 0.25);
        assert_eq!(time.elapsed_secs_f64(), 0.25);

        time.advance_by(Duration::from_millis(500));

        assert_eq!(time.delta(), Duration::from_millis(500));
        assert_eq!(time.delta_secs(), 0.5);
        assert_eq!(time.delta_secs_f64(), 0.5);
        assert_eq!(time.elapsed(), Duration::from_millis(750));
        assert_eq!(time.elapsed_secs(), 0.75);
        assert_eq!(time.elapsed_secs_f64(), 0.75);

        time.advance_by(Duration::ZERO);

        assert_eq!(time.delta(), Duration::ZERO);
        assert_eq!(time.delta_secs(), 0.0);
        assert_eq!(time.delta_secs_f64(), 0.0);
        assert_eq!(time.elapsed(), Duration::from_millis(750));
        assert_eq!(time.elapsed_secs(), 0.75);
        assert_eq!(time.elapsed_secs_f64(), 0.75);
    }

    #[test]
    fn test_advance_to() {
        let mut time: Time = Time::default();

        time.advance_to(Duration::from_millis(250));

        assert_eq!(time.delta(), Duration::from_millis(250));
        assert_eq!(time.delta_secs(), 0.25);
        assert_eq!(time.delta_secs_f64(), 0.25);
        assert_eq!(time.elapsed(), Duration::from_millis(250));
        assert_eq!(time.elapsed_secs(), 0.25);
        assert_eq!(time.elapsed_secs_f64(), 0.25);

        time.advance_to(Duration::from_millis(750));

        assert_eq!(time.delta(), Duration::from_millis(500));
        assert_eq!(time.delta_secs(), 0.5);
        assert_eq!(time.delta_secs_f64(), 0.5);
        assert_eq!(time.elapsed(), Duration::from_millis(750));
        assert_eq!(time.elapsed_secs(), 0.75);
        assert_eq!(time.elapsed_secs_f64(), 0.75);

        time.advance_to(Duration::from_millis(750));

        assert_eq!(time.delta(), Duration::ZERO);
        assert_eq!(time.delta_secs(), 0.0);
        assert_eq!(time.delta_secs_f64(), 0.0);
        assert_eq!(time.elapsed(), Duration::from_millis(750));
        assert_eq!(time.elapsed_secs(), 0.75);
        assert_eq!(time.elapsed_secs_f64(), 0.75);
    }

    #[test]
    #[should_panic]
    fn test_advance_to_backwards_panics() {
        let mut time: Time = Time::default();

        time.advance_to(Duration::from_millis(750));

        time.advance_to(Duration::from_millis(250));
    }

    #[test]
    fn test_wrapping() {
        let mut time: Time = Time::default();
        time.set_wrap_period(Duration::from_secs(3));

        time.advance_by(Duration::from_secs(2));

        assert_eq!(time.elapsed_wrapped(), Duration::from_secs(2));
        assert_eq!(time.elapsed_secs_wrapped(), 2.0);
        assert_eq!(time.elapsed_secs_wrapped_f64(), 2.0);

        time.advance_by(Duration::from_secs(2));

        assert_eq!(time.elapsed_wrapped(), Duration::from_secs(1));
        assert_eq!(time.elapsed_secs_wrapped(), 1.0);
        assert_eq!(time.elapsed_secs_wrapped_f64(), 1.0);

        time.advance_by(Duration::from_secs(2));

        assert_eq!(time.elapsed_wrapped(), Duration::ZERO);
        assert_eq!(time.elapsed_secs_wrapped(), 0.0);
        assert_eq!(time.elapsed_secs_wrapped_f64(), 0.0);

        time.advance_by(Duration::new(3, 250_000_000));

        assert_eq!(time.elapsed_wrapped(), Duration::from_millis(250));
        assert_eq!(time.elapsed_secs_wrapped(), 0.25);
        assert_eq!(time.elapsed_secs_wrapped_f64(), 0.25);
    }

    #[test]
    fn test_wrapping_change() {
        let mut time: Time = Time::default();
        time.set_wrap_period(Duration::from_secs(5));

        time.advance_by(Duration::from_secs(8));

        assert_eq!(time.elapsed_wrapped(), Duration::from_secs(3));
        assert_eq!(time.elapsed_secs_wrapped(), 3.0);
        assert_eq!(time.elapsed_secs_wrapped_f64(), 3.0);

        time.set_wrap_period(Duration::from_secs(2));

        assert_eq!(time.elapsed_wrapped(), Duration::from_secs(3));
        assert_eq!(time.elapsed_secs_wrapped(), 3.0);
        assert_eq!(time.elapsed_secs_wrapped_f64(), 3.0);

        time.advance_by(Duration::ZERO);

        // Time will wrap to modulo duration from full `elapsed()`, not to what
        // is left in `elapsed_wrapped()`. This test of values is here to ensure
        // that we notice if we change that behavior.
        assert_eq!(time.elapsed_wrapped(), Duration::from_secs(0));
        assert_eq!(time.elapsed_secs_wrapped(), 0.0);
        assert_eq!(time.elapsed_secs_wrapped_f64(), 0.0);
    }
}
