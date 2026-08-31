//! Multi-threaded schedule executor.

use core::any::Any;
use core::panic::AssertUnwindSafe;
use std::collections::{BTreeSet, VecDeque};

use zlim_task::{MainTaskPool, Scope};
use zlim_utils::exp::SyncUnsafeCell;
use zlim_utils::sync::{SegQueue, SpinLock};
use zlim_utils::vec::FastVec;

use super::{ConflictTable, ExecutorKind, JobExecutor, JobSchedule, JobScheduleView};

use crate::error::{ErrorContext, ErrorHandler, PanicPayload};
use crate::job::Job;
use crate::schedule::InternedScheduleLabel;
use crate::system::{SystemError, SystemFlags};
use crate::utils::DebugCheckedUnwrap;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------
// MultiThreadedExecutor

/// Completion event emitted by system tasks.
///
/// `index` is the job index of `JobSchedule`.
///
/// `deferred` means the job is `DEFERRED`, need to apply deferred commands.
///
/// `successed` means the job `run` return `Ok(())`.
struct Completed {
    index: u16,
    deferred: bool,
    successed: bool,
}

/// Mutable scheduling state reused between runs.
///
/// This stores runtime counters and queues derived from `JobSchedule`.
/// Buffers are pre-allocated in `init` and refreshed in `reset`.
struct ExecutorState {
    /// Remaining dependency counts for each system.
    incoming: Vec<u16>,
    /// Remaining strong dependencies for each system.
    strong_incoming: Vec<u16>,
    /// Runnable systems whose dependencies are currently satisfied.
    ready_systems: VecDeque<u16>,
    /// Systems currently executing.
    running_jobs: BTreeSet<u16>,
    /// Deferred systems that have run and still need `apply_deferred`.
    deferred_systems: Vec<u16>,
}

/// Buffer for deferring panic propagation until after execution completes.
struct PanicBuffer(SpinLock<Option<Box<PanicPayload>>>);

/// Runs the schedule on multiple worker threads.
///
/// The executor tracks dependency counters (`incoming`) and a ready queue,
/// spawning tasks for systems whose dependencies are satisfied.
///
/// Non-send systems are dispatched to the external/main-thread executor when
/// available; sendable systems run on the compute task pool.
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::schedule::{ExecutorKind, MultiThreadedExecutor};
///
/// #[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// struct Update;
///
/// // `MultiThreadedExecutor` runs independent jobs in parallel on the task
/// // pool; conflicting jobs still execute in a safe serialized order.
/// let schedule =
///     Schedule::with_executor(Update, Box::new(MultiThreadedExecutor::new()));
///
/// assert_eq!(schedule.executor_kind(), ExecutorKind::MultiThreaded);
/// ```
pub struct MultiThreadedExecutor {
    #[cfg(feature = "trace")]
    sync_span: SyncUnsafeCell<Option<zlim_log::Span>>,
    panic_buffer: PanicBuffer,
    // Each thread only uses try-lock, so spin-lock is better than Mutex.
    state: SpinLock<ExecutorState>,
    completed: SegQueue<Completed>,
}

// -----------------------------------------------------------------------------
// Context

/// Multi threaded context
#[derive(Copy, Clone)]
struct Context<'scope, 'env, 'sys> {
    world: WorldCell<'env>,
    label: InternedScheduleLabel,
    executor: &'env MultiThreadedExecutor,
    scope: &'scope Scope<'scope, 'env, ()>,
    jobs: &'sys [SyncUnsafeCell<Box<dyn Job>>],
    flags: &'sys [SystemFlags],
    outgoing: &'sys [&'sys [u16]],
    strong_outgoing: &'sys [&'sys [u16]],
    conflict_table: &'sys ConflictTable,
    error_handler: ErrorHandler,
    #[cfg(feature = "trace")]
    spans: &'sys [SyncUnsafeCell<zlim_log::Span>],
}

// -----------------------------------------------------------------------------
// MultiThreadedExecutor ctor

impl Default for MultiThreadedExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiThreadedExecutor {
    /// Creates a new multi-threaded executor.
    pub const fn new() -> Self {
        Self {
            #[cfg(feature = "trace")]
            sync_span: SyncUnsafeCell::new(None),
            state: SpinLock::new(ExecutorState::new()),
            completed: SegQueue::new(),
            panic_buffer: PanicBuffer(SpinLock::new(None)),
        }
    }
}

