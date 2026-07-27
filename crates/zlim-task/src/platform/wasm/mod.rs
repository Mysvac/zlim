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
pub fn run_local() {
    /* do nothing */
}
