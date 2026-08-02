#![expect(unsafe_code, reason = "lifetime transmutation")]

use core::cell::{Cell, RefCell};
use core::marker::PhantomData;
use core::panic::{UnwindSafe, RefUnwindSafe};
use std::borrow::Cow;

use async_task::Task;

use super::{block_on, LocalExecutor, MainExecutor};

// ----------------------------------------------------------------------------
// TaskPoolBuilder

/// Builder for configuring and creating a [`TaskPool`].
///
/// In wasm mode, all configuration methods are no-ops — there are no worker
/// threads to configure. The builder exists to maintain API parity with the
/// multi-threaded mode so that user code does not need conditional compilation.
///
/// # Examples
///
/// ```
/// # use zlim_task::TaskPoolBuilder;
/// let pool = TaskPoolBuilder::new()
///     .thread_count(4)
///     .thread_name("my-pool")
///     .build();
/// ```
#[must_use]
#[derive(Default)]
pub struct TaskPoolBuilder(()); // (()):  Prohibit direct creation

impl TaskPoolBuilder {
    /// Creates a new `TaskPoolBuilder` instance.
    #[inline]
    pub const fn new() -> Self {
        Self(())
    }

    /// Sets the number of worker threads.
    ///
    /// No-op in web assembly mode — only the current thread is used.
    #[inline]
    pub fn thread_count(self, thread_count: usize) -> Self {
        let _ = thread_count;
        self
    }

    /// Sets the stack size for worker threads.
    ///
    /// No-op in web assembly mode.
    #[inline]
    pub fn stack_size(self, stack_size: usize) -> Self {
        let _ = stack_size;
        self
    }

    /// Sets the name prefix for worker threads.
    ///
    /// No-op in web assembly mode.
    #[inline]
    pub fn thread_name(self, thread_name: impl Into<Cow<'static, str>>) -> Self {
        let _ = thread_name;
        self
    }

    /// Registers a callback invoked when a worker thread is spawned.
    ///
    /// No-op in web assembly mode.
    #[inline]
    pub fn on_thread_spawn(self, f: impl Fn() + Send + Sync + 'static) -> Self {
        let _ = f;
        self
    }

    /// Registers a callback invoked when a worker thread is destroyed.
    ///
    /// No-op in web assembly mode.
    #[inline]
    pub fn on_thread_destroy(self, f: impl Fn() + Send + Sync + 'static) -> Self {
        let _ = f;
        self
    }

    /// Consumes the builder and creates a new [`TaskPool`].
    #[inline]
    #[must_use]
    pub fn build(self) -> TaskPool {
        TaskPool(())
    }
}

// ----------------------------------------------------------------------------
// TaskPool

/// A task pool backed by the browser's microtask queue.
///
/// In WASM, there is only one thread — the browser's main thread. All spawned
/// tasks are submitted to the JS microtask queue via [`web_task`] and will
/// execute asynchronously when the browser yields control. No background worker
/// threads are spawned.
///
/// # Executors
///
/// - **JS microtask queue** — used by [`spawn`], [`spawn_local`], and
///   [`spawn_to_main`]. Tasks are handed to the browser's event loop via
///   [`web_task`] and run as microtasks.
///
/// - **`LocalExecutor`** / **`MainExecutor`** — used internally by
///   [`scope`] to drive scoped tasks. Tasks spawned on a [`Scope`] are
///   executed synchronously within the scope call, not on the microtask queue.
///
/// # Deadlock Warning
///
/// Due to WASM's single-threaded nature, do **not** block the current thread
/// waiting for a [`Task`] handle returned by [`spawn`], [`spawn_local`], or
/// [`spawn_to_main`] — the submitted task cannot run until the current task
/// yields, so blocking will cause a deadlock.
///
/// # Examples
///
/// ```
/// use zlim_task::TaskPool;
///
/// let pool = TaskPool::new();
/// let task = pool.spawn(async { 42 });
/// // The task will run when the browser processes the microtask queue.
/// ```
///
/// [`spawn`]: Self::spawn
/// [`spawn_local`]: Self::spawn_local
/// [`spawn_to_main`]: Self::spawn_to_main
/// [`scope`]: Self::scope
#[derive(Debug, Default)]
pub struct TaskPool(pub(super) ()); // (()):  Prohibit direct creation