// -----------------------------------------------------------------------------
// PanicBuffer

impl PanicBuffer {
    #[cold]
    #[inline(never)]
    fn preserve_payload(
        &self,
        payload: Box<dyn Any + Send>,
        job: &dyn Job,
        label: InternedScheduleLabel,
    ) {
        let payload = match payload.downcast::<PanicPayload>() {
            Ok(panic_payload) => panic_payload,
            #[expect(clippy::print_stderr, reason = "panic outout")]
            Err(payload) => {
                ::core::hint::cold_path();
                std::eprintln!(
                    "Encounter a panic in schedule `{label:?}`'s job `{}`.",
                    job.id()
                );
                Box::new(PanicPayload { payload })
            }
        };

        let mut slot = self.0.lock();

        if slot.is_none() {
            *slot = Some(payload);
        }

        ::core::mem::drop(slot);
    }

    #[inline(always)]
    fn take(&mut self) -> Option<Box<PanicPayload>> {
        self.0.get_mut().take()
    }
}

// -----------------------------------------------------------------------------
// ExecutorState Implementation

impl ExecutorState {
    const fn new() -> Self {
        Self {
            incoming: Vec::new(),
            strong_incoming: Vec::new(),
            ready_systems: VecDeque::new(),
            running_jobs: BTreeSet::new(),
            deferred_systems: Vec::new(),
        }
    }

    fn init(&mut self, schedule: &JobSchedule) {
        let job_count = schedule.nodes().len();
        let full_size_hint = job_count + (job_count >> 3);
        let half_size_hint = job_count >> 3;

        self.incoming = Vec::with_capacity(full_size_hint);
        self.strong_incoming = Vec::with_capacity(full_size_hint);
        self.ready_systems = VecDeque::with_capacity(half_size_hint);
        self.deferred_systems = Vec::with_capacity(half_size_hint);
    }

    #[inline]
    fn reset(&mut self, schedule: &JobSchedule) {
        let job_count = schedule.jobs.len();
        assert_eq!(job_count, schedule.flags.len());
        assert_eq!(job_count, schedule.nodes.len());
        assert_eq!(job_count, schedule.incoming.len());
        assert_eq!(job_count, schedule.outgoing.len());
        assert_eq!(job_count, schedule.strong_incoming.len());
        assert_eq!(job_count, schedule.strong_outgoing.len());
        #[cfg(feature = "trace")]
        assert_eq!(job_count, schedule.spans.len());

        self.incoming.clear();
        self.strong_incoming.clear();
        self.ready_systems.clear();
        self.running_jobs.clear();
        self.deferred_systems.clear();

        self.incoming.extend_from_slice(&schedule.incoming);
        self.strong_incoming
            .extend_from_slice(&schedule.strong_incoming);

        self.incoming.iter().enumerate().for_each(|(idx, &num)| {
            if num == 0 {
                // `incoming` logically includes `condition_incoming`, therefore,
                // nodes with an initial 0 incoming do not need to check the condition.
                self.ready_systems.push_back(idx as u16);
            }
        });
    }
}

// -----------------------------------------------------------------------------
// Context Implementation

impl<'scope, 'env: 'scope, 'sys: 'scope> Context<'scope, 'env, 'sys> {
    #[inline]
    fn new(
        world: &'env mut World,
        executor: &'env MultiThreadedExecutor,
        schedule: &'sys mut JobSchedule,
        scope: &'scope Scope<'scope, 'env, ()>,
        error_handler: ErrorHandler,
    ) -> Self {
        let JobScheduleView {
            label,
            jobs,
            flags,
            outgoing,
            strong_outgoing,
            conflict,
            #[cfg(feature = "trace")]
            spans,
            ..
        } = schedule.view();

        Self {
            world: world.cell(),
            label,
            executor,
            scope,
            jobs: SyncUnsafeCell::from_mut(jobs).transpose(),
            flags,
            outgoing,
            strong_outgoing,
            conflict_table: conflict,
            error_handler,
            #[cfg(feature = "trace")]
            spans: SyncUnsafeCell::from_mut(spans).transpose(),
        }
    }
}

