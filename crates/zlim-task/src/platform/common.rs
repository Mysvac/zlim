#![expect(unsafe_code, reason = "task spawn_unchecked is unsafe")]

use core::cell::RefCell;
use core::future::poll_fn;
use core::task::{Context, Poll, Waker};

use async_task::{Runnable, Task};
use atomic_waker::AtomicWaker;
use futures_lite::FutureExt;
use zlim_utils::ext::BlockList;
use zlim_utils::sync::SegQueue;

// ----------------------------------------------------------------------------
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
/// This executor is typically driven by a `TaskPool` that handles scheduling automatically.
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
    #[cfg(not(target_family = "wasm"))] // wasm: use `web_task` instead
    #[inline]
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
    #[cfg(not(target_family = "wasm"))] // wasm: `run_local` does nothing
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
    pub async fn run<T>(stop_signal: impl Future<Output = T>) -> T {
        let tick_forever = async {
            loop {
                LocalExecutor::tick().await;
            }
        };

        tick_forever.or(stop_signal).await
    }
}

// ----------------------------------------------------------------------------
// Main Thread Executor

static MAINEX: MainExecutor = MainExecutor {
    queue: SegQueue::new(),
    waker: AtomicWaker::new(),
};

/// A global, thread-safe executor for the main thread.
///
/// This executor can receive tasks from any thread (via `spawn`) and execute them
/// on the main thread. It uses a concurrent queue and atomic waker to handle
/// cross-thread submissions.
///
/// Tasks are **not** executed immediately upon submission. They are queued and will only
/// run when the executor is actively ticked via [`tick`], [`try_tick`] or [`run`].
///
/// [`run`]: Self::run
/// [`tick`]: Self::tick
/// [`try_tick`]: Self::try_tick
///
/// This executor is typically driven by a `TaskPool` that handles scheduling automatically.
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
    #[cfg(not(target_family = "wasm"))] // wasm: use `web_task` instead
    #[inline]
    pub fn spawn<T, F>(future: F) -> Task<T>
    where
        T: Send + 'static,
        F: Future<Output = T> + Send + 'static,
    {
        unsafe { Self::spawn_unchecked(future) }
    }

    /// Submits a new task without `Send` and `'static` bounds.
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
    /// This must be called from the main thread.
    #[cfg(not(target_family = "wasm"))] // wasm: `run_local` does nothing
    #[inline]
    pub fn try_tick() -> bool {
        #[cfg(debug_assertions)]
        assert_is_main_thread();

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
    /// This must be called from the main thread.
    pub async fn tick() {
        fn poll_tick(ctx: &mut Context<'_>) -> Poll<Runnable> {
            #[cfg(debug_assertions)]
            assert_is_main_thread();

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
    /// Processes queued tasks in a loop on the main thread. When `stop_signal`
    /// completes, this function returns the signal's output.
    ///
    /// This must be called from the main thread.
    pub async fn run<T>(stop_signal: impl Future<Output = T>) -> T {
        let tick_forever = async {
            loop {
                MainExecutor::tick().await;
            }
        };

        tick_forever.or(stop_signal).await
    }
}

#[inline]
#[track_caller]
#[cfg(debug_assertions)]
fn assert_is_main_thread() {
    // Currently only tested on the following platforms.
    #[cfg(any(
        target_os = "android",
        target_os = "windows",
        target_os = "linux",
        target_os = "macos",
    ))]
    {
        use ::std::sync::LazyLock;
        use ::std::thread::{ThreadId, current};

        static ID: LazyLock<ThreadId> = LazyLock::new(|| current().id());
        let main: ThreadId = *ID;
        let current: ThreadId = current().id();

        assert_eq! {
            main, current,
            "The MainExecutor must be ticked on the Main-Thread({:?}). Current: {:?} .",
            main, current,
        }
    }
}
