use super::{LocalExecutor, MainExecutor};

// -----------------------------------------------------------------------------
// task_pool

mod task_pool;

pub use task_pool::{Scope, TaskPool, TaskPoolBuilder};

// -----------------------------------------------------------------------------
// tick_local

/// Drives local tasks to completion.
///
/// This function continuously ticks executors in a loop
/// until all queued tasks have been processed.
///
/// For single threaded mode, this function drives both
/// `LocalExecutor` and `MainExecutor`.
///
/// # Example
///
/// ```no_run
/// use zlim_task::TaskPool;
///
/// let pool = TaskPool::new();
/// pool.spawn(async { println!("Hello from task!"); });
/// zlim_task::run_local(); // drive the task to completion
/// ```
pub fn run_local() {
    let mut has_task: bool = true;

    while has_task {
        has_task = false;
        has_task |= MainExecutor::try_tick();
        has_task |= LocalExecutor::try_tick();
    }
}

// -----------------------------------------------------------------------------
// block_on

/// Blocks the current thread on a future.
///
/// Simultaneously drive async executors to avoid deadlock.
/// 
/// # Examples
/// 
/// ```
/// use zlim_task::block_on;
/// 
/// let val = block_on(async { 1 + 2 });
/// 
/// assert_eq!(val, 3);
/// ```
#[inline]
pub fn block_on<T>(future: impl Future<Output = T>) -> T {
    use core::task::{Context, Poll};

    // Pin the future on the stack.
    let mut future = core::pin::pin!(future);
    // We don't care about the waker as we're just going to poll as fast as possible.
    let cx = &mut Context::from_waker(core::task::Waker::noop());

    // Keep polling until the future is ready.
    loop {
        match future.as_mut().poll(cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => run_local(),
        }
    }
}

// -----------------------------------------------------------------------------
// invoke_on_main

/// Send a single task to the main thread for execution and wait for the result.
///
/// In single threaded mode, it is equivalent to direct execution.
#[inline(always)]
pub fn invoke_on_main<T, F>(func: F) -> T
where
    T: Send,
    F: FnOnce() -> T + Send,
{
    func()
}

// -----------------------------------------------------------------------------
// designate_main_thread

/// Directly marks the current thread as the main thread.
///
/// In single-threaded mode this is a no-op: there is no separate
/// `MainExecutor` driver thread to avoid, so there is nothing to set up.
/// (The `zlim_main` macro inserts this call for API parity.)
#[inline(always)]
pub fn designate_main_thread() {
    /* do nothing */
}

// -----------------------------------------------------------------------------
// Static TaskPool
// -----------------------------------------------------------------------------
//
// In single-threaded mode all three pool newtypes share a single global
// `TaskPool`. There are no background threads — all tasks execute on the
// current thread when the executor is driven (via `run_local` or `scope`).
//
// Custom initialization via `try_init` is not supported: it always returns
// `false` because the static pool is baked into the binary.

// -----------------------------------------------------------------------------
// Storage

static TASK_POOL: TaskPool = TaskPool(());

// -----------------------------------------------------------------------------
// MainTaskPool

/// The primary task pool for parallel algorithms and single-frame compute.
///
/// Note: "Main" here refers to this being the *primary* task pool,
/// not that it only runs on the main thread.
///
/// In single-threaded mode this shares the same global [`TaskPool`] as
/// [`AsyncTaskPool`] and [`IoTaskPool`]. Custom initialization via
/// [`try_init`] is not supported — it always returns `false`.
///
/// [`try_init`]: Self::try_init
pub struct MainTaskPool;

impl core::ops::Deref for MainTaskPool {
    type Target = TaskPool;

    fn deref(&self) -> &Self::Target {
        &TASK_POOL
    }
}

impl MainTaskPool {
    /// Always returns `false` in single-threaded mode.
    ///
    /// Custom initialization is not supported — all three pool newtypes
    /// share a single static [`TaskPool`].
    pub fn try_init(_f: impl FnOnce() -> TaskPool) -> bool {
        false
    }

    /// Returns a reference to the global [`TaskPool`].
    pub fn get() -> &'static TaskPool {
        &TASK_POOL
    }
}

// -----------------------------------------------------------------------------
// AsyncTaskPool

/// A task pool for *async* CPU-intensive work that may span multiple frames.
///
/// In single-threaded mode this shares the same global [`TaskPool`] as
/// [`MainTaskPool`] and [`IoTaskPool`].
pub struct AsyncTaskPool;

impl core::ops::Deref for AsyncTaskPool {
    type Target = TaskPool;

    fn deref(&self) -> &Self::Target {
        &TASK_POOL
    }
}

impl AsyncTaskPool {
    /// Always returns `false` in single-threaded mode.
    ///
    /// Custom initialization is not supported — all three pool newtypes
    /// share a single static [`TaskPool`].
    pub fn try_init(_f: impl FnOnce() -> TaskPool) -> bool {
        false
    }

    /// Returns a reference to the global [`TaskPool`].
    pub fn get() -> &'static TaskPool {
        &TASK_POOL
    }
}

// -----------------------------------------------------------------------------
// IoTaskPool

/// A task pool for IO-intensive work with potentially long waits.
///
/// In single-threaded mode this shares the same global [`TaskPool`] as
/// [`MainTaskPool`] and [`AsyncTaskPool`].
pub struct IoTaskPool;

impl core::ops::Deref for IoTaskPool {
    type Target = TaskPool;

    fn deref(&self) -> &Self::Target {
        &TASK_POOL
    }
}

impl IoTaskPool {
    /// Always returns `false` in single-threaded mode.
    ///
    /// Custom initialization is not supported — all three pool newtypes
    /// share a single static [`TaskPool`].
    pub fn try_init(_f: impl FnOnce() -> TaskPool) -> bool {
        false
    }

    /// Returns a reference to the global [`TaskPool`].
    pub fn get() -> &'static TaskPool {
        &TASK_POOL
    }
}
