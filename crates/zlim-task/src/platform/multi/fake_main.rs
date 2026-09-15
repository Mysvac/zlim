use std::thread::ThreadId;
use std::sync::OnceLock;
use core::panic::AssertUnwindSafe;

use futures_lite::future::pending;

use super::{raw_block_on, MainExecutor, LocalExecutor};

/// Fake main thread.
///
/// `spawn_to_main` tasks are pushed into the global `MainExecutor`, which
/// must be driven by a *main thread*. In multi-threaded mode the library
/// therefore needs a main-thread identity, established in one of two ways:
///
/// - [`designate_main_thread`] — the application explicitly marks the current
///   thread (typically the real main thread) as the main thread *before
///   any `TaskPool` is created*. No background thread is spawned: the
///   marked thread drives the `MainExecutor` itself (e.g. via a `scope`
///   running on that thread).
/// - Otherwise, the first call to `main_thread_id()` transparently starts
///   a dedicated fake main thread (`FakeMain`) that owns the
///   `MainExecutor` waker exclusively and parks on it until the process
///   exits (or a task panics, in which case it re-enters the loop).
///
/// The fake-main path keeps the design uniform at the `TaskPool` level:
/// every `TaskPool` captures that thread as its main thread, so no `scope`
/// ever needs to drive the `MainExecutor` itself, and `spawn_to_main`
/// tasks are always executed no matter which thread created the pool.
/// Applications that want to avoid the extra thread call
/// [`designate_main_thread`] up front.
///
/// The fake main thread is deliberately never stopped: static items are
/// not dropped at program exit, so no `Drop` impl could stop it anyway.
/// It is spawned as a detached thread and reclaimed by the OS when the
/// process exits.
struct FakeMain {
    thread_id: ThreadId,
}

static FAKE_MAIN: OnceLock<FakeMain> = OnceLock::new();

// No `Drop` impl on purpose:
//
// Static items do not call `drop` at the end of the program, so a `Drop`
// impl here would never run. The thread is detached and reclaimed by the
// OS on process exit.
//
// https://doc.rust-lang.org/reference/items/static-items.html

/// Returns the ID of the main thread — the thread `spawn_to_main` tasks are
/// destined for.
///
/// If [`designate_main_thread`] was called, that thread is returned. Otherwise a
/// fake main thread is started (for environments such as tests) and its ID
/// is returned.
pub(super) fn main_thread_id() -> ThreadId {
    FAKE_MAIN.get_or_init(|| {
        ::core::hint::cold_path();

        let handle = std::thread::spawn(move || {
            // Loop working
            loop {
                // Ok(()) -> Never
                // Err -> panicked, continue
                let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    // The fake main thread is the *only* driver of the `MainExecutor`
                    // (scopes on other threads never drive it), so it owns the waker
                    // exclusively and can park on it via `run`.
                    raw_block_on(MainExecutor::run(LocalExecutor::run(pending::<()>())));
                }));
            }
        });

        let thread_id = handle.thread().id();

        FakeMain {
            thread_id,
        }
    }).thread_id
}

/// Directly marks the current thread as the main thread.
///
/// Call this **before any `TaskPool` is created** — typically the first
/// line of `main()` (the `zlim_main` macro inserts it automatically).
/// Afterwards no fake main thread is spawned: the current thread is the
/// main thread and drives the `MainExecutor` itself (via `scope` or
/// [`run_local`](crate::run_local)).
///
/// Do **not** call this in a test environment — the automatic fake main
/// thread is what keeps `spawn_to_main` tasks running under `cargo test`.
///
/// # Panics
///
/// Panics if the main-thread identity is already fixed — either because
/// `designate_main_thread` was called before, or because a `TaskPool` was already
/// created (which starts the automatic fake main thread).
///
/// On single-threaded / WASM platforms this is a no-op.
pub fn designate_main_thread() {
    let thread_id = std::thread::current().id();
    let fake_main = FakeMain { thread_id };

    let main_id = FAKE_MAIN.get_or_init(|| fake_main).thread_id;

    assert_eq!(
        main_id, thread_id,
        "`designate_main_thread()` must be called before any TaskPool is created: \
         the main thread is already fixed to {main_id:?}, but the current thread \
         is {thread_id:?}. \nIn a real application, call `designate_main_thread()` \
         once at the very start of `main()`, it's usually handled by `#[zlim_main]` macro.\
         In a test environment, do not call it — the fake main thread is started automatically.",
    );
}
