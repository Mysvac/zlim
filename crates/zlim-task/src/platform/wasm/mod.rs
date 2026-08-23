use super::{LocalExecutor, MainExecutor};

// -----------------------------------------------------------------------------
// task_pool

mod task_pool;

pub use task_pool::{TaskPool, TaskPoolBuilder, Scope};

// -----------------------------------------------------------------------------
// block_on

/// Blocks on the supplied `future`.
/// 
/// This implementation will busy-wait until it is completed.
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
            Poll::Pending => core::hint::spin_loop(),
        }
    }
}

// -----------------------------------------------------------------------------
// tick_local

/// A no-op in WASM — exists for API parity.
///
/// In WASM, no tasks are routed through the Rust-side executors during
/// normal operation:
///
/// - [`TaskPool::spawn`], [`spawn_local`], and [`spawn_to_main`] submit
///   tasks directly to the browser's microtask queue via [`web_task`].
///
/// - [`TaskPool::scope`] drives spawned tasks internally within the
///   scope call, so no external ticking is needed.
///
/// As a result, this function has no tasks to drive and returns immediately.
///
/// [`TaskPool::spawn`]: TaskPool::spawn
/// [`spawn_local`]: TaskPool::spawn_local
/// [`spawn_to_main`]: TaskPool::spawn_to_main
/// [`TaskPool::scope`]: TaskPool::scope
#[inline(always)]
pub fn run_local() {
    /* do nothing */
}

// -----------------------------------------------------------------------------
// set_main_thread

/// Directly marks the current thread as the main thread.
///
/// On WASM this is a no-op: all tasks route to the browser event loop, so
/// there is no `MainExecutor` driver thread to set up. (The `zlim_main`
/// macro inserts this call for API parity.)
#[inline(always)]
pub fn set_main_thread() {
    /* do nothing */
}

// -----------------------------------------------------------------------------
// block_on_main

/// Send a single task to the main thread for execution and wait for the result.
/// 
/// In single threaded mode, it is equivalent to direct execution.
#[inline]
pub fn block_on_main<T, F>(future: F) -> T
where
    T: Send + 'static,
    F: Future<Output = T> + Send + 'static,
{
    use core::task::{Context, Poll};

    // Pin the future on the stack.
    let mut future = core::pin::pin!(future);
    // We don't care about the waker as we're just going to poll as fast as possible.
    let cx = &mut Context::from_waker(core::task::Waker::noop());

    // Keep polling until the future is ready.
    loop {
        match future.as_mut().poll(cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => core::hint::spin_loop(),
        }
    }
}

// -----------------------------------------------------------------------------
// Static TaskPool
// -----------------------------------------------------------------------------
//
// In WASM mode all three pool newtypes share a single global `TaskPool`.
// Tasks are submitted to the browser's microtask queue — there are no
// background threads managed by Rust.
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
/// In WASM mode this shares the same global [`TaskPool`] as
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
    /// Always returns `false` in WASM mode.
    ///
    /// Custom initialization is not supported — all three pool newtypes
    /// share a single static [`TaskPool`].
    pub fn try_init(f: impl FnOnce() -> TaskPool) -> bool {
        let _ = f;
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
/// In WASM mode this shares the same global [`TaskPool`] as
/// [`MainTaskPool`] and [`IoTaskPool`].
pub struct AsyncTaskPool;

impl core::ops::Deref for AsyncTaskPool {
    type Target = TaskPool;

    fn deref(&self) -> &Self::Target {
        &TASK_POOL
    }
}

impl AsyncTaskPool {
    /// Always returns `false` in WASM mode.
    ///
    /// Custom initialization is not supported — all three pool newtypes
    /// share a single static [`TaskPool`].
    pub fn try_init(f: impl FnOnce() -> TaskPool) -> bool {
        let _ = f;
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
/// In WASM mode this shares the same global [`TaskPool`] as
/// [`MainTaskPool`] and [`AsyncTaskPool`].
pub struct IoTaskPool;

impl core::ops::Deref for IoTaskPool {
    type Target = TaskPool;

    fn deref(&self) -> &Self::Target {
        &TASK_POOL
    }
}

impl IoTaskPool {
    /// Always returns `false` in WASM mode.
    ///
    /// Custom initialization is not supported — all three pool newtypes
    /// share a single static [`TaskPool`].
    pub fn try_init(f: impl FnOnce() -> TaskPool) -> bool {
        let _ = f;
        false
    }

    /// Returns a reference to the global [`TaskPool`].
    pub fn get() -> &'static TaskPool {
        &TASK_POOL
    }
}
