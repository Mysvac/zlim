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
            IntoJobResult::into_job_result(ret)
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
        #[cfg(not(feature = "tracy"))]
        return Box::new(JobSystem::<O, T::System, STRICT> { system, id });

        #[cfg(feature = "tracy")]
        let (tracy, defer_tracy) = tracy_span_source(&id, system.flags());
        #[cfg(feature = "tracy")]
        return Box::new(JobSystem::<O, T::System, STRICT> {
            system,
            id,
            tracy,
            defer_tracy,
        });
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
) -> (
    &'static zlim_tracy::SpanSource,
    &'static zlim_tracy::SpanSource,
) {
    let func1 = format!("Job::run_raw::<{}>\0", id.name());
    let func2 = if flag.intersects(SystemFlags::DEFERRED) {
        format!("Job::apply_deferred::<{}>\0", id.name())
    } else {
        String::new()
    };

    let file = c"zlim_core::job";

    (
        zlim_tracy::SpanSource::new_leak(String::new(), func1, file, 0, 0xADFF2F),
        zlim_tracy::SpanSource::new_leak(String::new(), func2, file, 1, 0x2FFFAD),
    )
}

// -----------------------------------------------------------------------------
