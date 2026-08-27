use super::{LocalExecutor, MainExecutor};

// -----------------------------------------------------------------------------
// task_pool

mod xor_shift;
mod task_pool;
mod executors;
mod fake_main;
mod local_deque;

pub use task_pool::{TaskPool, TaskPoolBuilder, Scope};

// -----------------------------------------------------------------------------
// tick_local

/// Drives local tasks to completion.
///
/// Ticks the thread-local `LocalExecutor` and the global `MainExecutor`
/// until both queues are empty. In multi-threaded mode, worker threads are
/// driven by the pool itself and `spawn_to_main` tasks by the main thread
/// (a dedicated fake one unless [`designate_main_thread`] was called up front) —
/// this function is for threads that are neither.
///
/// # Example
///
/// ```
/// use zlim_task::TaskPool;
///
/// let pool = TaskPool::new();
/// pool.spawn_local(async { println!("Hello from task!"); });
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

#[cfg(feature = "async_io")]
use async_io::block_on as raw_block_on;

#[cfg(not(feature = "async_io"))]
use futures_lite::future::block_on as raw_block_on;

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
pub fn block_on<T>(future: impl Future<Output = T>) -> T {
    use executors::PoolExecutor;

    if std::thread::current().id() == fake_main::main_thread_id() {
        raw_block_on(MainExecutor::run(LocalExecutor::run(future)))
    } else {
        raw_block_on(PoolExecutor::run(LocalExecutor::run(future)))
    }
}

// -----------------------------------------------------------------------------
// invoke_on_main

/// Send a single task to the main thread for execution and wait for the result.
/// 
/// If you call this function on the worker thread, it will not execute the task
/// of the worker thread itself during the waiting period. In extreme cases, this
/// may cause all threads to sleep dead.
/// 
/// In other words, this function is mainly called by the top-level intersection
/// (App) of the program. Usually only used for internal implementation of `App::run`.
#[inline]
pub fn invoke_on_main<T, F>(func: F) -> T
where
    T: Send,
    F: FnOnce() -> T + Send,
{
    use core::panic::AssertUnwindSafe;
    use executors::PoolExecutor;

    // Ensure that the fake main thread has been initialized.
    let main_id = fake_main::main_thread_id();

    if std::thread::current().id() == main_id {
        return func();
    }

    let f = AssertUnwindSafe(func);
    let fut = async move { std::panic::catch_unwind(f) };

    #[expect(unsafe_code, reason = "blocking waiting, no need 'static lifetime")]
    let task = unsafe {
        MainExecutor::spawn_unchecked(fut).fallible()
    };

    #[cold]
    #[inline(never)]
    fn catch_panic_failed() -> ! {
        panic!("Failed to catch panic!");
    }

    let stop_signal = async move {
        match task.await {
            None => catch_panic_failed(),
            Some(Ok(val)) => val,
            Some(Err(payload)) => {
                ::core::hint::cold_path();
                std::panic::resume_unwind(payload)
            },
        }
    };

    raw_block_on(PoolExecutor::run(LocalExecutor::run(stop_signal)))
}


// -----------------------------------------------------------------------------
// designate_main_thread

pub use fake_main::designate_main_thread;

// -----------------------------------------------------------------------------
// Static TaskPool
// -----------------------------------------------------------------------------
//
// Three global singleton pools backed by `OnceLock<TaskPool>`, each serving a
// different workload category:
//
// | Pool            | Purpose                                     | Default threads      |
// |-----------------|---------------------------------------------|----------------------|
// | `MainTaskPool`  | Backend for parallel algorithms, single-frame compute | 50% of available |
// | `AsyncTaskPool` | Compute-intensive tasks spanning multiple frames      | 25% of available |
// | `IoTaskPool`    | IO-bound tasks with potentially long waits            | 25% of available |
//
// Each pool is lazily initialized on first access. Call `try_init` before the
// first `get()` to supply a custom configuration; it returns `true` on success
// (pool was uninitialized) or `false` if already initialized. In multi-threaded
// mode racing concurrent calls may observe stale results — call `try_init` once
// during app startup, before the first `get()`.

