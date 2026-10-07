//! The main-thread executor.
//!
//! Multi-threaded mode is the only mode with a thread that is *the* main thread: workers
//! are pool-owned, and everything that has to run on the main thread is handed to this
//! executor and driven there — by the fake main thread, or by the thread that called
//! [`designate_main_thread`](crate::designate_main_thread).
//!
//! In single-threaded mode and on WASM this executor does not exist: there is no separate
//! main thread to reach, so `spawn_to_main` is `spawn_local` and a task stays on the thread
//! that queued it. See the `single` and `wasm` scopes.

#![expect(unsafe_code, reason = "the executor spawns tasks without bounds")]

use core::future::poll_fn;
use core::task::{Context, Poll};

use async_task::{Runnable, Task};
use atomic_waker::AtomicWaker;
use futures_lite::FutureExt;
use zlim_utils::sync::SegQueue;

// -----------------------------------------------------------------------------
// MainExecutor

static MAINEX: MainExecutor = MainExecutor {
    queue: SegQueue::new(),
    waker: AtomicWaker::new(),
};

/// A global, thread-safe executor for the main thread.
///
/// This executor can receive tasks from any thread (via `spawn`) and execute them
/// on the thread that drives it. That is the main thread — a dedicated fake one
/// unless [`designate_main_thread`](crate::designate_main_thread) was called up front —
/// and applications hand main-thread work to it via `spawn_to_main`. It uses a
/// concurrent queue and atomic waker to handle cross-thread submissions.
///
/// Tasks are **not** executed immediately upon submission. They are queued and will only
/// run once the main thread ticks the executor.
///
/// # Deadlock Warning
///
/// Do not block the **main** thread waiting for a task's result.
///
/// Otherwise, due to the main thread being blocked and no one executing
/// the main thread tasks, a deadlock may occur.
///
/// # Panic Propagation
///
/// Panics from spawned tasks are propagated to the caller via the `Task` future.
pub(super) struct MainExecutor {
    // Thread-safe MPSC queue for cross-thread task submission.
    queue: SegQueue<Runnable>,
    // Atomic waker used to wake the main thread's ticker.
    waker: AtomicWaker,
}

impl core::fmt::Debug for MainExecutor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("MainExecutor")
    }
}

impl MainExecutor {
    /// Submits a new task to be executed on the main thread.
    ///
    /// This function is thread-safe and can be called from any thread. The future
    /// must be `Send` and `'static`.
    ///
    /// The task will not run until the main thread ticks the executor.
    #[inline]
    pub fn spawn<T, F>(future: F) -> Task<T>
    where
        T: Send + 'static,
        F: Future<Output = T> + Send + 'static,
    {
        unsafe { Self::spawn_unchecked(future) }
    }

    /// Submits a task without `Send` and `'static` bounds.
    ///
    /// This function is the same as [`spawn()`](Self::spawn), except it does not require
    /// [`Send`] and `'static` on `future`.
    ///
    /// # Safety
    ///
    /// - If `future` is not `'static`, borrowed variables must outlive its [`Runnable`].
    /// - If `future` is not `Send`, its [`Runnable`] is `!Send` and cannot be pushed into the
    ///   global `SegQueue` (which requires `Send`). In practice, the compiler will reject
    ///   `!Send` futures at the `MAINEX.queue.push(runnable)` call site. The caller must
    ///   ensure the future is `Send`.
    pub unsafe fn spawn_unchecked<T, F>(future: F) -> Task<T>
    where
        F: Future<Output = T>,
    {
        fn schedule(runnable: Runnable) {
            MAINEX.queue.push(runnable);
            MAINEX.waker.wake();
        }

        // SAFETY:
        // - `Schedule` is `Send` and `Sync` and `'static`.
        // - If `Fut` is not `'static`, borrowed variables must outlive its `Runnable`.
        // - If `Fut` is not `Send`, its `Runnable` must be used and dropped on the original thread.
        let (runnable, task) = unsafe {
            async_task::Builder::new()
                .propagate_panic(true)
                .spawn_unchecked(|()| future, schedule)
        };

        runnable.schedule();
        task
    }

    /// Attempts to run one queued task synchronously.
    ///
    /// Returns `true` if a task was executed, `false` if the queue was empty.
    ///
    /// Thread-safe: may be called from any thread, e.g. by
    /// [`run_local`](crate::run_local) or a scope.
    #[inline]
    pub fn try_tick() -> bool {
        match MAINEX.queue.pop() {
            Some(runnable) => {
                runnable.run();
                true
            }
            None => false,
        }
    }

    /// Waits for and runs one queued task asynchronously.
    ///
    /// If the queue is empty, this function registers the current waker and waits
    /// until a task is submitted from any thread.
    ///
    /// The thread that drives this executor executes the dequeued tasks on
    /// itself: the dedicated fake main thread started by the `TaskPool`
    /// machinery, or the thread marked by
    /// [`designate_main_thread`](crate::designate_main_thread).
    async fn tick() {
        fn poll_tick(ctx: &mut Context<'_>) -> Poll<Runnable> {
            MAINEX.waker.register(ctx.waker());
            match MAINEX.queue.pop() {
                Some(r) => Poll::Ready(r),
                None => Poll::Pending,
            }
        }

        poll_fn(poll_tick).await.run();
    }

    /// Runs the executor continuously until a stop signal is received.
    ///
    /// Processes queued tasks in a loop on the current thread. When `stop_signal`
    /// completes, this function returns the signal's output.
    ///
    /// Driven by the dedicated fake main thread started by the `TaskPool`
    /// machinery, or by the thread marked with
    /// [`designate_main_thread`](crate::designate_main_thread).
    pub async fn run<T>(stop_signal: impl Future<Output = T>) -> T {
        let tick_forever = async {
            loop {
                MainExecutor::tick().await;
            }
        };

        stop_signal.or(tick_forever).await
    }
}
