#![expect(unsafe_code, reason = "task spawn_unchecked is unsafe")]

use core::cell::RefCell;
use core::future::poll_fn;
use core::task::{Context, Poll, Waker};

use async_task::{Runnable, Task};
use futures_lite::FutureExt;
use zlim_utils::ext::BlockList;

// -----------------------------------------------------------------------------
// LocalExecutor

thread_local! {
    static LOCALEX: RefCell<LocalExecutor> = const {
        RefCell::new(LocalExecutor { queue: BlockList::new(), waker: None })
    };
}

/// A single-threaded executor for scheduling and running tasks on the current thread.
///
/// This executor is designed for thread-local task scheduling. It does not require `Send`
/// bounds on spawned futures, making it suitable for non-`Send` types and borrowed data.
///
/// Tasks are **not** executed immediately upon submission. They are queued and will only
/// run when the executor is actively ticked via [`tick`], [`try_tick`] or [`run`].
///
/// [`run`]: Self::run
/// [`tick`]: Self::tick
/// [`try_tick`]: Self::try_tick
///
/// Driven by `TaskPool::scope`, worker threads, or [`run_local`](crate::run_local).
///
/// # Deadlock Warning
///
/// Do not block the current thread waiting for a task's result (e.g., via `block_on`)
/// in the same thread that drives the executor. This will cause a deadlock.
pub(super) struct LocalExecutor {
    // The task queue holding pending runnable tasks.
    queue: BlockList<Runnable>,
    // The waker for the current poller (if any).
    waker: Option<Waker>,
}

impl core::fmt::Debug for LocalExecutor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("LocalExecutor")
    }
}

impl LocalExecutor {
    /// Submits a new thread-local task to the executor queue.
    ///
    /// The task will not run until the executor is ticked.
    #[inline]
    #[expect(clippy::allow_attributes, reason = "may be used")]
    #[allow(unused, reason = "maybe unused in some implementation")]
    pub fn spawn<T: 'static, F>(future: F) -> Task<T>
    where
        F: Future<Output = T> + 'static,
    {
        unsafe { Self::spawn_unchecked(future) }
    }

    /// Spawn a new local task without `'static` bounds.
    ///
    /// This function is the same as [`spawn()`](Self::spawn),
    /// except it does not require `'static` on `future`.
    ///
    /// # Safety
    ///
    /// - If `future` is not `Send`, its [`Runnable`] must be used and dropped on the original
    ///   thread. `LocalExecutor` satisfies this because the schedule function pushes to a
    ///   thread-local queue, and for `!Send` futures the `Runnable` is `!Send` and cannot leave
    ///   the thread.
    /// - If `future` is not `'static`, borrowed variables must outlive its [`Runnable`].
    /// - This function uses `async_task::spawn_unchecked` WITHOUT `propagate_panic`.
    ///   Panics from the future will propagate through `Runnable::run()` rather than through
    ///   `Task::await`, leaving the `Task` handle unresolved. Callers should wrap the future
    ///   with `catch_unwind` to prevent this.
    pub unsafe fn spawn_unchecked<T, F>(future: F) -> Task<T>
    where
        F: Future<Output = T>,
    {
        // Using a named function avoids closure allocation and reduces compilation overhead.
        fn schedule(runnable: Runnable) {
            LOCALEX.with_borrow_mut(|e| {
                e.queue.push_back(runnable);
                let _ = e.waker.take().map(Waker::wake);
            })
        }

        // SAFETY:
        // - `future` may be `!Send` or non-`'static`. The caller is responsible for ensuring
        //   the invariants documented on this function.
        // - `schedule` is a function item (Send + Sync + 'static), satisfying requirements 3
        //   and 4 of `async_task::spawn_unchecked`.
        let (runnable, task) = unsafe {
            // Note: no propagate_panic — panics in the future propagate through Runnable::run().
            // Scope callers add catch_unwind; direct spawn_local callers may get unresolved Tasks
            // on panic.
            async_task::spawn_unchecked(future, schedule)
        };

        runnable.schedule();
        task
    }

    /// Attempts to run one queued task synchronously.
    ///
    /// Returns `true` if a task was executed, `false` if the queue was empty.
    #[inline]
    pub fn try_tick() -> bool {
        match LOCALEX.with_borrow_mut(|ex| ex.queue.pop_front()) {
            Some(runnable) => {
                runnable.run();
                true
            }
            None => false,
        }
    }

    /// Waits for and runs **one** queued task asynchronously.
    ///
    /// If the queue is empty, this function waits until a task is submitted.
    pub async fn tick() {
        fn poll_tick(ctx: &mut Context<'_>) -> Poll<Runnable> {
            LOCALEX.with_borrow_mut(|ex| {
                match &mut ex.waker {
                    Some(w) => w.clone_from(ctx.waker()),
                    None => ex.waker = Some(ctx.waker().clone()),
                }
                match ex.queue.pop_front() {
                    Some(r) => Poll::Ready(r),
                    None => Poll::Pending,
                }
            })
        }

        poll_fn(poll_tick).await.run();
    }

    /// Runs the executor continuously until a stop signal is received.
    ///
    /// The executor processes queued tasks in a loop. When the `stop_signal`
    /// completes, this function returns the signal's output.
    #[expect(clippy::allow_attributes, reason = "may be used")]
    #[allow(unused, reason = "maybe unused in some implementation")]
    pub async fn run<T>(stop_signal: impl Future<Output = T>) -> T {
        let tick_forever = async {
            loop {
                LocalExecutor::tick().await;
            }
        };

        stop_signal.or(tick_forever).await
    }
}
