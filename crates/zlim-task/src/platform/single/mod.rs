use super::{LocalExecutor, MainExecutor};

// ----------------------------------------------------------------------------
// task_pool

mod task_pool;

pub use task_pool::{TaskPool, TaskPoolBuilder, Scope};

// ----------------------------------------------------------------------------
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

// ----------------------------------------------------------------------------
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
