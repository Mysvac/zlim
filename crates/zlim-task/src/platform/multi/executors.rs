//! This module provides the implementation of [`PoolExecutor`],
//! which is used exclusively in multi-threaded mode.
//!
//! [`PoolExecutor`] is a per-pool executor with work-stealing
//! capabilities. Each [`TaskPool`](crate::TaskPool) owns one instance.
//! Worker threads maintain local queues and can steal tasks from
//! the global queue or from each other to balance load.
#![expect(unsafe_code, reason = "original implementation")]

use core::cell::Cell;
use core::ptr;
use core::task::Waker;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;
use core::future::poll_fn;
use core::fmt::Debug;
use core::task::Poll;
use std::sync::{Weak, Arc, Mutex, PoisonError};

use async_task::Runnable;
use async_task::Task;
use futures_lite::FutureExt;
use zlim_utils::sync::{ListQueue, ArrayQueue};
use zlim_utils::ext::{ArrayDeque, CachePadded};

use super::LocalExecutor;
use super::xor_shift::XorShift64Star;

// -----------------------------------------------------------------------------
// Config

/// Capacity of each worker's local task queue.
/// 
/// Using 63 ensures the bounded `ArrayQueue` allocates exactly 64 slots
/// (`(x + 1).next_power_of_two()`). This balance provides good throughput
/// while keeping cache footprint reasonable.
const WORKER_QUEUE_SIZE: usize = 63;

// -----------------------------------------------------------------------------
// State

/// The internal, shared state of the executor.
struct State {
    /// Shared global queue
    queue: ListQueue<Runnable>,
    /// “Seats” for worker threads;
    /// length equals the number of workers(without main thread).
    seats: CachePadded<Box<[Seat]>>,
    /// Manages sleeping workers and stores their wakers;
    /// length equals the number of workers(without main thread).
    lounge: CachePadded<Mutex<Lounge>>,
    /// Indicates whether a worker is currently being woken up.
    /// This flag ensures workers are woken one by one, preventing thundering herd.
    ///
    /// Note: it is also considered `true` when all workers are already active
    /// (sleeping_num == 0 in the lounge). Initially `false` because no wakeup
    /// is in progress; the first wake_one() CAS will set it, and after that
    /// the lounge state drives the flag through sleep/wake transitions.
    is_waking: AtomicBool,
}

// -----------------------------------------------------------------------------
// Seat

/// A "seat" representing a worker thread's position in the executor.
/// 
/// Note: worker threads does not include main thread.
/// 
/// Each seat contains:
/// - A local task queue for cache-efficient task processing
/// - An occupancy flag for thread binding during initialization
struct Seat {
    /// Local, bounded task queue for this worker
    /// Uses `ArrayQueue` for lock-free push/pop operations
    queue: ArrayQueue<Runnable>,
    /// Indicates whether this seat is occupied by a bound worker
    /// Set during worker initialization via atomic compare-and-swap
    occupied: AtomicBool,
}

// -----------------------------------------------------------------------------
// Lounge

/// Manages sleeping workers and stores their wakers.
/// 
/// A worker can be in one of three states:
/// 
/// - **Working**
/// - **Waking** (transitioning from sleeping to working)
/// - **Sleeping**
///
/// When a **Working** worker fails to obtain a runnable, it
/// transitions to **Sleeping** and tries again. If it fails again,
/// it returns `Pending` and sleeps the thread.
///
/// When a sleeping worker is woken, it becomes **Waking**.
/// If a runnable is obtained, it becomes **Working**;
/// otherwise it returns to **Sleeping** and tries again.
/// If it fails again, it returns `Pending` and sleeps the thread.
struct Lounge {
    /// Number of workers currently sleeping (with registered wakers).
    sleeping_num: usize,
    
    /// Number of workers in waking state (transitioning from sleep).
    waking_count: usize,

    // /// Number of workers currently working. Useless.
    // working_num: usize,

    /// Mark the location of the first Waker index.
    /// May point to a `None` slot — callers should tolerate stale values.
    cached_index: usize,
    
    /// Optional wakers for each worker seat.
    /// 
    /// - `None` indicates worker is working or waking
    /// - `Some(waker)` indicates worker is sleeping
    wakers: Box<[Option<Waker>]>,
}

// -----------------------------------------------------------------------------
// PoolExecutor

