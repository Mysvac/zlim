//! Conversion of functions and systems into boxed jobs.

use zlim_error::IntoZlimResult;
use zlim_utils::debug::DebugLocation;

use super::{Job, JobId};
use crate::system::{AccessTable, SystemFlags};
use crate::system::{IntoSystem, System, SystemError};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------
// IntoJobResult

/// Converts a job function's return value into a [`Result<(), SystemError>`].
///
/// A job's output has to answer one question: should this job run? The return
/// conventions that can answer it are `()` (run, since it succeeded), `bool`
/// (the answer itself), and the `Result` forms of either (the answer, unless
/// something failed). All of them are covered by `IntoZlimResult<bool>`.
///
/// `Ok(true)` runs the job. `Ok(false)` is mapped to [`SystemError::Skipped`] — the
/// job is skipped, and since it is reported as skipped rather than run, its
/// delayed commands are not applied and dependents do not run. An `Err` is a
/// real failure and is reported to the error handler.
pub trait IntoJobResult {
    /// Converts `this` into a scheduler result.
    fn into_job_result(this: Self, location: DebugLocation) -> Result<(), SystemError>;
}

impl<T: IntoZlimResult<bool>> IntoJobResult for T {
    #[inline(always)]
    fn into_job_result(this: Self, location: DebugLocation) -> Result<(), SystemError> {
        match this.into_zlim_result() {
            Ok(true) => Ok(()),
            Ok(false) => Err(SystemError::Skipped),
            Err(e) => Err(SystemError::Runtime(e.with_location(location))),
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
    location: DebugLocation,
    /// The `tracy::Span` used for performance observation.
    ///
    /// `tracing::Span` and `tracy::Span` have different requirements:
    ///
    /// - The `tracing::Span` is for logging, so it must also cover the error
    ///   handling logic. But error handling happens outside of the job's `run`,
    ///   so the span is stored in `Schedule` to cover a wider scope.
    ///
    /// - The Tracy span instruments performance, so it is cached as a field of
    ///   the `Job` and begun/entered when the job itself runs.
    ///
    /// Although zlim_tracy supports a lightweight mode when the feature `tracy`
    /// is not enabled, `core` is a low-level library — has to optimize as
    /// aggressively as it can. so we still annotate it with `#[cfg(..)]`.
    #[cfg(feature = "tracy")]
    tracy: &'static zlim_tracy::SpanSource,
    #[cfg(feature = "tracy")]
    defer_tracy: &'static zlim_tracy::SpanSource,
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
        #[cfg(feature = "tracy")]
        let _span = self.tracy.begin();
        unsafe {
            let ret = self.system.run_raw((), world)?;
            IntoJobResult::into_job_result(ret, self.location)
        }
    }

    fn apply_deferred(&mut self, world: &mut World) {
        #[cfg(feature = "tracy")]
        let _span = self.defer_tracy.begin();
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
    // ↓ track_caller: Track the definition point of the job.
    #[track_caller]
    fn into_job<const STRICT: bool>(
        this: Self,
        name: &'static str,
        group: &'static str,
    ) -> Box<dyn Job> {
        let id = JobId::new(name, group);
        let system: T::System = IntoSystem::into_system(this);

        let location = DebugLocation::caller();

        #[cfg(feature = "tracy")]
        let tracy_location = ::core::panic::Location::caller();

        #[cfg(feature = "tracy")]
        let (tracy, defer_tracy) = tracy_span_source(&id, system.flags(), tracy_location);

        Box::new(JobSystem::<O, T::System, STRICT> {
            system,
            id,
            location,
            #[cfg(feature = "tracy")]
            tracy,
            #[cfg(feature = "tracy")]
            defer_tracy,
        })
    }
}

// In the current implementation, Jobs can only be inserted into a Schedule to run,
// so frequent creation should not occur. We can directly intern the span source.
//
// In the future, if jobs require frequent additions and deletions, they will need
// to add deduplication logic.
#[inline(never)]
#[cfg(feature = "tracy")]
fn tracy_span_source(
    id: &JobId,
    flag: SystemFlags,
    location: &'static ::core::panic::Location,
) -> (
    &'static zlim_tracy::SpanSource,
    &'static zlim_tracy::SpanSource,
) {
    let func1 = format!("Job::run_raw::<{}>\0", id.name());
    let func2 = if flag.intersects(SystemFlags::DEFERRED) {
        format!("Job::deferred::<{}>\0", id.name())
    } else {
        String::new()
    };

    let file = location.file_as_c_str();
    let line = location.line();

    (
        zlim_tracy::SpanSource::new_leak(String::new(), func1, file, line, 0xADFF2F),
        zlim_tracy::SpanSource::new_leak(String::new(), func2, file, line, 0x2FFFAD),
    )
}

// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::schedule::AnonymousSchedule;
    use crate::{derive::job, world::World};
    use zlim_error::ZlimError;

    job! {
        type: TestError,
        system: || -> Result<(), ZlimError> { Err(ZlimError::info("TestError")) },
        auto_register: false,
    }

    #[test]
    #[ignore = "manual trigger"]
    fn zlim_error_location() {
        zlim_log::LogConfig::default().apply();
        let mut world = World::alloc();
        world.insert_job::<TestError>(AnonymousSchedule, ());
        world.run_schedule(AnonymousSchedule);
    }
}
