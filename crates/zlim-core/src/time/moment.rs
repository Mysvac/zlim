use core::fmt::{Debug, Formatter};
use core::hash::Hash;
use core::marker::PhantomData;
use core::ops::{Add, AddAssign, Sub, SubAssign};
use core::time::Duration;

use crate::time::Time;

use super::TimeContext;

/// A measurement of a point in time from some [`Time<C>`](crate::time::Time).
///
/// Zlim's equivalent to Rust's [`Instant`](std::time::Instant) but captured from
/// a specific [`Time<C>`](crate::time::Time) resource instead of the platform's monotonic clock.
///
/// `Moments` are captured from clocks, see [`Time::capture`].
pub struct Moment<C = ()> {
    // How much time has elapsed since the source clock has been added to the world.
    elapsed: Duration,
    _marker: PhantomData<C>,
}

impl<C: TimeContext> Time<C> {
    #[inline]
    pub fn capture(&self) -> Moment<C> {
        Moment {
            elapsed: self.elapsed(),
            _marker: PhantomData,
        }
    }
}

impl<C> Moment<C> {
    /// Casts a moment from one clock source to another.
    ///
    /// Clocks may run at different rates meaning they may produce numerically
    /// different moments during the same world tick. This method is provided
    /// under the assumption you have a way to deal with this.
    pub fn cast<T>(self) -> Moment<T> {
        Moment {
            elapsed: self.elapsed,
            _marker: PhantomData,
        }
    }

    /// Returns a type-erased `Moment<()>` copy.
    pub fn as_generic(self) -> Moment<()> {
        Moment {
            elapsed: self.elapsed,
            _marker: PhantomData,
        }
    }

    /// Returns how much time has elapsed between this and a later moment.
    ///
    /// If `later` happened before this moment returns a zero [`Duration`].
    /// This method is equivalent to `later - self`.
    pub fn elapsed_between(self, later: Moment<C>) -> Duration {
        later - self
    }

    /// Offsets self by `offset`. Returns `None` if this would cause an overflow.
    pub fn checked_add(self, offset: Duration) -> Option<Self> {
        let elapsed = self.elapsed.checked_add(offset)?;
        Some(Moment {
            elapsed,
            _marker: PhantomData,
        })
    }

    /// Offsets self backwards by `offset`. Returns `None` if this would cause an overflow.
    pub fn checked_sub(self, offset: Duration) -> Option<Self> {
        let elapsed = self.elapsed.checked_sub(offset)?;
        Some(Moment {
            elapsed,
            _marker: PhantomData,
        })
    }

    /// Returns the amount of time since the other moment or `None`
    /// if that moment came after this one.
    pub fn checked_duration_since(self, earlier: Moment<C>) -> Option<Duration> {
        (self >= earlier).then(|| self.elapsed - earlier.elapsed)
    }

    /// Returns the amount of time since the other moment or zero duration
    /// if that moment came after this one.
    pub fn duration_since(self, earlier: Moment<C>) -> Duration {
        self.checked_duration_since(earlier).unwrap_or_default()
    }

    /// Returns how much time has passed between the first tick of the source clock and this moment.
    pub fn into_duration(self) -> Duration {
        self.elapsed
    }
}

impl<C: TimeContext> Moment<C> {
    /// Returns how much time has elapsed since we captured this moment.
    pub fn elapsed(self, time: &Time<C>) -> Duration {
        time.capture() - self
    }
}

impl<C> From<Duration> for Moment<C> {
    fn from(value: Duration) -> Self {
        Moment {
            elapsed: value,
            _marker: PhantomData,
        }
    }
}

impl<C> From<Moment<C>> for Duration {
    fn from(value: Moment<C>) -> Self {
        value.elapsed
    }
}

impl<C> Clone for Moment<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C> Copy for Moment<C> {}

impl<C> Debug for Moment<C> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Moment")
            .field("elapsed", &self.elapsed)
            .finish()
    }
}

impl<C> PartialEq for Moment<C> {
    fn eq(&self, other: &Self) -> bool {
        self.elapsed == other.elapsed
    }
}

impl<C> Eq for Moment<C> {}

impl<C> PartialOrd for Moment<C> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<C> Ord for Moment<C> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.elapsed.cmp(&other.elapsed)
    }
}

impl<C> Hash for Moment<C> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.elapsed.hash(state);
    }
}

impl<C> Add<Duration> for Moment<C> {
    type Output = Self;

    fn add(mut self, rhs: Duration) -> Self::Output {
        self.elapsed += rhs;
        self
    }
}

impl<C> AddAssign<Duration> for Moment<C> {
    fn add_assign(&mut self, rhs: Duration) {
        self.elapsed += rhs;
    }
}

impl<C> Sub for Moment<C> {
    type Output = Duration;