/// A pool level executor with work-stealing capabilities.
/// 
/// Each task pool will have its own dedicated `PoolExecutor`,
/// rather than sharing a single global instance.
/// 
/// Every `PoolExecutor` maintains an internal task queue for
/// distributing tasks across multiple threads.
/// 
/// Each thread will have a `Worker` instance, which is bound to
/// the current task pool's `PoolExecutor` when the thread is
/// created by the pool. The `Worker` then cooperates with a
/// `LocalExecutor` to run the asynchronous execution loop.
/// 
/// A `Worker` is a thread-local executor with work-stealing capabilities.
/// It steals tasks from its bound `PoolExecutor` into its local queue
/// for execution. When both the local and global queues are empty,
/// it will also attempt to steal tasks from other threads' `Worker`
/// instances to balance workloads.
#[derive(Clone)]
pub(super) struct PoolExecutor {
    state: Arc<State>,
}

impl Debug for PoolExecutor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PoolExecutor")
            .field("workers", &self.state.seats.len())
            .finish()
    }
}

// -----------------------------------------------------------------------------
// Worker

/// Async task executor residing in a worker thread,
/// responsible for executing tasks and work‑stealing.
/// 
/// Stored in thread‑local storage; each thread has one
/// instance.
/// 
/// Its fields are initialized when the `TaskPool` creates
/// a thread by calling `bind_local_worker`.
/// 
/// It holds a pointer to the executor's [`State`].
struct Worker {
    /// Fast random number generator for random work‑stealing.
    xor_shift: XorShift64Star,
    /// Pointer to the executor's shared [`State`]
    state: Cell<*const State>,
    /// Pointer to the thread’s local task queue
    queue: Cell<*const ArrayQueue<Runnable>>,
    /// Index of this worker’s seat in the executor
    seat_index: Cell<usize>,
    /// Current activity state of the worker
    /// 
    /// State transitions:
    /// - true → false: Working → Sleeping (when no tasks available)
    /// - false → true: Sleeping/Waking → Working (when task obtained)
    working: Cell<bool>,
}

thread_local! {
    // `const {}` enable a more efficient thread local implementation.
    static WORKER: Worker = const {
        Worker {
            xor_shift: XorShift64Star::fixed(),
            state: Cell::new(ptr::null()),
            queue: Cell::new(ptr::null()),
            seat_index: Cell::new(0),
            working: Cell::new(true),
        }
    };
}

// -----------------------------------------------------------------------------
// Worker

impl Lounge {
    /// Working → Sleeping
    fn insert(&mut self, id: usize, waker: &Waker) {
        debug_assert!(id < self.wakers.len());

        // SAFETY: `id` is the worker's seat index (assigned from
        // `0..seats.len()` during `bind_local_worker`).  `wakers.len()`
        // equals `seats.len()`, so `id` is always in bounds.
        unsafe {
            // ↓ The Worker is Working, its waker should be `None`.
            debug_assert!(self.wakers.get_unchecked(id).is_none());
            *self.wakers.get_unchecked_mut(id) = Some(waker.clone());
        }

        self.sleeping_num += 1;
        self.cached_index = self.cached_index.min(id);
    }

    /// Waking/Sleeping → Sleeping
    /// 
    /// - Returns `true` if the state changed from Waking to Sleeping,
    /// - Returns `false` if the worker was already Sleeping.
    fn try_insert(&mut self, id: usize, waker: &Waker) -> bool {
        debug_assert!(id < self.wakers.len());

        // SAFETY: `id` is the worker's seat index (assigned from
        // `0..seats.len()` during `bind_local_worker`).  `wakers.len()`
        // equals `seats.len()`, so `id` is always in bounds.
        let old = unsafe { self.wakers.get_unchecked_mut(id) };
        match old {
            Some(w) => {
                // Sleeping → Sleeping
                w.clone_from(waker);
                false
            },
            None => {
                // Waking → Sleeping
                *old = Some(waker.clone());
                self.waking_count -= 1;
                self.sleeping_num += 1;
                self.cached_index = self.cached_index.min(id);
                true
            },
        }
    }

