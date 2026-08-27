//! Collection of named [`Schedule`]s.
//!
//! [`Schedules`] stores every schedule owned by a [`World`], keyed by their
//! interned [`ScheduleLabel`].  Users build and mutate schedules in place
//! through [`Schedules::get_mut`] / [`Schedules::entry`] and execute them
//! with [`World::run_schedule`].
//!
//! [`Schedule`]: crate::schedule::Schedule
//! [`ScheduleLabel`]: crate::schedule::ScheduleLabel
//! [`World`]: crate::world::World
//! [`World::run_schedule`]: crate::world::World::run_schedule

use core::fmt::Debug;
use core::ops::{Deref, DerefMut};

use zlim_utils::hash::HashMap;

use super::{InternedScheduleLabel, Schedule, ScheduleLabel};

// -----------------------------------------------------------------------------
// Schedules

/// A collection of [`Schedule`]s, stored by their [`ScheduleLabel`].
///
/// Every [`World`] owns one `Schedules` collection (see
/// [`World::schedules_mut`]).  Labels are interned, so lookups compare and
/// hash pointer identities instead of full label values.
///
/// `Schedules` dereferences to the underlying
/// [`HashMap<InternedScheduleLabel, Schedule>`], exposing all standard map
/// operations in addition to the label-aware methods below.
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// enum Step {
///     Update,
///     Render,
/// }
///
/// #[job_fn(type = RenderFrame, name = "render_frame")]
/// fn render_frame() {}
///
/// let mut schedules = Schedules::new();
/// schedules.insert(Schedule::new(Step::Update));
/// schedules.entry(Step::Render).insert::<RenderFrame>(());
///
/// // Move the standalone schedules into a world and run them by label.
/// let mut world = World::alloc();
/// for stage in [Step::Update, Step::Render] {
///     if let Some(schedule) = schedules.remove(stage) {
///         world.schedules_mut().insert(schedule);
///     }
/// }
/// world.run_schedule(Step::Render);
/// assert!(world.schedules().contains(Step::Render));
/// ```
///
/// [`World`]: crate::world::World
/// [`World::schedules_mut`]: crate::world::World::schedules_mut
/// [`Schedule`]: crate::schedule::Schedule
/// [`ScheduleLabel`]: crate::schedule::ScheduleLabel
/// [`HashMap<InternedScheduleLabel, Schedule>`]: HashMap
pub struct Schedules {
    pub(crate) inner: HashMap<InternedScheduleLabel, Schedule>,
}

// -----------------------------------------------------------------------------
// Construction & Accessors

impl Schedules {
    /// Creates a new, empty schedules collection.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let schedules = Schedules::new();
    /// assert!(schedules.is_empty());
    /// ```
    pub const fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
}

impl Default for Schedules {
    fn default() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
}

impl Schedules {
    /// Inserts `schedule`, replacing any existing schedule with the same
    /// label, and returns a mutable reference to the stored schedule.
    pub fn add_schedule(&mut self, schedule: Schedule) -> &mut Schedule {
        let label = schedule.label();
        self.inner.insert(label, schedule);
        self.inner.get_mut(&label).unwrap()
    }

    /// Inserts `schedule`, returning the previously stored schedule with the
    /// same label, if any.
    pub fn insert(&mut self, schedule: Schedule) -> Option<Schedule> {
        let label = schedule.label();
        self.inner.insert(label, schedule)
    }

    /// Removes and returns the schedule stored under `label`.
    pub fn remove(&mut self, label: impl ScheduleLabel) -> Option<Schedule> {
        self.inner.remove(&label.intern())
    }

    /// Returns the schedule stored under `label`.
    pub fn get(&self, label: impl ScheduleLabel) -> Option<&Schedule> {
        self.inner.get(&label.intern())
    }

    /// Returns the schedule stored under `label` for mutation.
    pub fn get_mut(&mut self, label: impl ScheduleLabel) -> Option<&mut Schedule> {
        self.inner.get_mut(&label.intern())
    }

    /// Returns `true` if a schedule is stored under `label`.
    pub fn contains(&self, label: impl ScheduleLabel) -> bool {
        self.inner.contains_key(&label.intern())
    }

    /// Gets the entry for `label` for in-place manipulation.
    ///
    /// Creates an empty schedule under `label` on first access, then returns
    /// a mutable reference to it, so callers can configure the schedule
    /// without a separate insert step.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::schedule::ScheduleLabel;
    ///
    /// #[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
    /// struct FrameStart;
    ///
    /// let mut schedules = Schedules::new();
    ///
    /// // The first call creates an empty schedule under the label; later
    /// // calls return the same schedule for further mutation.
    /// let schedule = schedules.entry(FrameStart);
    /// assert!(schedule.jobs().next().is_none());
    /// drop(schedule);
    ///
    /// assert!(schedules.contains(FrameStart));
    /// ```
    pub fn entry(&mut self, label: impl ScheduleLabel) -> &mut Schedule {
        let label = label.intern();
        self.inner.entry(label).or_insert(Schedule::new(label))
    }

    /// Iterates all stored schedules.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Schedule> {
        self.inner.values()
    }

    /// Iterates all stored schedules mutably.
    pub fn iter_mut(&mut self) -> impl ExactSizeIterator<Item = &mut Schedule> {
        self.inner.values_mut()
    }

    /// Returns the number of stored schedules.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if no schedules are stored.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Removes all schedules, keeping the allocated memory for reuse.
    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

// -----------------------------------------------------------------------------
// Trait Implementations

impl Deref for Schedules {
    type Target = HashMap<InternedScheduleLabel, Schedule>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Schedules {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Debug for Schedules {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_set().entries(self.inner.keys()).finish()
    }
}

// -----------------------------------------------------------------------------