impl<'scope, 'env: 'scope, 'sys: 'scope> Context<'scope, 'env, 'sys> {
    /// Progresses scheduling work until no fresh completion event is observed.
    #[inline(always)]
    fn tick(&self) {
        loop {
            let Some(mut guard) = self.executor.state.try_lock() else {
                // Another thread is already advancing scheduling state.
                return;
            };
            self.tick_internal(&mut guard);
            // Make sure we drop the guard before checking
            // completed.is_empty(), or we could lose events.
            ::core::mem::drop(guard);
            // We cannot check `is_empty` before `tick_internal` because
            // initial dependency-free systems start in `ready_systems`,
            // not in `completed`.
            if self.executor.completed.is_empty() {
                return;
            }
        }
    }

    /// Drains completion events, then schedules newly-unblocked tasks.
    #[inline(never)]
    fn tick_internal(&self, state: &mut ExecutorState) {
        let completed_queue = &self.executor.completed;

        while let Some(signal) = completed_queue.pop() {
            self.handle_completed_job::<false>(state, signal);
        }

        self.spawn_ready_tasks(state);
    }

    /// Consumes completion events and propagates dependency updates.
    fn handle_completed_job<const SKIP_NOOP: bool>(
        &self,
        state: &mut ExecutorState,
        completed: Completed,
    ) {
        // Use an explicit stack to avoid deep recursion on long skip chains.
        let mut buffer: FastVec<Completed, 5> = FastVec::new();
        let pending = buffer.data();

        // SAFETY: Inlined capacity > 0.
        unsafe { pending.push_unchecked(completed) };

        while let Some(item) = pending.pop() {
            let Completed {
                index,
                deferred,
                successed,
            } = item;

            let index_t = index as usize;

            let _ = state.running_jobs.remove(&index);

            // systems that need to apply_deferred
            if deferred {
                // enqueued and wait for the synchronization point
                state.deferred_systems.push(index);
            }

            if successed {
                // SAFETY: Already checked during `init` and `reset`.
                let outgoing = unsafe { *self.strong_outgoing.get_unchecked(index_t) };
                for &to in outgoing {
                    let to = to as usize;
                    debug_assert!(state.strong_incoming.len() > to);
                    // SAFETY: Caller ensure that `JobSchedule` is correct.
                    unsafe { *state.strong_incoming.get_unchecked_mut(to) -= 1 };
                }
            }

            let outgoing = unsafe { *self.outgoing.get_unchecked(index_t) };

            for &to in outgoing {
                let to_t = to as usize;

                debug_assert!(to_t < state.incoming.len());
                // SAFETY: Caller ensure that `JobSchedule` is correct.
                let incoming = unsafe { state.incoming.get_unchecked_mut(to_t) };

                *incoming -= 1;

                // incoming != 0: the prerequisite items are not
                if *incoming != 0 {
                    continue;
                }
                // incoming == 0: the prerequisite jobs are all completed

                debug_assert!(to_t < state.strong_incoming.len());
                // SAFETY: Caller ensure that `JobSchedule` is correct.
                if unsafe { *state.strong_incoming.get_unchecked_mut(to_t) != 0 } {
                    // Skip systems whose strong incoming are unresolved,
                    // but continue propagating completion to dependents.
                    pending.push(Completed {
                        index: to,
                        deferred: false,
                        successed: false,
                    });
                    continue;
                }

                // incoming == 0 && strong_incoming == 0: ready

                // Noop, complete directly
                if SKIP_NOOP {
                    debug_assert!(to_t < self.flags.len());
                    // SAFETY: Caller ensure that `JobSchedule` is correct.
                    let flag = unsafe { self.flags.get_unchecked(to_t) };
                    if flag.intersects(SystemFlags::NO_OP) {
                        pending.push(Completed {
                            index: to,
                            deferred: false,
                            successed: true, // succeed
                        });
                        continue;
                    }
                }

                // SAFETY: Must push back, ensure that the previous items remains unchanged.
                state.ready_systems.push_back(to);
            }
        }
    }

