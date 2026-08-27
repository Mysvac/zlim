use crate::schedule::{Schedule, ScheduleLabel};
use crate::world::{World, WorldCell};

impl World {
    /// Inserts a schedule into the world, returning the old one if it
    /// exists.
    ///
    /// If a schedule with the same label already exists, it will be replaced.
    pub fn insert_schedule(&mut self, schedule: Schedule) -> Option<Schedule> {
        self.schedules.insert(schedule)
    }

    /// Removes a schedule from the world if it exists.
    ///
    /// Returns the removed schedule, or `None` if no schedule with the
    /// given label exists.
    pub fn remove_schedule(&mut self, label: impl ScheduleLabel) -> Option<Schedule> {
        self.schedules.remove(label.intern())
    }

    /// Returns a mutable reference to the schedule with the given label.
    ///
    /// Initializes a new empty schedule if it doesn't exist.
    pub fn schedule_entry(&mut self, label: impl ScheduleLabel) -> &mut Schedule {
        self.schedules.entry(label.intern())
    }
}

struct ReinsertGuard<'w> {
    world: WorldCell<'w>,
    schedule: Option<Schedule>,
}

impl Drop for ReinsertGuard<'_> {
    fn drop(&mut self) {
        let world = unsafe { self.world.full_mut() };
        let schedule = self.schedule.take().unwrap();

        if let Some(s) = world.schedules.insert(schedule) {
            ::core::hint::cold_path();
            let label = s.label();
            zlim_log::warn!(
                "Schedule `{label:?}` was inserted during a call to \
                `World::schedule_scope`, its value has been overwritten."
            );
        }
    }
}

impl World {
    /// Executes a closure with exclusive access to a schedule and the world.
    ///
    /// **Initializes a new empty schedule if it doesn't exist.**
    ///
    /// If you do not want to create a new schedule, use [`World::try_schedule_scope`] instead.
    ///
    /// This method temporarily removes the schedule from the world to satisfy
    /// Rust's borrowing rules, allowing the closure to mutably borrow both the
    /// schedule and the world simultaneously.
    pub fn schedule_scope<R>(
        &mut self,
        label: impl ScheduleLabel,
        func: impl FnOnce(&mut World, &mut Schedule) -> R,
    ) -> R {
        let label = label.intern();
        let schedule = self.schedules.remove(label);
        let schedule = schedule.or_else(|| {
            ::core::hint::cold_path();
            Some(Schedule::new(label))
        });

        let world = self.cell();
        let mut guard = ReinsertGuard { world, schedule };

        let world = unsafe { world.full_mut() };
        let schedule = unsafe { guard.schedule.as_mut().unwrap_unchecked() };

        func(world, schedule)
    }

    /// Executes a closure with exclusive access to a schedule and the world.
    ///
    /// If the schedule does not exist, returns `None` directly.
    ///
    /// This method temporarily removes the schedule from the world to satisfy
    /// Rust's borrowing rules, allowing the closure to mutably borrow both the
    /// schedule and the world simultaneously.
    pub fn try_schedule_scope<R>(
        &mut self,
        label: impl ScheduleLabel,
        func: impl FnOnce(&mut World, &mut Schedule) -> R,
    ) -> Option<R> {
        let label = label.intern();
        let schedule = Some(self.schedules.remove(label)?);

        let world = self.cell();
        let mut guard = ReinsertGuard { world, schedule };

        let world = unsafe { world.full_mut() };
        let schedule = unsafe { guard.schedule.as_mut().unwrap_unchecked() };

        Some(func(world, schedule))
    }

    /// Runs the schedule with the given label.
    ///
    /// **Initializes a new empty schedule if it doesn't exist.**
    ///
    /// If you do not want to create a new schedule, use [`World::try_run_schedule`] instead.
    ///
    /// This is a convenience method that combines `schedule_scope`
    /// with running the schedule.
    pub fn run_schedule(&mut self, label: impl ScheduleLabel) {
        self.schedule_scope(label.intern(), |world, sched| sched.run(world));
    }

    /// Runs the schedule with the given label, if it exists.
    ///
    /// If the schedule does not exist, returns directly.
    ///
    /// This is a convenience method that combines `try_schedule_scope`
    /// with running the schedule.
    pub fn try_run_schedule(&mut self, label: impl ScheduleLabel) {
        self.try_schedule_scope(label.intern(), |world, sched| sched.run(world));
    }
}