    /// Returns the amount of time elapsed since the other moment
    /// or zero duration if that moment came after this one.
    fn sub(self, other: Self) -> Self::Output {
        self.duration_since(other)
    }
}

impl<C> Sub<Duration> for Moment<C> {
    type Output = Self;

    fn sub(mut self, rhs: Duration) -> Self::Output {
        self.elapsed -= rhs;
        self
    }
}

impl<C> SubAssign<Duration> for Moment<C> {
    fn sub_assign(&mut self, rhs: Duration) {
        self.elapsed -= rhs;
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use crate::time::Time;

    #[test]
    fn moments_are_ordered() {
        const TIME_OFFSET: Duration = Duration::from_secs(1);

        let mut time: Time<()> = Time::default();

        let moment_0 = time.capture();

        time.advance_by(TIME_OFFSET);

        let moment_1_1 = time.capture();
        let moment_1_2 = time.capture();

        time.advance_by(TIME_OFFSET);

        let moment_2 = time.capture();

        assert!(moment_0 < moment_1_1);
        // Capturing the same moment again should be idempotent
        assert!(moment_1_1 == moment_1_2);
        // Might as well check the cmp implementation invariants
        assert!(moment_1_1 <= moment_1_2);
        assert!(moment_1_1 >= moment_1_2);

        assert!(moment_1_1 < moment_2);
        assert!(moment_1_2 < moment_2);
        assert!(moment_0 < moment_2);
    }

    #[test]
    fn moments_can_be_offset() {
        const TIME_OFFSET: Duration = Duration::from_secs(1);

        let mut time: Time<()> = Time::default();
        time.advance_to(TIME_OFFSET);

        let moment_0 = time.capture();

        assert_eq!(moment_0.checked_add(Duration::MAX), None);
        assert_eq!(
            moment_0.checked_add(TIME_OFFSET),
            Some(moment_0 + TIME_OFFSET)
        );
        assert_eq!(
            moment_0.checked_sub(TIME_OFFSET),
            Some(moment_0 - TIME_OFFSET)
        );

        assert_eq!(moment_0.checked_sub(TIME_OFFSET * 2), None);

        let moment_1 = moment_0 + TIME_OFFSET;
        let moment_neg_1 = moment_0 - TIME_OFFSET;

        assert!(moment_1 > moment_0);
        assert!(moment_0 > moment_neg_1);
        assert!(moment_1 > moment_neg_1);

        assert_eq!(moment_1 - TIME_OFFSET, moment_0);

        assert_eq!(moment_0 + Duration::ZERO, moment_0);
        assert_eq!(moment_0 - Duration::ZERO, moment_0);
        assert_eq!(moment_0.checked_add(Duration::ZERO), Some(moment_0));
        assert_eq!(moment_0.checked_sub(Duration::ZERO), Some(moment_0));
    }

    #[test]
    fn moments_offset_by_moments() {
        const TIME_OFFSET: Duration = Duration::from_secs(1);

        let mut time: Time<()> = Time::default();
        time.advance_by(TIME_OFFSET);

        let moment_0 = time.capture();

        time.advance_by(TIME_OFFSET);

        let moment_1 = time.capture();

        assert_eq!(
            moment_1 - moment_0 + moment_0.into_duration(),
            moment_1.into_duration()
        );
    }

    /// Subtracting a moment yields the time between the two moments, and a moment subtracted from an
    /// earlier one is zero rather than a negative duration.
    #[test]
    fn moments_are_subtracted() {
        const TIME_OFFSET: Duration = Duration::from_secs(1);

        let mut time: Time<()> = Time::default();
        time.advance_by(TIME_OFFSET);

        let moment_0 = time.capture();

        time.advance_to(TIME_OFFSET * 3);

        let moment_2 = time.capture();

        assert_eq!(moment_2 - moment_0, TIME_OFFSET * 2);
        assert_eq!(moment_2.duration_since(moment_0), TIME_OFFSET * 2);
        assert_eq!(
            moment_2.checked_duration_since(moment_0),
            Some(TIME_OFFSET * 2)
        );

        // The other way around saturates at zero instead of underflowing.
        assert_eq!(moment_0 - moment_2, Duration::ZERO);
        assert_eq!(moment_0.duration_since(moment_2), Duration::ZERO);
        assert_eq!(moment_0.checked_duration_since(moment_2), None);
    }

    #[test]
    fn elapsed() {
        const TIME_OFFSET: Duration = Duration::from_secs(1);

        let mut time: Time<()> = Time::default();
        time.advance_by(TIME_OFFSET);

        let moment_0 = time.capture();

        time.advance_by(TIME_OFFSET);

        let moment_1 = time.capture();

        let elapsed = moment_1 - moment_0;

        assert_eq!(moment_0.elapsed(&time), elapsed);
        assert_eq!(moment_0.elapsed_between(moment_1), elapsed);
    }
}
