//! Change Detection Core Implementation
//!
//! Tick is the world timestamp mechanism used primarily for change detection.
//!
//! It is a 32-bit integer representing a discrete point in time. Because it can
//! overflow and wrap around, it is not suitable for timeline synchronization
//! across different instances.
//!
//! In this crate, ticks are used as relative markers, not globally meaningful
//! absolute time values.
//!
//! The world maintains a continuously advancing `Tick` named `now` as the current
//! moment. Since `now` wraps on overflow, we must also cap the maximum observable
//! age between two ticks.
//!
//! Every [`CHECK_CYCLE`] ticks, all component/resource tick markers are validated
//! to ensure their age does not exceed [`MAX_TICK_AGE`]. This can introduce a
//! periodic pause (roughly every 8 hours), but the work is chunked and spread
//! across threads, so the runtime impact is typically small.

// -----------------------------------------------------------------------------
// Configuration

use zlim_ptr::{Slice, SliceMut};

/// Check cycle for component age validation (prevents overflow issues)
pub const CHECK_CYCLE: u32 = 1 << 29;

/// Maximum allowable Tick age - values exceeding this are clamped to this limit
pub const MAX_TICK_AGE: u32 = u32::MAX - (CHECK_CYCLE << 1) - 1;

// -----------------------------------------------------------------------------
// Tick

/// A 32-bit integer representing a discrete time point (or duration).
///
/// Primarily used by change detection to track when components/resources were
/// inserted or modified.
///
/// Not suitable for timeline synchronization between independent clients,
/// because tick progression rates are not guaranteed to match.
///
/// As a 32-bit value, it wraps periodically, so age checks and clamping are
/// built into the surrounding systems.
///
/// *Note* that a system that hasn't been run yet has a `Tick` of 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct Tick(u32);

impl Tick {
    /// Maximum valid tick age, equivalent to [`MAX_TICK_AGE`].
    ///
    /// Any tick older than this limit is clamped during world maintenance.
    pub const MAX_AGE: Self = Self::new(MAX_TICK_AGE);

    /// Creates a new `Tick`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_core::tick::Tick;
    /// let tick = Tick::new(42);
    /// ```
    #[inline(always)]
    pub const fn new(tick: u32) -> Self {
        Self(tick)
    }

    /// Returns the underlying `u32` value.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_core::tick::Tick;
    /// let tick = Tick::new(42);
    /// assert_eq!(tick.get(), 42);
    /// ```
    #[inline(always)]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Sets the tick value.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_core::tick::Tick;
    /// let mut tick = Tick::new(42);
    /// tick.set(100);
    /// assert_eq!(tick.get(), 100);
    /// ```
    #[inline(always)]
    pub const fn set(&mut self, tick: u32) {
        self.0 = tick;
    }

    /// Computes age relative to another tick.
    ///
    /// Uses wrapping subtraction so overflow/wrap-around is handled correctly.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_core::tick::Tick;
    /// let later = Tick::new(200);
    /// let earlier = Tick::new(100);
    /// let age = later.relative_to(earlier);
    /// assert_eq!(age.get(), 100);
    /// ```
    #[inline(always)]
    pub const fn relative_to(self, other: Self) -> Self {
        Self(self.0.wrapping_sub(other.0))
    }

    /// Returns whether this tick is newer than `other`, relative to `now`.
    ///
    /// This is used by change detection: if an update happened after
    /// `last_run` from the perspective of `this_run` (`now`), it is
    /// considered changed.
    ///
    /// Operationally, this compares two clamped ages:
    ///
    /// - age(self) = `now - self`
    /// - age(other) = `now - other`
    ///
    /// `self` is treated as newer when `age(self) < age(other)`. Clamping
    /// with [`MAX_TICK_AGE`] keeps comparisons stable across wrap-around.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_core::tick::Tick;
    /// let tick1 = Tick::new(100);
    /// let tick2 = Tick::new(200);
    /// let this_run = Tick::new(500);
    ///
    /// assert!(tick2.is_newer_than(tick1, this_run));
    /// assert!(!tick1.is_newer_than(tick2, this_run));
    /// ```
    #[inline]
    pub const fn is_newer_than(self, other: Tick, now: Tick) -> bool {
        // `core::cmp::min` cannot be used in `const fn`.
        #[inline(always)]
        const fn clamp(x: u32) -> u32 {
            if x < MAX_TICK_AGE { x } else { MAX_TICK_AGE }
        }

        let since_insert = clamp(now.relative_to(self).0);
        let since_system = clamp(now.relative_to(other).0);

        since_system > since_insert
    }