    /// Sleeping → Working/Waking
    fn remove(&mut self, id: usize) {
        debug_assert!(id < self.wakers.len());

        // SAFETY: `id` is the worker's seat index (assigned from
        // `0..seats.len()` during `bind_local_worker`).  `wakers.len()`
        // equals `seats.len()`, so `id` is always in bounds.
        let old = unsafe { self.wakers.get_unchecked_mut(id) };
        match old {
            Some(_) => {
                // Sleeping → Working
                *old = None;
                self.sleeping_num -= 1;
            },
            None => {
                // Waking → Working
                self.waking_count -= 1;
            },
        }
    }

    /// Checks if wakeup coordination is needed
    /// 
    /// Returns `true` if:
    /// - Any workers are in waking state, OR
    /// - All workers are active (sleeping == 0)
    /// 
    /// This prevents unnecessary wakeup attempts when workers
    /// are already transitioning to active state.
    #[inline(always)]
    fn is_waking(&self) -> bool {
        self.waking_count > 0 || self.sleeping_num == 0
    }

    /// Wakes a single sleeping worker if no wakeup is already in progress
    /// 
    /// This implements a "soft" wakeup strategy - only one worker
    /// is woken per available task, reducing contention.
    #[must_use]
    fn take_one(&mut self) -> Option<Waker> {
        self.wakers
            .iter_mut()
            .enumerate()
            .skip(self.cached_index)
            .find_map(|(index, waker)| {
                if waker.is_none() {
                    None
                } else {
                    self.sleeping_num -= 1;
                    self.waking_count += 1;
                    self.cached_index = index + 1;
                    waker.take()
                }
            })
    }
}


// -----------------------------------------------------------------------------
// State Implementation

impl State {
    /// Attempts to wake a single sleeping worker if no wakeup is in progress
    ///
    /// This method implements the thundering herd prevention:
    /// - Atomically sets `is_waking` flag
    /// - Only one thread successfully wakes a worker
    /// - Other threads see the flag and skip wakeup
    #[inline]
    fn wake_one(&self) {
        use Ordering::{AcqRel, Acquire};

        if self
            .is_waking
            .compare_exchange(false, true, AcqRel, Acquire)
            .is_ok()
        {
            let waker = self
                .lounge
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take_one();

            // Split `take_one` and `Waker::wake`,
            // reduce the time that occupied lock
            if let Some(waker) = waker {
                waker.wake();
            }
        }
    }

}

// -----------------------------------------------------------------------------
// Worker Implementation


impl Worker {
    /// Returns a reference to the bound executor state
    /// 
    /// # Safety
    /// Must only be called after successful `bind()`
    #[inline(always)]
    const fn state(&self) -> &State {
        debug_assert!(!self.state.get().is_null());
        unsafe{ &*self.state.get() }
    }

    /// Returns a reference to this worker's local queue
    /// 
    /// # Safety
    /// Must only be called after successful `bind()`
    #[inline(always)]
    const fn queue(&self) -> &ArrayQueue<Runnable> {
        debug_assert!(!self.queue.get().is_null());
        unsafe{ &*self.queue.get() }
    }

    #[inline(never)]
    fn steal_global_inner(src: &ListQueue<Runnable>, dst: &ArrayQueue<Runnable>) {
        let mut deque: ArrayDeque<Runnable, WORKER_QUEUE_SIZE> = ArrayDeque::new();

        // Separate global theft and local storage to minimize lock holding time.
        let mut guard = src.lock_pop();
        for _ in 0..WORKER_QUEUE_SIZE {
            if let Some(runnable) = src.pop_with_lock(&mut guard) {
                // SAFETY: deque was just created (empty) and we push at most
                // WORKER_QUEUE_SIZE items, which equals its capacity.
                unsafe { deque.push_back_unchecked(runnable); }
            } else {
                break;
            }
        }
        ::core::mem::drop(guard);

        while let Some(runnable) = deque.pop_front() {
            let ret = dst.push(runnable);
            debug_assert!(ret.is_ok());
            // SAFETY: This is called from fetch_runnable only after the local
            // queue's pop() returned None, so the queue is empty. Only the
            // current thread pushes to its own local queue (other threads only
            // pop from it during work stealing). With capacity WORKER_QUEUE_SIZE
            // and at most WORKER_QUEUE_SIZE items, push cannot fail.
            unsafe { ret.unwrap_unchecked(); }
        }
    }