    /// Resolves ready no-op jobs immediately and propagates their completion.
    fn handle_no_op_jobs(&self, state: &mut ExecutorState) {
        // Collect indices first, then remove from back to front to keep them valid.
        let mut buffer: FastVec<usize, 5> = FastVec::new();
        let no_op_jobs = buffer.data();

        for (index, &id) in state.ready_systems.iter().enumerate() {
            let flags = unsafe { *self.flags.get_unchecked(id as usize) };
            if flags.intersects(SystemFlags::NO_OP) {
                no_op_jobs.push(index);
            }
        }

        while let Some(back) = no_op_jobs.pop() {
            let index = state.ready_systems.swap_remove_back(back);
            let signal = Completed {
                // SAFETY: reverse pop, should be correct.
                index: unsafe { index.debug_checked_unwrap() },
                successed: true,
                deferred: false,
            };
            self.handle_completed_job::<true>(state, signal);
        }
    }

    #[inline(never)]
    fn handle_deferred_jobs(&self, state: &mut ExecutorState) -> Box<dyn FnOnce() + Send + 'scope> {
        let world = self.world;
        let jobs = self.jobs;
        let panic_buffer = &self.executor.panic_buffer;
        let label = self.label;
        #[cfg(feature = "trace")]
        let span = &self.executor.sync_span;

        // Drain without reallocating by reusing the existing buffer capacity.
        let mut deferred: Vec<u16> = Vec::new();
        deferred.append(&mut state.deferred_systems);

        Box::new(move || {
            #[cfg(feature = "trace")]
            let _span = unsafe { (&mut *span.get()).as_mut().unwrap().enter() };

            let world = unsafe { world.full_mut() };
            world.flush();

            for index in deferred {
                let index = index as usize;
                debug_assert!(index < jobs.len());
                let job = unsafe { &mut *jobs.get_unchecked(index).get() };
                let func = AssertUnwindSafe(|| job.apply_deferred(world));

                if let Err(payload) = ::std::panic::catch_unwind(func) {
                    panic_buffer.preserve_payload(payload, &**job, label);
                }
            }

            world.flush();
        })
    }

    /// Tries to spawn all currently ready systems that do not conflict.
    fn spawn_ready_tasks(&self, state: &mut ExecutorState) {
        let len = state.ready_systems.len();

        for _ in 0..len {
            let Some(index) = state.ready_systems.pop_front() else {
                return;
            };

            let mut runnings = state.running_jobs.iter();
            // Use `is_conflict(index, job)` instead of `is_conflict(job, index)`,
            // The invariant `index` is `row`, variant `job` is `column`,
            // So as to ensure the cache friendly.
            let is_conflict = runnings.any(|&job| self.conflict_table.is_conflict(index, job));

            if !is_conflict {
                self.spawn_one_task(state, index);
                continue;
            }

            // Conflicts with a running job — defer until it completes.
            state.ready_systems.push_back(index);

            if !self.executor.completed.is_empty() {
                return; // Prioritize handling fresh completion signals.
            }
        }
    }

    /// Spawns one runnable system task and updates running/deferred bookkeeping.
    fn spawn_one_task(&self, state: &mut ExecutorState, index: u16) {
        let index_t = index as usize;

        // SAFETY: `ExecutorState::reset` ensure that `jobs.len == job_count`.
        let job = unsafe {
            debug_assert!(index_t < self.jobs.len());
            &mut **self.jobs.get_unchecked(index_t).get()
        };

        // SAFETY: `ExecutorState::reset` ensure that `flags.len == job_count`.
        let flags = unsafe {
            debug_assert!(index_t < self.flags.len());
            *self.flags.get_unchecked(index_t)
        };

        // SAFETY: `ExecutorState::reset` ensure that `span.len == job_count`.
        #[cfg(feature = "trace")]
        let span = unsafe {
            debug_assert!(index_t < self.spans.len());
            &mut *self.spans.get_unchecked(index_t).get()
        };

        // Reading raw flags avoids repeated virtual method calls.
        let no_op = flags.intersects(SystemFlags::NO_OP);
        let deferred = flags.intersects(SystemFlags::DEFERRED);
        let deferred = deferred & !no_op;
        // ↑ Noop + Deferred is a placeholder used to insert `ApplyDeferred/SyncPoint`.
        // ↑ Placeholder itself does not require apply_deferred.
        let non_send = flags.intersects(SystemFlags::NON_SEND);
        let exclusive = flags.intersects(SystemFlags::EXCLUSIVE);

        let need_apply_deferred = exclusive && !state.deferred_systems.is_empty();

        let apply_deferred: Option<Box<dyn FnOnce() + Send>> = if need_apply_deferred {
            Some(self.handle_deferred_jobs(state))
        } else {
            None
        };

        let context: Context<'scope, 'env, 'sys> = *self;

        let task = async move {
            if let Some(apply_deferred) = apply_deferred {
                apply_deferred();
            }

            let func = AssertUnwindSafe(|| unsafe {
                #[cfg(feature = "trace")]
                let _span = span.enter();
                if let Err(e) = job.run_raw(context.world) {
                    ::core::hint::cold_path();
                    if !matches!(e, SystemError::None) {
                        ::core::hint::cold_path();
                        let id = job.id();
                        let tick = job.last_run();
                        let ctx = ErrorContext::Job { id, tick };
                        (context.error_handler)(e.into(), ctx);
                    }
                    return false; // Error -> false
                }
                true // Success -> true
            });

            let result = ::std::panic::catch_unwind(func);

            let successed = result.unwrap_or_else(|payload| {
                context
                    .executor
                    .panic_buffer
                    .preserve_payload(payload, job, context.label);
                false
            });

            let signal = Completed {
                index,
                successed,
                deferred: successed & deferred,
            };

            context.executor.completed.push(signal);

            // Attempt to take over the scheduler.
            context.tick();
        };

        state.running_jobs.insert(index);

        if non_send {
            core::hint::cold_path();
            self.scope.spawn_to_main(task);
        } else {
            self.scope.spawn(task);
        }

        // Handle no-op jobs after spawning to keep worker execution flowing.
        if exclusive && !state.ready_systems.is_empty() {
            self.handle_no_op_jobs(state);
        }
    }
}

