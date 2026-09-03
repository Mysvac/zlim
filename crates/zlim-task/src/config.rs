use core::fmt::{Debug, Formatter};
use std::sync::Arc;

use crate::{AsyncTaskPool, IoTaskPool, MainTaskPool, TaskPoolBuilder};

/// Defines a simple way to determine how many threads to use given the number of remaining cores
/// and number of total cores
#[derive(Clone)]
pub struct TaskPoolConfig {
    /// Force using at least this many threads
    pub min_threads: usize,
    /// Under no circumstance use more than this many threads for this pool
    pub max_threads: usize,
    /// Target using this percentage of total cores, clamped by `min_threads` and `max_threads`.
    /// It is permitted to use 1.0 to try to use all remaining threads
    pub percent: f32,
    /// Callback that is invoked once for every created thread as it starts.
    /// This configuration will be ignored under wasm platform.
    pub on_thread_spawn: Option<Arc<dyn Fn() + Send + Sync + 'static>>,
    /// Callback that is invoked once for every created thread as it terminates
    /// This configuration will be ignored under wasm platform.
    pub on_thread_destroy: Option<Arc<dyn Fn() + Send + Sync + 'static>>,
}

impl Debug for TaskPoolConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TaskPoolConfig")
            .field("min_threads", &self.min_threads)
            .field("max_threads", &self.max_threads)
            .field("percent", &self.percent)
            .finish_non_exhaustive()
    }
}

impl TaskPoolConfig {
    /// Determine the number of threads to use for this task pool
    fn get_number_of_threads(&self, remaining_threads: usize, total_threads: usize) -> usize {
        assert!(
            self.percent.is_finite() && self.percent >= 0.0,
            "`TaskPoolConfig::percent` must be a non-negative finite number",
        );

        let proportion = total_threads as f32 * self.percent;
        let mut desired = proportion as usize;

        // Equivalent to round() for positive floats without libm requirement for
        // no_std compatibility
        if proportion - desired as f32 >= 0.5 {
            desired += 1;
        }

        // Limit ourselves to the number of cores available
        desired = desired.min(remaining_threads);

        // Clamp by min_threads, max_threads. (This may result in us using more threads than are
        // available, this is intended. An example case where this might happen is a device with
        // <= 2 threads.
        desired.clamp(self.min_threads, self.max_threads)
    }
}

/// Sets up the default task pools: [`AsyncTaskPool`], [`MainTaskPool`],
/// [`IoTaskPool`].
///
/// In single-threaded mode this configuration has no effect.
///
/// In multi-threaded mode, [`apply`](Self::apply) proceeds as follows:
///
/// 1. Query the number of available threads via [`available_parallelism`],
///    minus one for the main thread.
///
/// 2. Clamp it with `min_total_threads` and `max_total_threads`, yielding
///    the "total threads" and "remaining threads" counts.
///
/// 3. Initialize the [`IoTaskPool`] from its [`TaskPoolConfig`], then
///    update the "remaining threads" count.
///
/// 4. Initialize the [`AsyncTaskPool`] from its [`TaskPoolConfig`], then
///    update the "remaining threads" count.
///
/// 5. Initialize the [`MainTaskPool`] from its [`TaskPoolConfig`].
///
/// Because the [`MainTaskPool`] is initialized last, its `percent` may be
/// set to `1.0` to claim every remaining thread.
///
/// Default configuration:
///
/// - `25%` for the [`IoTaskPool`]
/// - `25%` for the [`AsyncTaskPool`]
/// - all remaining threads for the [`MainTaskPool`]
///
/// Note that in a test environment the global pools may already have been
/// initialized by another test; in that case [`apply`](Self::apply) returns
/// early without reconfiguring them.
///
/// [`available_parallelism`]: std::thread::available_parallelism
#[derive(Debug)]
pub struct TaskPoolConfigs {
    /// If the number of physical cores is less than `min_total_threads`, force using
    /// `min_total_threads`
    pub min_total_threads: usize,
    /// If the number of physical cores is greater than `max_total_threads`, force using
    /// `max_total_threads`
    pub max_total_threads: usize,

    /// Used to determine the number of threads for the IO pool
    pub io_pool: TaskPoolConfig,
    /// Used to determine the number of threads for the async-compute pool
    pub async_pool: TaskPoolConfig,
    /// Used to determine the number of threads for the main pool
    pub main_pool: TaskPoolConfig,
}