impl TaskPool {
    /// Creates a new `TaskPool`.
    ///
    /// In web assembly mode, this does not spawn any threads.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        TaskPool(())
    }

    /// Returns the number of threads owned by the task pool.
    ///
    /// Always returns `1` in web assembly mode.
    #[inline]
    pub fn thread_count(&self) -> usize {
        1
    }

    /// Spawns a `Send + 'static` future onto the pool.
    ///
    /// The task is submitted to the browser's microtask queue via
    /// [`web_task::spawn`] and will **not** run immediately — it executes
    /// only when the current call stack unwinds and the browser processes
    /// pending microtasks. Returns a [`Task`] handle.
    ///
    /// # Deadlock Warning
    ///
    /// Do **not** block waiting for the returned [`Task`] handle on the
    /// WASM main thread. The spawned task cannot run until the current
    /// synchronous code yields, so blocking will deadlock. Use
    /// [`TaskPool::scope`] instead if you need to collect results
    /// synchronously.
    ///
    /// [`TaskPool::scope`]: Self::scope
    #[inline]
    pub fn spawn<T, F>(&self, future: F) -> Task<T>
    where
        T: Send + 'static,
        F: Future<Output = T> + Send + 'static,
    {
        web_task::spawn(future)
    }

    /// Spawns a `!Send` (non-`Send`) future onto the pool.
    ///
    /// The task is submitted to the browser's microtask queue via
    /// [`web_task::spawn_local`]. Because WASM is single-threaded, the task
    /// never leaves the current thread and therefore does not require `Send`.
    ///
    /// As with [`spawn`], the task will **not** run immediately.
    ///
    /// # Deadlock Warning
    ///
    /// Do **not** block waiting for the returned [`Task`] handle — the same
    /// deadlock risk as [`spawn`] applies here. Use [`TaskPool::scope`] to
    /// collect results synchronously.
    ///
    /// [`spawn`]: Self::spawn
    /// [`TaskPool::scope`]: Self::scope
    #[inline]
    pub fn spawn_local<T: 'static, F>(&self, future: F) -> Task<T>
    where
        F: Future<Output = T> + 'static,
    {
        web_task::spawn_local(future)
    }

    /// Spawns a `Send + 'static` future onto the main thread.
    ///
    /// In WASM, there is only one thread, so this function is equivalent
    /// to [`spawn`] — the task is submitted to the browser's microtask
    /// queue via [`web_task::spawn`]. It exists for API parity with
    /// multi-threaded mode.
    ///
    /// # Deadlock Warning
    ///
    /// Same deadlock risk as [`spawn`]. Do **not** block waiting for the
    /// returned [`Task`] handle.
    ///
    /// [`spawn`]: Self::spawn
    #[inline]
    pub fn spawn_to_main<T, F>(&self, future: F) -> Task<T>
    where
        T: Send + 'static,
        F: Future<Output = T> + Send + 'static,
    {
        web_task::spawn(future)
    }
}

// ----------------------------------------------------------------------------
// Scope

/// A scope for running non-`'static` futures on a [`TaskPool`].
///
/// Created by [`TaskPool::scope`], this object allows spawning tasks that
/// borrow stack-local data. All tasks spawned on the scope are guaranteed to
/// complete before `scope` returns, so borrowed references remain valid.
///
/// This is analogous to `rayon::scope` and `crossbeam::scope`.
///
/// # Examples
///
/// ```no_run
/// use zlim_task::TaskPool;
///
/// let pool = TaskPool::new();
/// let values = [1_u32, 2, 3, 4];
///
/// let doubled = pool.scope(|scope| {
///     for value in &values {
///         scope.spawn(async move { *value * 2 });
///     }
/// });
///
/// assert_eq!(doubled, vec![2, 4, 6, 8]);
/// ```
#[derive(Debug)]
pub struct Scope<'sco, 'env: 'sco, T> {
    // The number of pending tasks spawned on the scope
    pending: &'sco Cell<usize>,
    // Vector to gather results of all futures spawned during scope run
    results: &'env RefCell<Vec<Option<T>>>,
    // make `Scope` invariant over 'sco and 'env
    _marker1: PhantomData<&'sco mut &'sco ()>,
    _marker2: PhantomData<&'env mut &'env ()>,
    // Ensure the certainty of Sync, Send, etc.
    _marker3: PhantomData<*const ()>,
}

