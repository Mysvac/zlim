#![expect(unsafe_code, reason = "lifetime transmutation")]

use core::any::Any;
use core::future::Future;
use core::marker::PhantomData;
use core::panic::{AssertUnwindSafe, RefUnwindSafe, UnwindSafe};
use std::borrow::Cow;
use std::thread::{JoinHandle, ThreadId};
use std::sync::Arc;

use event_listener::{Event, EventListener};
use futures_lite::FutureExt;
use zlim_os::thread::available_parallelism;
use zlim_utils::sync::SegQueue;
use async_task::FallibleTask;
use async_task::Task;

use super::executors::PoolExecutor;
use super::{LocalExecutor, MainExecutor, block_on};

// -----------------------------------------------------------------------------
// OnDrop

#[repr(transparent)]
struct OnDrop(Option<Arc<dyn Fn() + Send + Sync + 'static>>);

impl Drop for OnDrop {
    fn drop(&mut self) {
        if let Some(call) = self.0.as_ref() {
            call();
        }
    }
}

const MAX_THREADS: usize = 31;

// -----------------------------------------------------------------------------
// TaskPoolBuilder

/// Builder for creating a [`TaskPool`].
///
/// Currently configurable parameters:
///
/// - [`thread_count`]: Number of additional worker threads to spawn (excluding the current thread).
///   Defaults to the number of logical cores on the system.
///
/// - [`thread_name`]: Thread name prefix. If set, threads are named in the format
///   `{thread_name} ({id})`, e.g., `MyPool (1)`. Default: `TaskPool ({id})`.
///
/// - [`stack_size`]: Stack size for additional threads. Default is system-dependent.
///
/// - [`on_thread_spawn`]: Callback executed once when each thread spawns.
///
/// - [`on_thread_destroy`]: Callback executed once when each thread is about to terminate.
///
/// # Examples
///
/// ```
/// use zlim_task::TaskPoolBuilder;
/// use std::sync::atomic::{AtomicU32, Ordering};
///
/// let task_pool = TaskPoolBuilder::new()
///     .thread_count(2)
///     .thread_name("doc")
///     .build();
///
/// let result = AtomicU32::new(0);
///
/// task_pool.scope(|scope| {
///     for _ in 0..100 {
///         scope.spawn(async {
///             result.fetch_add(1, Ordering::Relaxed);
///         })
///     }
/// });
///
/// let result = result.load(Ordering::Relaxed);
/// assert_eq!(result, 100);
/// ```
///
/// [`thread_count`]: Self::thread_count
/// [`thread_name`]: Self::thread_name
/// [`stack_size`]: Self::stack_size
/// [`on_thread_spawn`]: Self::on_thread_spawn
/// [`on_thread_destroy`]: Self::on_thread_destroy
#[derive(Default)]
#[must_use]
pub struct TaskPoolBuilder {
    /// Number of threads. If `None`, uses logical core count.
    thread_count: Option<usize>,
    /// Custom stack size.
    stack_size: Option<usize>,
    /// Thread name prefix.
    thread_name: Option<Cow<'static, str>>,
    /// Called on thread spawn.
    on_thread_spawn: Option<Arc<dyn Fn() + Send + Sync + 'static>>,
    /// Called on thread termination.
    on_thread_destroy: Option<Arc<dyn Fn() + Send + Sync + 'static>>,
}

impl TaskPoolBuilder {
    /// Creates a new [`TaskPoolBuilder`].
    #[inline]
    pub const fn new() -> Self {
        Self{
            thread_count: None,
            stack_size: None,
            thread_name: None,
            on_thread_spawn: None,
            on_thread_destroy: None,
        }
    }

    /// Sets the number of threads in the pool.
    ///
    /// If unset, defaults to the system's logical core count.
    /// 
    /// The task pool should have at least `1` working thread and a maximum
    /// of `31` working threads. Exceeding or falling short will be clamped.
    #[inline]
    pub fn thread_count(mut self, thread_count: usize) -> Self {
        self.thread_count = Some(thread_count);
        self
    }

    /// Override the stack size of the threads created for the pool.
    #[inline]
    pub fn stack_size(mut self, stack_size: usize) -> Self {
        self.stack_size = Some(stack_size);
        self
    }

    /// Sets the thread name prefix.
    ///
    /// Threads will be named `<thread_name> (<thread_index>)`, e.g., `MyThreadPool (2)`.
    #[inline]
    pub fn thread_name(mut self, thread_name: impl Into<Cow<'static, str>>) -> Self {
        self.thread_name = Some(thread_name.into());
        self
    }

    /// Sets a callback invoked once per thread when it starts.
    ///
    /// Executed on the thread itself with access to thread‑local storage.
    /// Blocks async task execution on that thread until the callback completes.
    #[inline]
    pub fn on_thread_spawn(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_thread_spawn = Some(Arc::new(f));
        self
    }

    /// Sets a callback invoked once per thread when it terminates.
    ///
    /// Executed on the thread itself with access to thread‑local storage.
    /// Blocks thread termination until the callback completes.
    #[inline]
    pub fn on_thread_destroy(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_thread_destroy = Some(Arc::new(f));
        self
    }

    /// Creates a [`TaskPool`] with the configured options.
    #[inline]
    #[must_use]
    pub fn build(self) -> TaskPool {
        TaskPool::new_internal(self)
    }
}

// -----------------------------------------------------------------------------
// TaskPool

/// A thread pool for executing asynchronous tasks with work-stealing.
///
/// Manages a fixed number of worker threads and distributes tasks across
/// them using a work-stealing scheduler. At least one worker thread is
/// always spawned — explicitly setting `thread_count(0)` is coerced to 1.
///
/// ---
///
/// # Core APIs
///
/// The pool provides four primary interfaces:
///
/// | Method | Future bounds | Target | Description |
/// |--------|--------------|--------|-------------|
/// | [`spawn`] | `Send + 'static` | `PoolExecutor` (global) | Distributed across worker threads |
/// | [`spawn_local`] | `'static` only | `LocalExecutor` (current thread) | Stays on the calling thread |
/// | [`spawn_to_main`] | `Send + 'static` | `MainExecutor` (main thread) | Wakes main thread to execute |
/// | [`scope`] | non-`'static` | mixed | Scoped fork-join, collects results |
///
/// ## `spawn` APIs
///
/// [`spawn`] is the most commonly used API. Tasks are submitted to the
/// pool's `PoolExecutor` and automatically distributed across worker
/// threads via work-stealing load balancing. Returns a [`Task`] handle
/// that can be awaited, detached, or canceled — the task runs regardless
/// of whether the handle is polled.
///
/// [`spawn_local`] submits tasks to the current thread's `LocalExecutor`.
/// - On **worker threads**: tasks are automatically polled as part of the
///   worker's execution loop — no explicit ticking needed.
/// - On the **main thread**: tasks are **not** automatically polled and
///   require explicit driving via [`run_local`] or [`scope`].
///
/// [`spawn_to_main`] submits tasks to the `MainExecutor` — a global,
/// thread-safe queue. The task can be submitted from any thread, but it
/// will only execute when the main thread ticks the executor (via
/// [`run_local`] or [`scope`]). This is useful for tasks that must
/// interact with main-thread-only APIs (e.g., rendering, UI updates).
///
/// ## `scope` APIs
///
/// [`Scope::spawn`] behaves like [`TaskPool::spawn`]: tasks are submitted
/// to the global `PoolExecutor` and automatically distributed across
/// worker threads.
///
/// [`Scope::spawn_local`] forces the task onto the current thread's
/// `LocalExecutor`. Unlike the pool-level version, scope-spawned local
/// tasks are automatically driven regardless of which thread calls it.
///
/// [`Scope::spawn_to_main`] submits tasks to the `MainExecutor`,
/// analogous to [`TaskPool::spawn_to_main`].
///
/// # Executors
///
/// The pool uses three executors:
///
/// - **`PoolExecutor`** — per-pool, multi-threaded. Contains a global
///   task queue. Each worker thread maintains a local queue and can
///   steal tasks from the global queue or other workers' queues.
///
/// - **`LocalExecutor`** — thread-local storage, one per thread.
///   Stores `!Send` tasks on the owning thread.
///
/// - **`MainExecutor`** — a global, wakeable endpoint for the main
///   thread. Used by `spawn_to_main` to send tasks from any thread
///   to the main thread.
///
/// # Examples
///
/// ```
/// use zlim_task::TaskPool;
///
/// let pool = TaskPool::new();
///
/// let task = pool.spawn(async { 1 + 1 });
///
/// assert_eq!(zlim_task::block_on(task), 2);
/// ```
///
/// ```
/// use zlim_task::TaskPool;
///
/// let pool = TaskPool::new();
///
/// let mut results = pool.scope(|scope| {
///     for value in 1..=4 {
///         scope.spawn(async move { value * value });
///     }
/// });
///
/// results.sort_unstable();
/// assert_eq!(results, vec![1, 4, 9, 16]);
/// ```
///
/// [`spawn`]: Self::spawn
/// [`spawn_local`]: Self::spawn_local
/// [`spawn_to_main`]: Self::spawn_to_main
/// [`scope`]: Self::scope
/// [`run_local`]: crate::run_local
#[derive(Debug)]
pub struct TaskPool {
    /// Main Thread Id.
    thread_id: ThreadId,
    /// The executor for the pool.
    executor: PoolExecutor,
    /// Worker threads.
    threads: Box<[JoinHandle<()>]>,
    /// Shutdown signal sender.
    stop_event: Event,
}

impl Default for TaskPool {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskPool {
    /// Creates a `TaskPool` with default configuration.
    ///
    /// The worker count defaults to [`available_parallelism`] and at least `1`.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_task::TaskPool;
    /// let pool = TaskPool::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        TaskPoolBuilder::new().build()
    }

    fn new_internal(builder: TaskPoolBuilder) -> Self {
        // main thread id
        let thread_id = std::thread::current().id();

        // shutdown signal
        let stop_event = Event::new();

        // Set the number of threads based on Builder or available_parallelism.
        let thread_count = builder
            .thread_count
            .unwrap_or_else(|| available_parallelism().get())
            .clamp(1, MAX_THREADS); // At least one worker thread.
            // When a single pool has too many threads, the task scheduling
            // overhead will significantly increase. Therefore, set a maximum value.

        // PoolExecutor
        let executor = PoolExecutor::new(thread_count);

        // Create threads
        let threads: Box<[JoinHandle<()>]> = (0..thread_count)
            .map(|i| {
                // clone PoolExecutor and shutdown signal channel receiver
                let global_ex = executor.clone();
                let listener = stop_event.listen();
                // ↑ Listener created here (in `TaskPool::new`), not inside the thread.
                //
                // `Event::notify` only reaches listeners that have already been registered
                // at the moment of the call. By creating all listeners upfront, the drop
                // handler is guaranteed to see and wake every worker.

                // Set thread name
                let name = builder.thread_name.as_deref().unwrap_or("TaskPool");
                let thread_name = format!("{name} ({i})");

                let mut thread_builder = std::thread::Builder::new().name(thread_name);

                // Set thread stack size
                if let Some(stack_size) = builder.stack_size {
                    thread_builder = thread_builder.stack_size(stack_size);
                }

                let on_thread_spawn = builder.on_thread_spawn.clone();
                let on_thread_destroy = builder.on_thread_destroy.clone();

                thread_builder
                    .spawn(move || {
                        // Move Arc to closure, ensure its validity during thread execution.
                        let executor: PoolExecutor = global_ex;
                        let mut listener: EventListener = listener;
                        
                        // bind and initialize `LOCAL_WORKER`.
                        executor.bind_local_worker();


                        // Call `on_thread_spawn`
                        if let Some(on_spawn) = on_thread_spawn {
                            on_spawn();
                        }

                        // Create a drop guard, call `on_thread_destroy` automatically.
                        let _destructor = OnDrop(on_thread_destroy);

                        // Loop working
                        loop {
                            // Future's panic will be propagated to Task, we do not handle here.
                            let res = std::panic::catch_unwind(AssertUnwindSafe(|| {
                                block_on(executor.run(LocalExecutor::run(&mut listener)))
                            }));

                            // Err -> panicked
                            // Ok(()) -> TaskPool Dropped (channel closed)
                            if res.is_ok() {
                                return;
                            }
                        }
                    })
                    .expect("Failed to spawn thread.")
            })
            .collect();

        Self {
            thread_id,
            executor,
            threads,
            stop_event,
        }
    }
    
    /// Returns the number of worker threads in the pool.
    ///
    /// Does not include the thread where the task pool is located.
    #[inline]
    pub fn thread_count(&self) -> usize {
        self.threads.len()
    }

    /// Spawns a `Send + 'static` future onto the task pool.
    ///
    /// The task is submitted to the pool's `PoolExecutor`, which schedules
    /// it on an available worker thread via work-stealing.
    ///
    /// Returns a [`Task`] handle that can be awaited, canceled, or detached.
    /// The pool will execute the task regardless of whether the handle is polled.
    ///
    /// - For non‑`Send` futures, use [`TaskPool::spawn_local`].
    /// - For non‑`'static` futures, use [`TaskPool::scope`].
    ///
    /// # Deadlock Warning
    ///
    /// Do **not** block the main thread waiting for the returned [`Task`]
    /// handle if the spawned task needs to run on the main thread via
    /// [`spawn_to_main`] — the task cannot make progress while the main
    /// thread is blocked.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_task::TaskPool;
    ///
    /// let pool = TaskPool::new();
    /// let task = pool.spawn(async { 21 + 21 });
    ///
    /// assert_eq!(zlim_task::block_on(task), 42);
    /// ```
    ///
    /// [`spawn_to_main`]: Self::spawn_to_main
    #[inline]
    pub fn spawn<T, F>(&self, future: F) -> Task<T> 
    where
        T: Send + 'static,
        F: Future<Output = T> + Send + 'static,
    {
        self.executor.spawn(future)
    }

    /// Spawns a `'static` but `!Send` future onto the pool.
    ///
    /// Because the future is `!Send`, it is submitted to the current
    /// thread's `LocalExecutor`.
    ///
    /// Returns a [`Task`] handle that can be awaited, canceled, or detached.
    ///
    /// - **Worker threads**: automatically tick their `LocalExecutor`
    ///   as part of the worker loop — no manual driving needed.
    /// - **Main thread**: the `LocalExecutor` is **not** automatically
    ///   ticked. Use [`run_local`] or [`scope`] to drive it.
    ///
    /// # Deadlock Warning
    ///
    /// Do **not** block the main thread waiting for a [`Task`] from
    /// `spawn_local` — the spawned task cannot run until the current
    /// thread yields to the executor. Use [`TaskPool::scope`] to
    /// collect results synchronously.
    ///
    /// # Example
    ///
    /// ```
    /// use core::cell::Cell;
    /// use std::rc::Rc;
    /// use zlim_task::TaskPool;
    ///
    /// let pool = TaskPool::new();
    /// let value = Rc::new(Cell::new(0));
    /// let value_for_task = Rc::clone(&value);
    ///
    /// let task = pool.spawn_local(async move {
    ///     value_for_task.set(7);
    ///     value_for_task.get()
    /// });
    ///
    /// // Drive the local executor to process the task:
    /// zlim_task::run_local();
    ///
    /// assert_eq!(zlim_task::block_on(task), 7);
    /// assert_eq!(value.get(), 7);
    /// ```
    ///
    /// [`run_local`]: crate::run_local
    /// [`scope`]: Self::scope
    /// [`TaskPool::scope`]: Self::scope
    #[inline]
    pub fn spawn_local<T: 'static, F>(&self, future: F) -> Task<T>
    where
        F: Future<Output = T> + 'static,
    {
        LocalExecutor::spawn(future)
    }

    /// Spawns a `Send + 'static` future that must run on the main thread.
    ///
    /// The task is submitted to the `MainExecutor` — a global, thread-safe
    /// queue. It can be submitted from any thread, but will only execute
    /// when the main thread ticks the executor via [`run_local`] or [`scope`].
    ///
    /// This is useful for tasks that must interact with main-thread-only
    /// APIs (e.g., rendering, UI updates).
    ///
    /// # Deadlock Warning
    ///
    /// Do **not** block the main thread waiting for the returned [`Task`]
    /// handle — the spawned task cannot run until the main thread yields
    /// to the executor. Use [`TaskPool::scope`] for synchronous collection.
    ///
    /// [`run_local`]: crate::run_local
    /// [`scope`]: Self::scope
    /// [`TaskPool::scope`]: Self::scope
    #[inline]
    pub fn spawn_to_main<T, F>(&self, future: F) -> Task<T>
    where
        T: Send + 'static,
        F: Future<Output = T> + Send + 'static,
    {
        MainExecutor::spawn(future)
    }

}

impl Drop for TaskPool {
    fn drop(&mut self) {
        // Close MPMC channel, all receivers will receive the signal.
        self.stop_event.notify(usize::MAX);

        let threads = core::mem::take(&mut self.threads);
        let panicking = std::thread::panicking();

        for join_handle in threads {
            let res = join_handle.join();
            if !panicking {
                res.expect("Task thread panicked while executing.");
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Scope

type FallibleTaskQueue<T> = SegQueue<FallibleTask<Result<T, Box<dyn Any + Send>>>>;

/// A [`TaskPool`] scope for running one or more non‑`'static` futures.
///
/// All tasks spawned through a scope are driven to completion before the
/// enclosing [`TaskPool::scope`] call returns. Results are collected into
/// the returned `Vec<T>`.
#[derive(Debug)]
pub struct Scope<'sco, 'env: 'sco, T> {
    executor: &'sco PoolExecutor,
    tasks: &'sco FallibleTaskQueue<T>,
    // make `Scope` invariant over 'scope and 'env
    _marker1: PhantomData<&'sco mut &'sco ()>,
    _marker2: PhantomData<&'env mut &'env ()>,
    // Ensure the certainty of Sync, Send, etc..
    _marker3: PhantomData<*const ()>,
}

impl<T> Drop for Scope<'_, '_, T> {
    fn drop(&mut self) {
        let queue = self.tasks;
        let future = async {
            while let Some(task) = queue.pop() {
                task.cancel().await;
            }
        };
        block_on(future);
    }
}

// SAFETY: `Scope` is `Send` when `T: Send` because all tasks are driven to
// completion on the current thread before `scope` returns, so there is no
// concurrent access across threads.
unsafe impl<T: Send> Send for Scope<'_, '_, T> {}
unsafe impl<T: Send> Sync for Scope<'_, '_, T> {}
impl<T> UnwindSafe for Scope<'_, '_, T> {}
impl<T> RefUnwindSafe for Scope<'_, '_, T> {}

impl<'sco, 'env, T: Send + 'env> Scope<'sco, 'env, T> {
    /// Spawns a scoped future onto the thread pool.
    ///
    /// Submits the task to the pool's `PoolExecutor`; it may be executed
    /// on any worker thread.
    ///
    /// The future's result will be included in the vector returned by
    /// [`TaskPool::scope`].
    ///
    /// For futures that should run on the same thread, use
    /// [`Scope::spawn_local`] instead.
    pub fn spawn<F: Future<Output = T> + Send + 'sco>(&self, f: F) {
        let fut = AssertUnwindSafe(f).catch_unwind();
        // SAFETY: `spawn_unchecked` requires that `PoolExecutor` outlive the
        // returned `Task`.  This holds because:
        // - `Scope` borrows `PoolExecutor` for its entire existence,
        // - `TaskPool::scope` blocks until all spawned tasks complete
        //   (the loop after `f(scope_ref)` drains the `FallibleTask` queue),
        // - `Scope::drop` cancels any remaining tasks on panic.
        // Therefore every task from `spawn_unchecked` is resolved or
        // cancelled before `PoolExecutor` can be dropped.
        let task = unsafe { self.executor.spawn_unchecked(fut) };
        self.tasks.push(task.fallible());
    }

    /// Spawns a scoped future onto the thread where the scope is running.
    ///
    /// Submits the task to the current thread's `LocalExecutor` and
    /// actively drives it, guaranteeing execution on the current thread.
    ///
    /// The future's result will be included in the vector returned by
    /// [`TaskPool::scope`].
    ///
    /// Prefer [`Scope::spawn`] unless the future must run on the scope's
    /// thread.
    pub fn spawn_local<F: Future<Output = T> + 'sco>(&self, f: F) {
        let fut = AssertUnwindSafe(f).catch_unwind();
        // SAFETY:
        // - The future `fut` (with `catch_unwind` applied) has lifetime `'sco` (`'env`).
        //   All scope-spawned tasks are awaited or cancelled before `scope()` returns
        //   (guaranteed by the blocking loop + `Scope::drop` guard), so borrowed data
        //   outlives the Runnable.
        // - If `fut` is `!Send`, the Runnable is `!Send` and stays in the current thread's
        //   `LocalExecutor` queue (pushed by the thread-local schedule function).
        // - `catch_unwind` prevents future panics from reaching the runtime, so the missing
        //   `propagate_panic` on `LocalExecutor::spawn_unchecked` is not an issue here.
        unsafe {
            self.tasks.push(LocalExecutor::spawn_unchecked(fut).fallible());
        }
    }

    /// Spawns a scoped future onto the main thread.
    ///
    /// Submits the task to the `MainExecutor` — analogous to
    /// [`TaskPool::spawn_to_main`].  The task is sent to the main thread
    /// and the current thread **blocks** waiting for the main thread to
    /// execute it and produce a result.
    ///
    /// Deadlock is possible if the main thread never drives the
    /// `MainExecutor` (e.g. the main thread is blocked on other work
    /// and never calls [`run_local`](crate::run_local) or [`scope`]).
    /// When `scope()` is called on the main thread this is handled
    /// automatically — it ticks `MainExecutor` in its internal loop.
    /// When called from a worker thread the caller is responsible for
    /// ensuring the main thread makes progress.
    ///
    /// The future's result will be included in the vector returned by
    /// [`TaskPool::scope`].
    ///
    /// [`spawn_to_main`]: TaskPool::spawn_to_main
    /// [`scope`]: TaskPool::scope
    pub fn spawn_to_main<F: Future<Output = T> + Send + 'sco>(&self, f: F) {
        let fut = AssertUnwindSafe(f).catch_unwind();
        // SAFETY:
        // - The future `fut` (with `catch_unwind` applied) has lifetime `'sco` (`'env`).
        //   All scope-spawned tasks are completed before `scope()` returns, so borrowed
        //   data outlives the Runnable.
        // - `fut` is `Send` (required by the function signature), so the Runnable is `Send`
        //   and can be pushed to the global `SegQueue` in `MainExecutor`.
        // - `MainExecutor::spawn_unchecked` uses `propagate_panic(true)`, and `catch_unwind`
        //   additionally prevents panics from reaching the runtime.
        unsafe {
            self.tasks.push(MainExecutor::spawn_unchecked(fut).fallible());
        }
    }
}

