//! Conversion of functions and systems into boxed jobs.

use super::{Job, JobId};
use crate::error::{IntoZlimResult, ZlimError};
use crate::system::{AccessTable, SystemFlags};
use crate::system::{IntoSystem, System, SystemError};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------
// IntoJobResult

/// Converts a job function's return value into a [`Result<(), SystemError>`].
///
/// Job functions may return `()`, `bool`, `Result<(), E>`, or
/// `Result<bool, E>`; this trait normalizes those into the scheduler's error
/// convention. A `false` / `Ok(false)` result is mapped to
/// [`SystemError::None`] — a benign early exit that prevents dependent jobs
/// from running — while `()` / `Ok(())` map to success.
pub trait IntoJobResult {
    /// Converts `this` into a scheduler result.
    fn into_job_result(this: Self) -> Result<(), SystemError>;
}

impl<T: IntoZlimResult<()>> IntoJobResult for T {
    #[inline(always)]
    fn into_job_result(this: Self) -> Result<(), SystemError> {
        this.into_zlim_result().map_err(SystemError::Runtime)
    }
}

impl IntoJobResult for bool {
    #[inline(always)]
    fn into_job_result(this: Self) -> Result<(), SystemError> {
        if this { Ok(()) } else { Err(SystemError::None) }
    }
}

impl<E: Into<ZlimError>> IntoJobResult for Result<bool, E> {
    fn into_job_result(this: Self) -> Result<(), SystemError> {
        match this {
            Ok(true) => Ok(()),
            Ok(false) => Err(SystemError::None),
            Err(e) => Err(SystemError::Runtime(e.into())),
        }
    }
}

// -----------------------------------------------------------------------------
// JobSystem

/// A [`Job`] adapter that wraps a system.
#[repr(C)]
pub struct JobSystem<O, S, const STRICT: bool>
where
    O: IntoJobResult + 'static,
    S: System<Input = (), Output = O>,
{
    system: S,
    id: JobId,
}

impl<O, S, const STRICT: bool> Job for JobSystem<O, S, STRICT>
where
    O: IntoJobResult + 'static,
    S: System<Input = (), Output = O>,
{
    fn id(&self) -> JobId {
        self.id
    }

    fn flags(&self) -> SystemFlags {
        self.system.flags()
    }

    fn last_run(&self) -> Tick {
        self.system.last_run()
    }

    fn clamp_ticks(&mut self, now: Tick) {
        self.system.clamp_ticks(now);
    }

    fn set_last_run(&mut self, last_run: Tick) {
        self.system.set_last_run(last_run);
    }

    fn initialize(&mut self, world: &World) {
        self.system.initialize(world);
    }

    fn register_access(&self, table: &mut AccessTable) {
        self.system.register_access(table, STRICT);
    }

    unsafe fn run_raw(&mut self, world: WorldCell<'_>) -> Result<(), SystemError> {
        unsafe {
            let ret = self.system.run_raw((), world)?;
            IntoJobResult::into_job_result(ret)
        }
    }

    fn apply_deferred(&mut self, world: &mut World) {
        self.system.apply_deferred(world);
    }
}

// -----------------------------------------------------------------------------
// IntoJob

/// Converts a function or system into a boxed [`Job`].
///
/// This is the bridge used by the `job!` macro. `STRICT` selects whether the
/// job registers its access strictly (`true` — logs access conflicts) or
/// permissively (`false`).
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::job::IntoJob;
///
/// fn my_system() {}
///
/// // Build a strict boxed job from a plain function:
/// let mut job = IntoJob::into_job::<true>(my_system, "my_job", "my_group");
///
/// let world = World::alloc();
/// job.initialize(&world);
/// assert_eq!(job.id().name(), "my_job");
/// ```
pub trait IntoJob<O: IntoJobResult, M>: IntoSystem<(), O, M> {
    /// Converts `this` into a boxed job with the given name and group.
    fn into_job<const STRICT: bool>(
        this: Self,
        name: &'static str,
        group: &'static str,
    ) -> Box<dyn Job>;
}

impl<O, M, T> IntoJob<O, M> for T
where
    O: IntoJobResult + 'static,
    T: IntoSystem<(), O, M>,
{
    fn into_job<const STRICT: bool>(
        this: Self,
        name: &'static str,
        group: &'static str,
    ) -> Box<dyn Job> {
        let id = JobId::new(name, group);
        let system: T::System = IntoSystem::into_system(this);
        Box::new(JobSystem::<O, T::System, STRICT> { system, id })
    }
}

// -----------------------------------------------------------------------------