// -----------------------------------------------------------------------------
// Default constructors

/// Default constructor for [`MainTaskPool`].
///
/// Uses **50%** of available parallelism (at least 1, at most 15 threads).
#[cold]
#[inline(never)]
fn main_pool_default() -> TaskPool {
    let available: usize = zlim_os::thread::available_parallelism().get();
    let threads: usize = (available >> 1).clamp(1, 15);
    zlim_log::info!("Main TaskPool Threads: {threads}");
    TaskPoolBuilder::new().thread_name("MainTaskPool").thread_count(threads).build()
}

/// Default constructor for [`AsyncTaskPool`].
///
/// Uses **25%** of available parallelism (at least 1, at most 7 threads).
#[cold]
#[inline(never)]
fn async_pool_default() -> TaskPool {
    let available: usize = zlim_os::thread::available_parallelism().get();
    let threads: usize = (available >> 2).clamp(1, 7);
    zlim_log::info!("Async TaskPool Threads: {threads}");
    TaskPoolBuilder::new().thread_name("AsyncTaskPool").thread_count(threads).build()
}

/// Default constructor for [`IoTaskPool`].
///
/// Uses **25%** of available parallelism (at least 1, at most 7 threads).
#[cold]
#[inline(never)]
fn io_pool_default() -> TaskPool {
    let available: usize = zlim_os::thread::available_parallelism().get();
    let threads: usize = (available >> 2).clamp(1, 7);
    zlim_log::info!("IO TaskPool Threads: {threads}");
    TaskPoolBuilder::new().thread_name("IoTaskPool").thread_count(threads).build()
}

// -----------------------------------------------------------------------------
// Storage

static MAIN_TASK_POOL: std::sync::OnceLock<TaskPool> = std::sync::OnceLock::new();

static ASYNC_TASK_POOL: std::sync::OnceLock<TaskPool> = std::sync::OnceLock::new();

static IO_TASK_POOL: std::sync::OnceLock<TaskPool> = std::sync::OnceLock::new();

// -----------------------------------------------------------------------------
// MainTaskPool

/// The primary task pool for parallel algorithms and single-frame compute.
///
/// Note: "Main" here refers to this being the *primary* task pool,
/// not that it only runs on the main thread.
///
/// # Default configuration
///
/// When initialized implicitly (via [`get`] or [`Deref`]), uses **50%** of
/// available parallelism (at least 1, at most 15 threads).
///
/// # Custom initialization
///
/// Call [`try_init`] before the first access to supply a custom [`TaskPool`].
/// Returns `true` on success (pool was not yet initialized), `false` if the
/// pool had already been initialized. Racing concurrent calls may observe
/// stale results — call once during app startup, before the first `get()`.
///
/// # Alternatives
///
/// For work spanning multiple frames, use [`AsyncTaskPool`].
/// For IO-bound work, use [`IoTaskPool`].
///
/// [`get`]: Self::get
/// [`Deref`]: core::ops::Deref
/// [`try_init`]: Self::try_init
pub struct MainTaskPool;

impl core::ops::Deref for MainTaskPool {
    type Target = TaskPool;

    fn deref(&self) -> &Self::Target {
        MAIN_TASK_POOL.get_or_init(main_pool_default)
    }
}

impl MainTaskPool {
    /// Attempts to initialize the pool with a custom [`TaskPool`].
    ///
    /// Returns `true` if the pool was not yet initialized and `f` was used,
    /// or `false` if the pool had already been initialized (in which case
    /// `f` is still called, but its result is discarded).
    ///
    /// **Concurrency note:** racing calls from multiple threads may observe
    /// stale results. Call once during app startup, before the first `get()`.
    pub fn try_init(f: impl FnOnce() -> TaskPool) -> bool {
        let ret = MAIN_TASK_POOL.get().is_none();
        let _ = MAIN_TASK_POOL.get_or_init(f);
        ret
    }