    /// Clamps a single tick value if it is older than `MAX_TICK_AGE`.
    ///
    /// If the tick is too old, it is moved to the fallback value
    /// `now - Tick::MAX_AGE`.
    #[inline(always)]
    pub const fn clamp(&mut self, now: Tick) {
        let age = now.relative_to(*self);
        let fallback = now.relative_to(Tick::MAX_AGE);
        if age.0 > MAX_TICK_AGE {
            *self = fallback;
        }
    }
}

impl core::hash::Hash for Tick {
    /// Hashes the inner `u32` value.
    #[inline(always)]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write_u32(self.0);
    }
}

// -----------------------------------------------------------------------------
// DetectChanges

/// Change-detection trait for components and resources.
///
/// Types implementing this trait can report when they were inserted and when
/// they were most recently modified.
///
/// This trait is typically consumed through wrappers such as [`Ref`] and
/// [`Res`], where `last_run`/`this_run` context is tracked by the scheduler.
///
/// See [`zlim_core::borrow`](crate::borrow) for more information.
///
/// [`Ref`]: crate::borrow::Ref
/// [`Res`]: crate::borrow::Res
pub trait DetectChanges {
    /// Returns `true` if this value was added after the system last ran.
    fn is_added(&self) -> bool;

    /// Returns `true` if this value was added or mutably dereferenced
    /// either since the last time the system ran or, if the system never ran,
    /// since the beginning of the program.
    fn is_changed(&self) -> bool;

    /// Returns the change tick recording the time this data was added.
    fn added_tick(&self) -> Tick;

    /// Returns the change tick recording the time this data was most recently changed.
    ///
    /// Note that components and resources are also marked as changed upon insertion.
    fn changed_tick(&self) -> Tick;
}

/// Mutable change-detection trait for components and resources.
///
/// Extends [`DetectChanges`] with the ability to bypass change detection
/// and manually set change markers.
///
/// This is typically consumed through mutable wrappers such as [`Mut`],
/// [`ResMut`], and [`UntypedMut`].
///
/// [`Mut`]: crate::borrow::Mut
/// [`ResMut`]: crate::borrow::ResMut
/// [`UntypedMut`]: crate::borrow::UntypedMut
pub trait DetectChangesMut: DetectChanges {
    /// The mutable value type returned when bypassing change detection.
    type Value<'w>
    where
        Self: 'w;

    /// Returns a mutable reference to the inner value without triggering
    /// change detection.
    ///
    /// This allows modifying data without marking it as changed in the
    /// current run.
    fn bypass(&mut self) -> Self::Value<'_>;

    /// Manually marks this value as having been added in the current run.
    fn set_added(&mut self);

    /// Manually marks this value as having been changed in the current run.
    fn set_changed(&mut self);
}

// -----------------------------------------------------------------------------
// TicksRef

/// Immutable references to insertion/change ticks with run context.
///
/// Contains immutable references to the added/changed ticks plus the system
/// run context (`last_run`, `this_run`).
///
/// This is the low-level representation used by read-only change-tracking
/// system parameters.
///
/// See [`Ref`]/[`Res`]/[`UntypedRef`].
///
/// Fields are public for advanced/custom system-parameter use cases.
///
/// [`Ref`]: crate::borrow::Ref
/// [`Res`]: crate::borrow::Res
/// [`UntypedRef`]: crate::borrow::UntypedRef
#[derive(Debug, Clone, Copy)]
pub struct TicksRef<'w> {
    // Perhaps we can directly store the value instead of referencing,
    // then we can reduce 8 Bytes per struct.
    // But the reference is just a pointer, there is no need to access
    // its value, which may be faster during iteration.
    /// Reference to the tick recording when this data was inserted.
    pub added: &'w Tick,
    /// Reference to the tick recording when this data was most recently
    /// modified.
    pub changed: &'w Tick,
    /// The tick when the system (or system parameter) last ran.
    pub last_run: Tick,
    /// The tick of the current system run.
    pub this_run: Tick,
}

// -----------------------------------------------------------------------------
// TicksMut