// -----------------------------------------------------------------------------
// JobExecutor Implementation

impl JobExecutor for MultiThreadedExecutor {
    /// Returns [`ExecutorKind::MultiThreaded`].
    ///
    /// [`ExecutorKind::MultiThreaded`]: crate::schedule::ExecutorKind::MultiThreaded
    fn kind(&self) -> ExecutorKind {
        ExecutorKind::MultiThreaded
    }

    /// Initializes internal scheduling buffers from a compiled schedule.
    ///
    /// This pre-allocates storage for dependency counters and ready queues.
    fn init(&mut self, schedule: &JobSchedule) {
        self.state.get_mut().init(schedule);
        #[cfg(feature = "trace")]
        {
            *self.sync_span.get_mut() =
                Some(zlim_log::info_span!(parent: None, "sync point", schedule = ?schedule.label));
        }
    }

    /// Executes the schedule using task-based parallel dispatch.
    ///
    /// Systems are launched when all incoming dependencies are resolved and
    /// access-conflict checks pass.
    ///
    /// Deferred systems are tracked during execution and applied at sync points:
    /// - before spawning an exclusive system when needed,
    /// - and once after the worker scope drains.
    ///
    /// Reported system errors are forwarded to `handler`.
    ///
    /// If any task panics, the panic payload is captured and rethrown after the
    /// task scope completes.
    fn exec(&mut self, schedule: &mut JobSchedule, world: &mut World, handler: ErrorHandler) {
        if schedule.nodes().is_empty() {
            return;
        }
        let label = schedule.label;

        self.state.get_mut().reset(schedule);

        MainTaskPool::get().scope(|scope| {
            Context::new(world, self, schedule, scope, handler).tick();
        });

        let jobs = schedule.jobs_mut();

        #[cfg(feature = "trace")]
        let _span = self
            .sync_span
            .get_mut()
            .as_mut()
            .expect("should initialized")
            .enter();

        for &index in &self.state.get_mut().deferred_systems {
            let index = index as usize;
            let func = AssertUnwindSafe(|| unsafe {
                debug_assert!(index < jobs.len());
                jobs.get_unchecked_mut(index).apply_deferred(world);
            });

            if let Err(payload) = ::std::panic::catch_unwind(func) {
                ::core::hint::cold_path();
                let job = &*jobs[index];
                self.panic_buffer.preserve_payload(payload, job, label);
            }
        }

        #[cfg(feature = "trace")]
        ::core::mem::drop(_span);

        if let Some(payload) = self.panic_buffer.take() {
            ::core::hint::cold_path();
            std::panic::resume_unwind(payload);
        }

        // In theory, the deferred queue should be empty at this point.
        world.flush();
    }
}

// -----------------------------------------------------------------------------
