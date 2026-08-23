//! Schedule label definition and interning.
//!
//! [`ScheduleLabel`] identifies a schedule within a [`Schedules`] collection.
//! Labels are interned so comparison and hashing use pointer equality on the
//! interned value rather than full structural comparison.
//!
//! [`Schedules`]: crate::schedule::Schedules

use crate::define_label;
use crate::label::Interned;

pub use zlim_core_derive::ScheduleLabel;

// -----------------------------------------------------------------------------
// ScheduleLabel

define_label!(
    /// A strongly-typed class of labels used to identify a [`Schedule`].
    ///
    /// Each schedule in a [`Schedules`] collection is keyed by its label value;
    /// labels are interned so lookups compare pointer identities rather than
    /// full label values.
    ///
    /// Prefer defining your own label enums/structs with
    /// `#[derive(ScheduleLabel)]` for stable, explicit schedule routing.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::schedule::{InternedScheduleLabel, ScheduleLabel};
    ///
    /// /// Labels for the game's main loop stages.
    /// #[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
    /// enum MainLoop {
    ///     Update,
    ///     Render,
    /// }
    ///
    /// // Interning deduplicates equivalent labels to a single canonical
    /// // handle, so comparisons use pointer identity.
    /// let a: InternedScheduleLabel = MainLoop::Update.intern();
    /// let b: InternedScheduleLabel = MainLoop::Update.intern();
    /// assert_eq!(a, b);
    ///
    /// // Schedules accept any label implementation.
    /// let schedule = Schedule::new(MainLoop::Update);
    /// assert_eq!(schedule.label(), MainLoop::Update.intern());
    /// ```
    ///
    /// [`Schedule`]: crate::schedule::Schedule
    /// [`Schedules`]: crate::schedule::Schedules
    #[diagnostic::on_unimplemented(
        note = "consider annotating `{Self}` with `#[derive(ScheduleLabel)]`"
    )]
    ScheduleLabel,
    SCHEDULE_LABEL_INTERNER
);

/// A shorthand for `Interned<dyn ScheduleLabel>`.
pub type InternedScheduleLabel = Interned<dyn ScheduleLabel>;

/// Built-in marker label for anonymous schedules.
///
/// A zero-sized [`ScheduleLabel`] for schedules that do not need a
/// meaningful identity, such as ad-hoc schedules created inline.
///
/// [`ScheduleLabel`]: ScheduleLabel
#[derive(ScheduleLabel, Clone, Copy, Default, Debug, Hash, PartialEq, Eq)]
pub struct AnonymousSchedule;

// -----------------------------------------------------------------------------
