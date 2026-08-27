use std::thread::{ThreadId, JoinHandle};
use event_listener::{Event, EventListener};
use std::sync::OnceLock;
use core::panic::AssertUnwindSafe;

use super::{raw_block_on, MainExecutor, LocalExecutor};

/// Fake main thread
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
///   `MainExecutor` waker exclusively and keeps polling it until the
///   process exits.
///
/// The fake-main path keeps the design uniform at the `TaskPool` level:
/// every `TaskPool` captures that thread as its main thread, so no `scope`
/// ever needs to drive the `MainExecutor` itself, and `spawn_to_main`
/// tasks are always executed no matter which thread created the pool.
/// Applications that want to avoid the extra thread call
/// [`designate_main_thread`] up front.
struct FakeMain {
    thread_id: ThreadId,
    handle: Option<JoinHandle<()>>,
    stop_event: Option<Event<()>>,
}

impl Drop for FakeMain {
    fn drop(&mut self) {
        if let Some(event) = self.stop_event.take() {
            event.notify(usize::MAX);
        }

        if let Some(handle) = self.handle.take() {
            let panicking = std::thread::panicking();
            let x = handle.join();
            assert!(panicking || x.is_ok());
        }
    }
}

static FAKE_MAIN: OnceLock<FakeMain> = OnceLock::new();

/// Returns the ID of the main thread — the thread `spawn_to_main` tasks are
/// destined for.
///
/// If [`designate_main_thread`] was called, that thread is returned. Otherwise a
/// fake main thread is started (for environments such as tests) and its ID
/// is returned.
pub(super) fn main_thread_id() -> ThreadId {
    FAKE_MAIN.get_or_init(|| {
        ::core::hint::cold_path();
        
        // shutdown signal
        let stop_event = Event::new();
        let listener = stop_event.listen();

        let handle = std::thread::spawn(move || {
            let mut listener: EventListener = listener;

            // Loop working
            loop {
                // The fake main thread is the *only* driver of the
                // `MainExecutor` (scopes on other threads never drive it),
                // so it owns the waker exclusively and can park on it via
                // `run`.
                let func = || raw_block_on(MainExecutor::run(LocalExecutor::run(&mut listener)));

                // Err -> panicked
                // Ok(()) -> FakeMain dropped (channel closed)
                if std::panic::catch_unwind(AssertUnwindSafe(func)).is_ok() {
                    return;
                }
            }
        });

        let thread_id = handle.thread().id();

        FakeMain {
            thread_id,
            handle: Some(handle),
            stop_event: Some(stop_event),
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
    let fake_main = FakeMain { thread_id, handle: None, stop_event: None };

    let main_id = FAKE_MAIN.get_or_init(|| fake_main).thread_id;

    assert_eq!(
        main_id, thread_id,
        "`designate_main_thread()` must be called before any TaskPool is created: \
         the main thread is already fixed to {main_id:?}, but the current \
         thread is {thread_id:?}. \nIn a real application, call `designate_main_thread()` \
         once at the very start of `main()`, it's usually handled by `#[zlim_main]` macro.\
         In a test environment, do not call it — the fake main thread is started automatically.",
    );
}