/// Mutable references to insertion/change ticks with run context.
///
/// Contains mutable references to the added/changed ticks plus the system
/// run context (`last_run`, `this_run`).
///
/// This enables APIs that both read and update change markers.
///
/// See [`Mut`]/[`ResMut`]/[`UntypedMut`].
///
/// Fields are public for advanced/custom system-parameter use cases.
///
/// [`Mut`]: crate::borrow::Mut
/// [`ResMut`]: crate::borrow::ResMut
/// [`UntypedMut`]: crate::borrow::UntypedMut
#[derive(Debug)]
pub struct TicksMut<'w> {
    /// Mutable reference to the tick recording when this data was inserted.
    pub added: &'w mut Tick,
    /// Mutable reference to the tick recording when this data was most recently
    /// modified.
    pub changed: &'w mut Tick,
    /// The tick when the system (or system parameter) last ran.
    pub last_run: Tick,
    /// The tick of the current system run.
    pub this_run: Tick,
}

impl<'w> From<TicksMut<'w>> for TicksRef<'w> {
    /// Converts mutable tick references into an immutable [`TicksRef`].
    ///
    /// The [`last_run`] and [`this_run`] values are copied; the
    /// [`added`] and [`changed`] references are reborrowed immutably.
    ///
    /// [`last_run`]: TicksRef::last_run
    /// [`this_run`]: TicksRef::this_run
    /// [`added`]: TicksRef::added
    /// [`changed`]: TicksRef::changed
    #[inline(always)]
    fn from(this: TicksMut<'w>) -> Self {
        TicksRef {
            added: this.added,
            changed: this.changed,
            last_run: this.last_run,
            this_run: this.this_run,
        }
    }
}

// -----------------------------------------------------------------------------
// TicksSliceRef

/// Immutable slices of insertion/change ticks with run context.
///
/// Contains immutable slices for added/changed ticks plus the system run
/// context (`last_run`, `this_run`).
///
/// `length` stores the logical element count used by high-level slice wrappers.
///
/// See [`SliceRef`]/[`UntypedSliceRef`].
///
/// Fields are public for advanced/custom system-parameter use cases.
///
/// [`SliceRef`]: crate::borrow::SliceRef
/// [`UntypedSliceRef`]: crate::borrow::UntypedSliceRef
#[derive(Debug, Clone, Copy)]
pub struct TicksSliceRef<'w> {
    /// The number of elements in the slices.
    pub length: usize,
    /// Immutable slice of insertion ticks, one per element.
    pub added: Slice<'w, Tick>,
    /// Immutable slice of last-modification ticks, one per element.
    pub changed: Slice<'w, Tick>,
    /// The tick when the system (or system parameter) last ran.
    pub last_run: Tick,
    /// The tick of the current system run.
    pub this_run: Tick,
}

// -----------------------------------------------------------------------------
// TicksSliceMut

/// Mutable slices of insertion/change ticks with run context.
///
/// Contains mutable slices for added/changed ticks plus the system run
/// context (`last_run`, `this_run`).
///
/// This is primarily used by typed/untyped mutable slice accessors.
///
/// See [`SliceMut`]/[`UntypedSliceMut`].
///
/// Fields are public for advanced/custom system-parameter use cases.
///
/// [`SliceMut`]: crate::borrow::SliceMut
/// [`UntypedSliceMut`]: crate::borrow::UntypedSliceMut
#[derive(Debug)]
pub struct TicksSliceMut<'w> {
    /// The number of elements in the slices.
    pub length: usize,
    /// Mutable slice of insertion ticks, one per element.
    pub added: SliceMut<'w, Tick>,
    /// Mutable slice of last-modification ticks, one per element.
    pub changed: SliceMut<'w, Tick>,
    /// The tick when the system (or system parameter) last ran.
    pub last_run: Tick,
    /// The tick of the current system run.
    pub this_run: Tick,
}

impl<'w> From<TicksSliceMut<'w>> for TicksSliceRef<'w> {
    /// Converts mutable tick slices into an immutable [`TicksSliceRef`].
    ///
    /// The [`length`], [`last_run`], and [`this_run`] values are copied;
    /// the [`added`] and [`changed`] slices are converted to immutable.
    ///
    /// [`length`]: TicksSliceRef::length
    /// [`last_run`]: TicksSliceRef::last_run
    /// [`this_run`]: TicksSliceRef::this_run
    /// [`added`]: TicksSliceRef::added
    /// [`changed`]: TicksSliceRef::changed
    #[inline(always)]
    fn from(this: TicksSliceMut<'w>) -> Self {
        TicksSliceRef {
            length: this.length,
            added: this.added.into(),
            changed: this.changed.into(),
            last_run: this.last_run,
            this_run: this.this_run,
        }
    }
}

// -----------------------------------------------------------------------------