impl TaskPool {
    /// Creates a scope for running non‑`'static` futures on the pool.
    ///
    /// Takes a closure that receives a [`Scope`] object, which can be used
    /// to spawn tasks that borrow stack-local data. This method blocks until
    /// all spawned tasks complete, then collects and returns their results.
    ///
    /// This is analogous to [`std::thread::scope`] and `rayon::scope`.
    ///
    /// Tasks spawned via [`Scope::spawn`] are distributed to worker threads;
    /// [`Scope::spawn_local`] runs on the current thread; and
    /// [`Scope::spawn_to_main`] sends tasks to the main thread.
    ///
    /// # Example
    ///
    /// ```
    /// use zlim_task::TaskPool;
    ///
    /// let pool = TaskPool::new();
    ///
    /// let values = [1_u32, 2, 3, 4];
    ///
    /// let mut results = pool.scope(|scope| {
    ///     for value in &values {
    ///         scope.spawn(async move { *value * 2 });
    ///     }
    /// });
    ///
    /// results.sort_unstable();
    /// assert_eq!(results, vec![2, 4, 6, 8]);
    /// ```
    pub fn scope<'env, F, T>(&self, f: F) -> Vec<T>
    where
        T: Send + 'static,
        F: for<'s> FnOnce(&'s Scope<'s, 'env, T>),
    {
        // SAFETY: This safety comment applies to all references transmuted to 'env.
        //
        // Any futures spawned with these references need to return before this function
        // completes. This is guaranteed because we drive all the futures spawned onto
        // the Scope to completion in this function.
        //
        // However, rust has no way of knowing this so we transmute the lifetimes to 'env
        // here to appease the compiler as it is unable to validate safety.
        //
        // Any usages of the references passed into `Scope` must be accessed through
        // the transmuted reference for the rest of this function.

        // SAFETY: As above, all futures must complete in this function so we can change the lifetime
        let executor: &PoolExecutor = &self.executor;
        let executor_ref: &'env PoolExecutor = unsafe {
            core::mem::transmute::<&PoolExecutor, &'env PoolExecutor>(executor)
        };

        // SAFETY: As above, all futures must complete in this function so we can change the lifetime
        let tasks: FallibleTaskQueue<T> = FallibleTaskQueue::new();
        let tasks_ref: &'env FallibleTaskQueue<T> = unsafe {
            core::mem::transmute::<&FallibleTaskQueue<T>, &'env FallibleTaskQueue<T>>(&tasks)
        };

        // SAFETY: As above, all futures must complete in this function so we can change the lifetime
        let scope: Scope<'_, 'env, T> = Scope {
            executor: executor_ref,
            tasks: tasks_ref,
            _marker1: PhantomData,
            _marker2: PhantomData,
            _marker3: PhantomData,
        };
        let scope_ref: &'env Scope<'_, 'env, T> = unsafe {
            core::mem::transmute::<&Scope<T>, &'env Scope<T>>(&scope)
        };

        // Spawn Tasks
        f(scope_ref);

        // No task, return directly.
        if tasks.is_empty() {
            return Vec::new();
        }

        #[cold]
        #[inline(never)]
        fn catch_panic_failed() -> ! {
            panic!("Failed to catch panic!");
        }

        let stop_signal = async {
            let mut results: Vec<T> = Vec::with_capacity(tasks.len());
            while let Some(task) = tasks.pop() {
                match task.await {
                    Some(Ok(val)) => results.push(val),
                    Some(Err(payload)) => std::panic::resume_unwind(payload),
                    None => catch_panic_failed(),
                }
            }
            results
        };

        if std::thread::current().id() == self.thread_id {
            block_on(MainExecutor::run(LocalExecutor::run(stop_signal)))
        } else {
            block_on(LocalExecutor::run(stop_signal))
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn create_task_pool() {
        let _ = TaskPool::new();
    }
}