impl Default for TaskPoolConfigs {
    fn default() -> Self {
        TaskPoolConfigs {
            // By default, use however many cores are available on the system
            min_total_threads: 1,
            max_total_threads: usize::MAX,

            // Use 25% of cores for IO, at least 1, no more than 4
            io_pool: TaskPoolConfig {
                min_threads: 1,
                max_threads: 4,
                percent: 0.25,
                on_thread_spawn: None,
                on_thread_destroy: None,
            },

            // Use 25% of cores for async compute, at least 1, no more than 4
            async_pool: TaskPoolConfig {
                min_threads: 1,
                max_threads: 4,
                percent: 0.25,
                on_thread_spawn: None,
                on_thread_destroy: None,
            },

            // Use all remaining cores for compute (at least 1)
            main_pool: TaskPoolConfig {
                min_threads: 1,
                max_threads: usize::MAX,
                percent: 1.0, // This 1.0 here means "whatever is left over"
                on_thread_spawn: None,
                on_thread_destroy: None,
            },
        }
    }
}

impl TaskPoolConfigs {
    /// Create a configuration that forces using the given number of threads.
    pub fn with_thread_count(thread_count: usize) -> Self {
        TaskPoolConfigs {
            min_total_threads: thread_count,
            max_total_threads: thread_count,
            ..Default::default()
        }
    }

    /// Initializes the global task pools ([`MainTaskPool`],
    /// [`AsyncTaskPool`], [`IoTaskPool`]) according to this configuration.
    ///
    /// In single-threaded mode this is a no-op. In multi-threaded mode, if a
    /// global pool was already initialized (e.g. by an earlier implicit
    /// access or by another test), `apply` returns early and leaves it
    /// untouched.
    pub fn apply(&mut self) {
        if crate::cfg::single_thread!() {
            zlim_log::info!("TaskPoolConfigs was ignored in single-threaded mode.");
            return;
        }

        let total_threads = zlim_os::thread::available_parallelism().get() - 1;

        let total_threads = total_threads.clamp(self.min_total_threads, self.max_total_threads);

        zlim_log::info!("Assigning {total_threads} cores to default task pools");

        let mut remaining_threads = total_threads;

        {
            // Determine the number of IO threads we will use
            let io_threads = self
                .io_pool
                .get_number_of_threads(remaining_threads, total_threads);
            remaining_threads = remaining_threads.saturating_sub(io_threads);

            let success = IoTaskPool::try_init(|| {
                let mut builder = TaskPoolBuilder::new()
                    .thread_count(io_threads)
                    .thread_name("IO Task Pool");

                if let Some(f) = self.io_pool.on_thread_spawn.take() {
                    builder = builder.on_thread_spawn(move || f());
                }
                if let Some(f) = self.io_pool.on_thread_destroy.take() {
                    builder = builder.on_thread_destroy(move || f());
                }

                builder.build()
            });

            if success {
                // If there are too many threads, it will be clamped to a maximum value.
                zlim_log::info!("IO TaskPool Threads: {io_threads}"); // so this is inaccurate
            } else {
                ::core::hint::cold_path();
                zlim_log::warn!(
                    "Static TaskPool already initialized before `TaskPoolConfigs::apply`."
                );
                return;
            }
        }
        {
            // Determine the number of async-compute threads we will use
            let async_threads = self
                .async_pool
                .get_number_of_threads(remaining_threads, total_threads);
            remaining_threads = remaining_threads.saturating_sub(async_threads);

            let success = AsyncTaskPool::try_init(|| {
                let mut builder = TaskPoolBuilder::new()
                    .thread_count(async_threads)
                    .thread_name("Async Task Pool");

                if let Some(f) = self.async_pool.on_thread_spawn.take() {
                    builder = builder.on_thread_spawn(move || f());
                }
                if let Some(f) = self.async_pool.on_thread_destroy.take() {
                    builder = builder.on_thread_destroy(move || f());
                }

                builder.build()
            });

            if success {
                zlim_log::info!("Async TaskPool Threads: {async_threads}"); // inaccurate
            } else {
                ::core::hint::cold_path();
                zlim_log::warn!(
                    "Static TaskPool already initialized before `TaskPoolConfigs::apply`."
                );
                return;
            }
        }

        {
            // Determine the number of main-pool threads we will use
            let main_threads = self
                .main_pool
                .get_number_of_threads(remaining_threads, total_threads);

            let success = MainTaskPool::try_init(|| {
                let mut builder = TaskPoolBuilder::new()
                    .thread_count(main_threads)
                    .thread_name("Main Task Pool");

                if let Some(f) = self.main_pool.on_thread_spawn.take() {
                    builder = builder.on_thread_spawn(move || f());
                }
                if let Some(f) = self.main_pool.on_thread_destroy.take() {
                    builder = builder.on_thread_destroy(move || f());
                }

                builder.build()
            });

            if success {
                zlim_log::info!("Main TaskPool Threads: {main_threads}"); // inaccurate
            } else {
                ::core::hint::cold_path();
                zlim_log::warn!(
                    "Static TaskPool already initialized before `TaskPoolConfigs::apply`."
                );
            }
        }
    }
}