    #[inline(never)]
    fn steal_global(&self) -> Option<Runnable> {
        let src: &ListQueue<Runnable> = &self.state().queue;
        let dst: &ArrayQueue<Runnable> = self.queue();

        if let Some(r) = src.pop() {
            Worker::steal_global_inner(src, dst);
            self.wake();
            self.wake_one();
            return Some(r);
        }

        None
    }

    #[inline(never)]
    fn steal_worker_inner(src: &ArrayQueue<Runnable>, dst: &ArrayQueue<Runnable>) {
        let len: usize = src.len() >> 1;
        // if src.len == 1, we do not steal,
        // because we already stole one before calling this function.
        for _ in 0..len {
            if let Some(runnable) = src.pop() {
                let ret = dst.push(runnable);
                debug_assert!(ret.is_ok());
                // SAFETY: Same reasoning as steal_global_inner: dst is the
                // current worker's local queue, which was empty before this
                // steal (fetch_runnable popped from it and got None). Only
                // the current thread pushes to its own queue, so capacity
                // (WORKER_QUEUE_SIZE) is sufficient for the stolen items.
                unsafe { ret.unwrap_unchecked(); }
            } else {
                return;
            }
        }
    }

    #[inline(never)]
    fn steal_worker(&self) -> Option<Runnable> {
        let state: &State = self.state();
        let dst: &ArrayQueue<Runnable> = self.queue();

        // Pick a random starting point in the iterator list and rotate the list.
        let worker_num = state.seats.len();
        let start = self.xor_shift.next_usize(worker_num);
        let iter = state.seats[start..]
            .iter()
            .chain(state.seats[..start].iter())
            .filter(|seat| !ptr::eq(&seat.queue, dst));

        // Try stealing from each local queue in the list.
        for seat in iter {
            let src: &ArrayQueue<Runnable> = &seat.queue;
            if let Some(r) = src.pop() {
                Worker::steal_worker_inner(src, dst);
                self.wake();
                self.wake_one();
                return Some(r);
            }
        }

        None
    }

    /// Transitions worker to sleeping state, registering a waker
    /// 
    /// **Working/Waking/Sleeping** → **Sleeping**
    /// 
    /// - Returns `true` if this is a new sleep (state changed):
    ///   Working/Waking -> Sleeping
    /// - Return `false` if already sleeping (just updating waker):
    ///   Sleeping -> Sleeping
    fn sleep(&self, waker: &Waker) -> bool {
        let state = self.state();
        let mut lounge = state.lounge
            .lock()
            .unwrap_or_else(PoisonError::into_inner);

        if self.working.get() {
            // Working → Sleeping
            lounge.insert(self.seat_index.get(), waker);
            self.working.set(false);
            // loop again, return `true`.
        } else {
            // Already not working, update waker
            if !lounge.try_insert(self.seat_index.get(), waker) {
                // Sleeping -> Sleeping
                return false;
            }
            // else: Waking -> Sleeping, loop again, return `true`.
        }

        state.is_waking.store(lounge.is_waking(), Ordering::Release);
        // Working/Waking -> sleeping, try steal again
        true
    }

    /// Wakes this worker 
    /// 
    /// **Sleeping/Waking** → **Working**
    #[inline(always)]
    fn wake(&self) {
        /// Wakes this worker (Sleeping → Working or Waking → Working).
        #[inline(never)]
        fn wake_internal(this: &Worker) {
            let state: &State = this.state();

            let mut lounge = state.lounge
                .lock()
                .unwrap_or_else(PoisonError::into_inner);

            lounge.remove(this.seat_index.get());

            state.is_waking.store(lounge.is_waking(), Ordering::Release);

            this.working.set(true);
        }

        // Do nothing if self is working
        if !self.working.get() {
            wake_internal(self);
        }
    }

    /// Wakes an other worker if exist. (**Sleeping** → **Waking**).
    #[inline]
    fn wake_one(&self) {
        self.state().wake_one();
    }

    /// Attempts to get a runnable task using the work-stealing hierarchy
    /// 
    /// Priority order (classic work-stealing algorithm):
    /// 1. Local queue (fast path, no synchronization)
    /// 2. Global queue (shared, requires synchronization)
    /// 3. Other workers' queues (work stealing, random victim selection)
    /// 
    /// Returns `Some(Runnable)` if a task was found, `None` otherwise.
    #[inline(always)]
    fn fetch_runnable(&self) -> Option<Runnable> {
        if let Some(runnable) = self.queue().pop() {
            self.wake();
            return Some(runnable);
        }

        core::hint::cold_path();
        self.steal_global().or_else(|| self.steal_worker())
    }

