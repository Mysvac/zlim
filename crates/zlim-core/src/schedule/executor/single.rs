//! Single-threaded schedule executor.

use core::panic::AssertUnwindSafe;

use zlim_log as log;

use super::{ExecutorKind, JobExecutor, JobSchedule, JobScheduleView};
use crate::error::{ErrorContext, ErrorHandler};
use crate::system::{SystemError, SystemFlags};
use crate::world::World;

// -----------------------------------------------------------------------------
// SingleThreadedExecutor

/// Runs the schedule using a single thread.
///
/// Useful if you're dealing with a single-threaded environment, saving your
/// threads for other things, or just trying to minimize overhead.
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::schedule::{ExecutorKind, ScheduleLabel, SingleThreadedExecutor};
///
/// #[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// struct Update;
///
/// // `SingleThreadedExecutor` is one of the built-in `JobExecutor`s that
/// // `Schedule::with_executor` accepts.
/// let schedule =
///     Schedule::with_executor(Update, Box::new(SingleThreadedExecutor::new()));
/// assert_eq!(schedule.executor_kind(), ExecutorKind::SingleThreaded);
/// ```
pub struct SingleThreadedExecutor {
    strong_incoming: Vec<u16>,
}

// -----------------------------------------------------------------------------
// ctor

impl SingleThreadedExecutor {
    /// Creates a new single-threaded executor.
    pub const fn new() -> Self {
        Self {
            strong_incoming: Vec::new(),
        }
    }
}

impl Default for SingleThreadedExecutor {
    fn default() -> Self {
        Self::new()
    }
}

// -----------------------------------------------------------------------------
// JobExecutor

impl JobExecutor for SingleThreadedExecutor {
    /// Returns [`ExecutorKind::SingleThreaded`].
    ///
    /// [`ExecutorKind::SingleThreaded`]: crate::schedule::ExecutorKind::SingleThreaded
    fn kind(&self) -> ExecutorKind {
        ExecutorKind::SingleThreaded
    }

    /// Validates the compiled schedule is internally consistent.
    fn init(&mut self, schedule: &JobSchedule) {
        let nodes = schedule.nodes();
        let jobs = schedule.jobs();
        assert_eq!(nodes.len(), jobs.len());
    }

    /// Runs all systems sequentially on the current thread.
    fn exec(&mut self, schedule: &mut JobSchedule, world: &mut World, handler: ErrorHandler) {
        let JobScheduleView {
            jobs,
            flags,
            nodes,
            strong_incoming,
            strong_outgoing,
            ..
        } = schedule.view();

        let job_count = jobs.len();
        assert_eq!(job_count, flags.len());
        assert_eq!(job_count, strong_incoming.len());
        assert_eq!(job_count, strong_outgoing.len());
        debug_assert_eq!(job_count, nodes.len());

        self.strong_incoming.clear();
        self.strong_incoming.extend_from_slice(strong_incoming);

        for (index, (job, &flag)) in jobs.iter_mut().zip(flags).enumerate() {
            // Condition not met, skip.
            if self.strong_incoming[index] != 0 {
                continue; // next system
            }

            // Noop Job, skip.
            if flag.intersects(SystemFlags::NO_OP) {
                // SAFETY: Already checked above - `assert_eq!(job_count, strong_outgoing.len());`
                let outgoing = unsafe { *strong_outgoing.get_unchecked(index) };
                for &to in outgoing {
                    let to = to as usize;
                    debug_assert!(self.strong_incoming.len() > to);
                    // SAFETY: Caller ensure that `JobSchedule` is correct.
                    unsafe { *self.strong_incoming.get_unchecked_mut(to) -= 1 };
                }
                continue;
            }

            // Normal Job
            let func = AssertUnwindSafe(|| unsafe {
                if let Err(e) = job.run_raw(world.cell()) {
                    core::hint::cold_path();
                    if !matches!(e, SystemError::None) {
                        core::hint::cold_path();
                        let tick = job.last_run();
                        let id = job.id();
                        let name = id.name();
                        let group = id.group();
                        let ctx = ErrorContext::Job { name, group, tick };
                        handler(e.into(), ctx);
                    }
                    return false; // Error -> false
                }
                true // Success -> true
            });

            let ok = std::panic::catch_unwind(func).unwrap_or_else(|payload| {
                core::hint::cold_path();
                log::error!("Encountered a panic in job `{}`!", job.id());
                std::panic::resume_unwind(payload);
            });

            // Apply deferred
            if flag.intersects(SystemFlags::DEFERRED) {
                job.apply_deferred(unsafe { world.cell().full_mut() });
            }

            if ok {
                // SAFETY: Already checked above - `assert_eq!(job_count, strong_outgoing.len());`
                let outgoing = unsafe { *strong_outgoing.get_unchecked(index) };
                for &to in outgoing {
                    let to = to as usize;
                    debug_assert!(self.strong_incoming.len() > to);
                    // SAFETY: Caller ensure that `JobSchedule` is correct.
                    unsafe { *self.strong_incoming.get_unchecked_mut(to) -= 1 };
                }
            }
        }

        world.flush();
    }
}

// -----------------------------------------------------------------------------
