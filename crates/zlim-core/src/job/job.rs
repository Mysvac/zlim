#![expect(clippy::module_inception, reason = "For better structure.")]

//! The [`Job`] trait: the runtime interface of a schedulable unit of work.

use super::JobId;
use crate::system::{AccessTable, SystemError, SystemFlags};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

/// The runtime interface of a job — a schedulable unit executed against
/// the [`World`].
///
/// Jobs are produced by [`JobDB`] constructors (see the `job_fn` and
/// `job!` macros) and are executed by the job scheduler.  Each
/// job carries a [`JobId`], scheduling [`SystemFlags`], and its own
/// change-detection baseline tick.
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::job::Job;
///
/// // `#[job_fn]` generates a `JobLabel` marker whose `database()` builds a
/// // boxed `Job` from a plain function:
/// #[job_fn(type = MyJob, name = "my_job")]
/// fn my_job() {}
///
/// let mut job = (MyJob::database().ctor)("my_group");
///
/// // Every job carries a stable identity:
/// assert_eq!(job.id().name(), "my_job");
/// assert_eq!(job.id().group(), "my_group");
///
/// // Jobs are initialized once against the world they run in:
/// let world = World::alloc();
/// job.initialize(&world);
///
/// // Execution is an `unsafe` operation performed by the scheduler through
/// // a raw `WorldCell`:
/// // let _ = unsafe { job.run(world.cell()) };
/// ```
///
/// [`JobDB`]: super::JobDB
pub trait Job: Send + Sync + 'static {
    /// Returns the unique identifier of this job.
    fn id(&self) -> JobId;

    /// Returns the scheduling flags of this job.
    fn flags(&self) -> SystemFlags;

    /// Returns the tick when this job last ran.
    ///
    /// `initialize` seeds this with the world's current tick, and the
    /// scheduler updates it after each execution via [`set_last_run`].
    ///
    /// [`set_last_run`]: Self::set_last_run
    fn last_run(&self) -> Tick;

    /// Clamps stored change-detection ticks against `now` to keep them
    /// within a valid range after tick wrap-around.
    fn clamp_ticks(&mut self, now: Tick);

    /// Sets the tick when this job last ran.
    fn set_last_run(&mut self, last_run: Tick);

    /// Initializes this job against the given world.
    ///
    /// Called once before the job is first executed.
    ///
    /// The implementer must ensure that this function is safe to be
    /// called repeatedly. And initialization should be skipped directly
    /// when called repeatedly to endure performance.
    fn initialize(&mut self, world: &World);

    /// Declares the world / resource access used by this job.
    ///
    /// The scheduler uses this to detect conflicts between concurrently
    /// running jobs.
    fn register_access(&self, table: &mut AccessTable);

    /// Executes the job's logic against the given world.
    ///
    /// This function does not initialize the Job. If the Job is not
    /// initialized, the call always returns [`SystemError::Uninitialized`]`.
    ///
    /// Due to the uncertain accessibility of World, this function will not
    /// handle delayed commands submitted.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the world is accessed only according to
    /// the access patterns declared through [`register_access`], and that
    /// concurrent jobs do not access the world in conflicting ways.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use zlim_core::prelude::*;
    /// use zlim_core::job::{Job, JobDB};
    ///
    /// #[job_fn(type = GreetJob, name = "greet_job")]
    /// fn greet_job() {}
    ///
    /// let mut job = GreetJob::database().ctor("");
    /// let world = World::alloc();
    /// job.initialize(&world);
    ///
    /// // The scheduler hands the job a `WorldCell` and propagates failures
    /// // as `SystemError`:
    /// let result = unsafe { job.run(world.cell()) };
    /// assert!(result.is_ok());
    /// ```
    ///
    /// [`register_access`]: Self::register_access
    unsafe fn run_raw(&mut self, world: WorldCell<'_>) -> Result<(), SystemError>;

    /// Applies queued deferred commands to the world.
    ///
    /// The scheduler calls this only for jobs whose flags include
    /// `DEFERRED`.
    fn apply_deferred(&mut self, world: &mut World);
}

impl dyn Job {
    /// Executes the job's logic against the provided world, then applies
    /// any queued deferred effects.
    ///
    /// This function will also automatically initialize the job.
    pub fn run(&mut self, world: &mut World) -> Result<(), SystemError> {
        self.initialize(world);
        let result = unsafe { self.run_raw(world.cell()) };
        self.apply_deferred(world);
        result
    }
}
