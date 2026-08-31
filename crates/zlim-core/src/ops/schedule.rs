use crate::schedule::{MissingSchedule, Schedule, ScheduleLabel};
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
    /// This method temporarily removes the schedule from the world to satisfy
    /// Rust's borrowing rules, allowing the closure to mutably borrow both the
    /// schedule and the world simultaneously.
    ///
    /// # Panics
    ///
    /// If the requested schedule does not exist.
    pub fn schedule_scope<R>(
        &mut self,
        label: impl ScheduleLabel,
        func: impl FnOnce(&mut World, &mut Schedule) -> R,
    ) -> R {
        let label = label.intern();
        let schedule = self.schedules.remove(label);
        let schedule = schedule.or_else(|| {
            ::core::hint::cold_path();
            panic!("The schedule with the label {label:?} was not found.")
        });

        let world = self.cell();
        let mut guard = ReinsertGuard { world, schedule };

        let world = unsafe { world.full_mut() };
        let schedule = unsafe { guard.schedule.as_mut().unwrap_unchecked() };

        func(world, schedule)
    }

    /// Executes a closure with exclusive access to a schedule and the world.
    ///
    /// If the schedule does not exist, returns `Err` directly.
    ///
    /// This method temporarily removes the schedule from the world to satisfy
    /// Rust's borrowing rules, allowing the closure to mutably borrow both the
    /// schedule and the world simultaneously.
    pub fn try_schedule_scope<R>(
        &mut self,
        label: impl ScheduleLabel,
        func: impl FnOnce(&mut World, &mut Schedule) -> R,
    ) -> Result<R, MissingSchedule> {
        let label = label.intern();
        let missing = MissingSchedule { label };
        let schedule = Some(self.schedules.remove(label).ok_or(missing)?);

        let world = self.cell();
        let mut guard = ReinsertGuard { world, schedule };

        let world = unsafe { world.full_mut() };
        let schedule = unsafe { guard.schedule.as_mut().unwrap_unchecked() };

        Ok(func(world, schedule))
    }

    /// Runs the schedule with the given label.
    ///
    /// # Panics
    ///
    /// If the requested schedule does not exist.
    pub fn run_schedule(&mut self, label: impl ScheduleLabel) {
        self.schedule_scope(label.intern(), |world, sched| sched.run(world));
    }

    /// Runs the schedule with the given label.
    ///
    /// Return Error if the schedule does not exist.
    pub fn try_run_schedule(&mut self, label: impl ScheduleLabel) -> Result<(), MissingSchedule> {
        self.try_schedule_scope(label.intern(), |world, sched| sched.run(world))
    }
}