    /// Returns a reference to the global [`TaskPool`].
    ///
    /// If the pool has not been initialized yet (neither via [`try_init`] nor
    /// a prior `get`), it is implicitly created with the default configuration
    /// (50% of available parallelism).
    ///
    /// [`try_init`]: Self::try_init
    pub fn get() -> &'static TaskPool {
        MAIN_TASK_POOL.get_or_init(main_pool_default)
    }
}

// -----------------------------------------------------------------------------
// AsyncTaskPool

/// A task pool for *async* CPU-intensive work that may span multiple frames.
///
/// # Default configuration
///
/// When initialized implicitly (via [`get`] or [`Deref`]), uses **25%** of
/// available parallelism (at least 1, at most 5 threads).
///
/// # Custom initialization
///
/// Call [`try_init`] before the first access to supply a custom [`TaskPool`].
///
/// [`get`]: Self::get
/// [`Deref`]: core::ops::Deref
/// [`try_init`]: Self::try_init
pub struct AsyncTaskPool;

impl core::ops::Deref for AsyncTaskPool {
    type Target = TaskPool;

    fn deref(&self) -> &Self::Target {
        ASYNC_TASK_POOL.get_or_init(async_pool_default)
    }
}

impl AsyncTaskPool {
    /// Attempts to initialize the pool with a custom [`TaskPool`].
    ///
    /// Returns `true` if the pool was not yet initialized and `f` was used,
    /// or `false` if already initialized.
    ///
    /// **Concurrency note:** call once during app startup, before the first `get()`.
    pub fn try_init(f: impl FnOnce() -> TaskPool) -> bool {
        let ret = ASYNC_TASK_POOL.get().is_none();
        let _ = ASYNC_TASK_POOL.get_or_init(f);
        ret
    }

    /// Returns a reference to the global [`TaskPool`].
    ///
    /// Implicitly initializes with the default configuration (25% of available
    /// parallelism) on first access.
    pub fn get() -> &'static TaskPool {
        ASYNC_TASK_POOL.get_or_init(async_pool_default)
    }
}

// -----------------------------------------------------------------------------
// IoTaskPool

/// A task pool for IO-intensive work with potentially long waits.
///
/// # Default configuration
///
/// When initialized implicitly (via [`get`] or [`Deref`]), uses **25%** of
/// available parallelism (at least 1, at most 5 threads).
///
/// # Custom initialization
///
/// Call [`try_init`] before the first access to supply a custom [`TaskPool`].
///
/// [`get`]: Self::get
/// [`Deref`]: core::ops::Deref
/// [`try_init`]: Self::try_init
pub struct IoTaskPool;

impl core::ops::Deref for IoTaskPool {
    type Target = TaskPool;

    fn deref(&self) -> &Self::Target {
        IO_TASK_POOL.get_or_init(io_pool_default)
    }
}

impl IoTaskPool {
    /// Attempts to initialize the pool with a custom [`TaskPool`].
    ///
    /// Returns `true` if the pool was not yet initialized and `f` was used,
    /// or `false` if already initialized.
    ///
    /// **Concurrency note:** call once during app startup, before the first `get()`.
    pub fn try_init(f: impl FnOnce() -> TaskPool) -> bool {
        let ret = IO_TASK_POOL.get().is_none();
        let _ = IO_TASK_POOL.get_or_init(f);
        ret
    }

    /// Returns a reference to the global [`TaskPool`].
    ///
    /// Implicitly initializes with the default configuration (25% of available
    /// parallelism) on first access.
    pub fn get() -> &'static TaskPool {
        IO_TASK_POOL.get_or_init(io_pool_default)
    }
}