// Placeholder implementaion to maintain code consistency.
impl<T> Drop for Scope<'_, '_, T> {
    fn drop(&mut self) {
        /* do nothing */
    }
}

// SAFETY: `Scope` is `Send` when `T: Send` because all tasks are driven to
// completion on the current thread before `scope` returns, so there is no
// concurrent access across threads.
unsafe impl<T: Send> Send for Scope<'_, '_, T> {}
unsafe impl<T: Send> Sync for Scope<'_, '_, T> {}
impl<T> UnwindSafe for Scope<'_, '_, T> {}
impl<T> RefUnwindSafe for Scope<'_, '_, T> {}

impl<'sco, 'env, T: Send + 'env> Scope<'sco, 'env, T> {
    /// Spawns a `Send` future onto the scope.
    ///
    /// In WASM mode, this delegates to [`spawn_local`]. Unlike
    /// [`TaskPool::spawn`], scope tasks are driven synchronously via the
    /// Rust executors within the [`TaskPool::scope`] call — they do **not**
    /// go to the JS microtask queue.
    ///
    /// [`spawn_local`]: Self::spawn_local
    /// [`TaskPool::spawn`]: crate::TaskPool::spawn
    /// [`TaskPool::scope`]: crate::TaskPool::scope
    #[inline]
    pub fn spawn<F: Future<Output = T> + Send + 'sco>(&self, f: F) {
        self.spawn_local(f);
    }

    /// Spawns a non-`Send` future onto the scope.
    ///
    /// The task is submitted to the `LocalExecutor` and will run on the
    /// current thread. Unlike [`spawn`], this method does not require `Send`
    /// on the future, so it can capture `Rc`, `Cell`, and other `!Send` types.
    ///
    /// The future may borrow data with lifetime `'sco`, which is guaranteed
    /// to outlive the task because all scope tasks complete before
    /// [`TaskPool::scope`] returns.
    ///
    /// [`spawn`]: Self::spawn
    /// [`TaskPool::scope`]: crate::TaskPool::scope
    pub fn spawn_local<F: Future<Output = T> + 'sco>(&self, f: F) {
        let pending = self.pending;
        let results = self.results;

        // increment the number of pending tasks
        pending.update(|i| i + 1);

        // add a spot to keep the result, and record the index
        let mut buf = results.borrow_mut();
        let task_number = buf.len();
        buf.push(None);
        ::core::mem::drop(buf);

        // create the job closure
        let f = async move {
            let result = f.await;

            // store the result in the allocated slot
            let mut buf = results.borrow_mut();
            buf[task_number] = Some(result);
            drop(buf);

            // decrement the pending tasks count
            pending.update(|i| i - 1);
        };

        // SAFETY: The future `f` captures `'sco`-bounded references to
        // `pending` and `results`, both of which are stack-allocated in
        // `TaskPool::scope` and transmuted to `'env`. All spawned tasks
        // are driven to completion before `scope` returns, so `'sco`
        // references remain valid for the duration of the task.
        unsafe {
            LocalExecutor::spawn_unchecked(f).detach();
        }
    }

    /// Spawns a future that must run on the main thread onto the scope.
    ///
    /// The task is submitted to the `MainExecutor` and will be driven by
    /// the main thread's executor during the scope. The future must be
    /// `Send` because the `MainExecutor` can receive tasks from any thread.
    ///
    /// As with [`spawn_local`], the future may borrow data with lifetime
    /// `'sco`, which is valid until [`TaskPool::scope`] returns.
    ///
    /// [`spawn_local`]: Self::spawn_local
    /// [`TaskPool::scope`]: crate::TaskPool::scope
    pub fn spawn_to_main<F: Future<Output = T> + Send + 'sco>(&self, f: F) {
        let pending = self.pending;
        let results = self.results;

        // increment the number of pending tasks
        pending.update(|i| i + 1);

        // add a spot to keep the result, and record the index
        let mut buf = results.borrow_mut();
        let task_number = buf.len();
        buf.push(None);
        ::core::mem::drop(buf);

        // create the job closure
        let f = async move {
            let result = f.await;

            // store the result in the allocated slot
            let mut buf = results.borrow_mut();
            buf[task_number] = Some(result);
            drop(buf);

            // decrement the pending tasks count
            pending.update(|i| i - 1);
        };

        // SAFETY: The future `f` captures `'sco`-bounded references to
        // `pending` and `results`, both of which are stack-allocated in
        // `TaskPool::scope` and transmuted to `'env`. All spawned tasks
        // are driven to completion before `scope` returns, so `'sco`
        // references remain valid for the duration of the task.
        unsafe {
            MainExecutor::spawn_unchecked(f).detach();
        }
    }
}