    /// Attempts to get a runnable task
    /// 
    /// - Return Ready and set **Working** if succeeded.
    /// - Return Pending and set **Sleeping** if repeatedly failed.
    async fn runnable(&self) -> Runnable {
        poll_fn(|cx| {
            loop {
                if let Some(r) = self.fetch_runnable() {
                    return Poll::Ready(r);
                }
                // Only enter sleep after the second `None`.
                if !self.sleep(cx.waker()) {
                    // Sleeping -> Sleeping, return Pending
                    return Poll::Pending;
                }
                // else: Working/Waking -> Sleeping, try again.

                // Worker currently has no tasks, try to assist
                // the thread local executor.
                if !LocalExecutor::try_tick() {
                    // If there are no tasks, yield the current thread.
                    std::thread::yield_now();
                }
            }
        })
        .await
    }

    /// Worker thread:
    /// - Uses work-stealing from local/global/other workers
    /// - Processes in batches of `RUN_BATCH` tasks before yielding
    async fn work_run() -> ! {
        /// Number of tasks processed before a worker yields to the scheduler.
        /// This prevents long-running tasks from starving other work.
        const RUN_BATCH: usize = 120;

        // SAFETY: The thread-local worker lives as long as the thread.
        let this: &'static Worker = WORKER.with(|w: &Worker| unsafe {
            core::mem::transmute::<&Worker, &Worker>(w)
        });

        loop {
            for _ in 0..RUN_BATCH {
                this.runnable().await.run();
            }
            futures_lite::future::yield_now().await;
        }
    }

    /// Main thread:
    /// - Cycles through worker seats, polling each one in round-robin
    ///   order for tasks.
    /// - Yields frequently to avoid starving bound workers.
    async fn main_run(state: &State) -> ! {
        debug_assert!(!state.seats.is_empty(), "PoolExecutor requires at least one worker seat");
        let mut counter = 0_usize;

        loop {
            counter = (counter + 1) % state.seats.len();

            let seat = &state.seats[counter];
            if let Some(runnable) = seat.queue.pop() {
                runnable.run();
            }

            futures_lite::future::yield_now().await;
        }
    }
}

// -----------------------------------------------------------------------------
// PoolExecutor Implementation


impl PoolExecutor {
    /// Creates a new executor with the specified number of worker seats
    ///
    /// # Arguments
    /// - `num` - Number of worker threads this executor will support
    ///
    /// # Initial State
    /// - Global queue is empty
    /// - All seats are unoccupied
    /// - Lounge has no sleeping workers
    /// - `is_waking` starts as `false` (no wakeup in progress); the first
    ///   `wake_one()` call will set it via CAS.
    pub fn new(worker_num: usize) -> Self {
        assert!(worker_num > 0, "worker thread num should not be `0`");

        // idle capacity is 32 * 64 == 2048, appropriate? (default is 16 * 64)
        let queue: ListQueue<Runnable> = ListQueue::new(32);

        // [0..worker_num] for worker thread, without main thread
        let seats: CachePadded<Box<[Seat]>> = CachePadded::new(
            (0..worker_num).map(|_|Seat{
                occupied: AtomicBool::new(false),
                queue: ArrayQueue::new(WORKER_QUEUE_SIZE),
            }).collect()
        );

        // [0..worker_num] for worker thread, without main thread
        let lounge: CachePadded<Mutex<Lounge>> = CachePadded::new(Mutex::new(Lounge {
            waking_count: 0,
            sleeping_num: 0,
            cached_index: 0,
            wakers: (0..worker_num).map(|_|None).collect(),
        }));

        let is_waking: AtomicBool = AtomicBool::new(false);

        Self {
            state: Arc::new(State { queue, seats, lounge, is_waking }),
        }
    }

