use super::{LocalExecutor, MainExecutor};

// ----------------------------------------------------------------------------
// task_pool

mod xor_shift;
mod task_pool;
mod executors;

pub use task_pool::{TaskPool, TaskPoolBuilder, Scope};

// ----------------------------------------------------------------------------
// block_on

#[cfg(feature = "async_io")]
pub use async_io::block_on;

#[cfg(not(feature = "async_io"))]
pub use futures_lite::future::block_on;

// ----------------------------------------------------------------------------
// tick_local

/// Drives local tasks to completion.
/// 
/// This function continuously ticks executors in a loop until all queued
/// tasks have been processed.
/// 
/// For multi-threaded mode, this function drives both
/// `LocalExecutor` and `MainExecutor` on the main thread.
/// Worker threads are automatically driven by the pool.
///
/// # Example
///
/// ```
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