impl TaskPool {
    /// Creates a scope for running non-`'static` futures on the pool.
    ///
    /// This method takes a closure `f` which receives a [`Scope`] object.
    /// The scope can be used to spawn tasks that borrow stack-local data.
    /// All tasks spawned on the scope are driven to completion before this
    /// method returns, collecting their results into a `Vec<T>`.
    ///
    /// This is analogous to `rayon::scope` and `crossbeam::scope`.
    ///
    /// In web assembly mode, the scope blocks the current thread and
    /// drives both the `LocalExecutor` and `MainExecutor` so that tasks
    /// submitted via [`Scope::spawn`], [`Scope::spawn_local`], and
    /// [`Scope::spawn_to_main`] all make progress.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_task::TaskPool;
    ///
    /// let pool = TaskPool::new();
    /// let values = [1_u32, 2, 3, 4];
    ///
    /// let doubled = pool.scope(|scope| {
    ///     for value in &values {
    ///         scope.spawn(async move { *value * 2 });
    ///     }
    /// });
    ///
    /// assert_eq!(doubled, vec![2, 4, 6, 8]);
    /// ```
    pub fn scope<'env, F, T>(&self, f: F) -> Vec<T>
    where
        T: Send + 'static,
        F: for<'s> FnOnce(&'s Scope<'s, 'env, T>),
    {
        // SAFETY: This safety comment applies to all references transmuted to 'env.
        //
        // Any futures spawned with these references need to return before this function
        // completes. This is guaranteed because we drive all the futures spawned onto
        // the Scope to completion in this function.
        //
        // However, rust has no way of knowing this so we transmute the lifetimes to 'env
        // here to appease the compiler as it is unable to validate safety.
        //
        // Any usages of the references passed into `Scope` must be accessed through
        // the transmuted reference for the rest of this function.

        // SAFETY: As above, all futures must complete in this function so we can change the lifetime
        let results: RefCell<Vec<Option<T>>> = RefCell::new(Vec::new());
        let results_ref: &'env RefCell<Vec<Option<T>>> = unsafe {
            core::mem::transmute::<&RefCell<Vec<Option<T>>>, &'env RefCell<Vec<Option<T>>>>(&results)
        };

        // SAFETY: As above, all futures must complete in this function so we can change the lifetime
        let pending: Cell<usize> = Cell::new(0);
        let pending_ref: &'env Cell<usize> = unsafe {
            core::mem::transmute::<&Cell<usize>, &'env Cell<usize>>(&pending)
        };

        // SAFETY: As above, all futures must complete in this function so we can change the lifetime
        let scope: Scope<'_, 'env, T> = Scope {
            pending: pending_ref,
            results: results_ref,
            _marker1: PhantomData,
            _marker2: PhantomData,
            _marker3: PhantomData,
        };
        let scope_ref: &'env Scope<'_, 'env, T> = unsafe {
            core::mem::transmute::<&Scope<T>, &'env Scope<T>>(&scope)
        };

        // Spawn Tasks
        f(scope_ref);

        // Run Tasks
        let stop_signal = async move {
            while pending_ref.get() != 0 {
                futures_lite::future::yield_now().await;
            }
        };
        // For web assembly task pool, we must tick both
        // `LocalExecutor` and `MainExecutor`. Otherwise, deadlock may occur.
        block_on(LocalExecutor::run(MainExecutor::run(stop_signal)));

        // Collect Results
        results.take().into_iter().map(Option::unwrap).collect()
    }
}