    /// Binds this worker to a specific executor, claiming a seat.
    /// 
    /// This is called when a thread joins a task pool. The worker
    /// atomically claims an unoccupied seat and stores pointers to
    /// the executor state and local queue.
    /// 
    /// # Safety
    /// Worker internally retains a raw pointer into `PoolExecutor`'s
    /// shared [`State`].  The raw pointer stays valid as long as the
    /// [`PoolExecutor`] is alive (the [`State`] is kept in an [`Arc`]
    /// that is dropped together with the executor).
    pub fn bind_local_worker(&self) {
        WORKER.with(|worker|{
            if !worker.state.get().is_null() {
                return;
            }

            let state: &State = &self.state;
            worker.state.set(state);

            for (index, seat) in state.seats.iter().enumerate()  {
                if !seat.occupied.swap(true, Ordering::AcqRel) {
                    worker.queue.set(&seat.queue);
                    worker.seat_index.set(index);
                    worker.xor_shift.randomize();
                    return;
                }
            }

            unreachable!("Failed to bind worker: no seats available in executor");
        })
    }

    /// Spawns a future onto the executor's global queue.
    ///
    /// The schedule closure holds a [`Weak`] reference to the executor's
    /// shared [`State`].  If the [`TaskPool`](crate::TaskPool) is dropped
    /// before the spawned task completes, the next reschedule attempt will
    /// find the `Weak` broken and silently cancel the task (drop the
    /// [`Runnable`]).  This is the safe general‑purpose entry point.
    ///
    /// Returns a [`Task`] handle that can be awaited, cancelled, or
    /// detached.
    pub fn spawn<T: Send>(&self, future: impl Future<Output = T> + Send) -> Task<T> {
        let state: Weak<State> = Arc::downgrade(&self.state);

        let schedule = move |runnable| {
            match state.upgrade() {
                Some(s) => {
                    s.queue.push(runnable);
                    s.wake_one();
                },
                None => {
                    // Pool has been dropped — cancel the task by dropping
                    // the Runnable without re‑scheduling.
                    core::hint::cold_path();
                    ::core::mem::drop(runnable);
                },
            }
        };

        // SAFETY: The schedule closure captures a `Weak<State>`, breaking the
        // dependency on `PoolExecutor`'s lifetime.  If the pool disappears the
        // weak reference simply fails to upgrade and the task is cleanly
        // cancelled.  No dangling reference is possible.
        let (runnable, task) = unsafe {
            async_task::Builder::new()
                .propagate_panic(true)
                .spawn_unchecked(|()|future, schedule)
        };

        // Immediately schedule the task for execution
        runnable.schedule();
        task
    }

    /// Spawns a future onto the executor's global queue **without** a
    /// weak‑reference safety net.
    ///
    /// The schedule closure captures a bare `&State` reference.  The caller
    /// is responsible for ensuring that [`Self`] (and therefore the
    /// [`State`]) outlives the returned [`Task`] — dropping the pool while
    /// any `Task` from this function is still alive constitutes undefined
    /// behaviour.
    ///
    /// This variant is used internally by [`Scope::spawn`], where the
    /// enclosing [`TaskPool::scope`] blocks until every spawned task
    /// completes, trivially satisfying the lifetime requirement.
    ///
    /// # Safety
    ///
    /// `Self` must outlive every [`Task`] returned by this function.
    ///
    /// [`Scope::spawn`]: crate::Scope::spawn
    /// [`TaskPool::scope`]: crate::TaskPool::scope
    pub unsafe fn spawn_unchecked<T: Send>(&self, future: impl Future<Output = T> + Send) -> Task<T> {
        // SAFETY (caller): the caller guarantees that `Self` outlives the
        // returned `Task`, so the `&State` captured below stays valid for
        // the full lifetime of every schedule closure.
        let state: &State = &self.state;

        let schedule = move |runnable| {
            state.queue.push(runnable);
            state.wake_one();
        };

        // SAFETY: the caller's lifetime guarantee (documented above) ensures
        // the captured `&State` is never dangling.
        let (runnable, task) = unsafe {
            async_task::Builder::new()
                .propagate_panic(true)
                .spawn_unchecked(|()|future, schedule)
        };

        // Immediately schedule the task for execution
        runnable.schedule();
        task
    }

    /// Runs the executor until the given future completes
    pub async fn run<T>(&self, stop_signal: impl Future<Output = T>) -> T {
        let is_main_thread = WORKER.with(|w: &Worker| w.queue.get().is_null());
        let state: &State = &self.state;

        let run_forever = async move {
            if is_main_thread {
                Worker::main_run(state).await;
            } else {
                Worker::work_run().await;
            }
        };

        // Run until stop signal completes
        run_forever.or(stop_signal).await
    }

}
