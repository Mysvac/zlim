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
///
/// A `PhantomData<T>` output always succeeds, letting generic jobs use a type
/// parameter in their signature without producing an actual value.
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

// A `PhantomData` output carries no runtime data, so it always succeeds.
// This lets generic jobs use a type parameter in their signature without
// producing an actual value.
impl<T: 'static> IntoJobResult for core::marker::PhantomData<T> {
    #[inline(always)]
    fn into_job_result(_: Self) -> Result<(), SystemError> {
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// JobSystem

/// A [`Job`] adapter that wraps a system and registers non-strict access.
#[repr(C)]
pub struct JobSystem<O, S: System<Input = (), Output = O>> {
    system: S,
    id: JobId,
}

/// A [`Job`] adapter that wraps a system and registers strict access.
///
/// Strict access makes the scheduler log access-conflict diagnostics that
/// non-strict jobs would silently allow.
#[repr(C)]
pub struct StrictJobSystem<O, S: System<Input = (), Output = O>> {
    system: S,
    id: JobId,
}

macro_rules! impl_job_for {
    ($ty:ident, $strict:expr) => {
        impl<O, S> Job for $ty<O, S>
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
                self.system.register_access(table, $strict);
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
    };
}

impl_job_for!(JobSystem, false);
impl_job_for!(StrictJobSystem, true);

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
        let system = IntoSystem::into_system(this);
        let id = JobId::new(name, group);
        if STRICT {
            Box::new(StrictJobSystem { system, id })
        } else {
            Box::new(JobSystem { system, id })
        }
    }
}

// -----------------------------------------------------------------------------
