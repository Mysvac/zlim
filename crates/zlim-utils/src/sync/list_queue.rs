#![expect(unsafe_code, reason = "original implementation need unsafe codes")]

use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ops::BitOrAssign;
use core::panic::{RefUnwindSafe, UnwindSafe};
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering::{Acquire, Release};
use core::{fmt, ptr};

use crate::ext::CachePadded;
use crate::sync::{SpinLock, SpinLockGuard};

// ----------------------------------------------------------------------------
// Bit Flags

const BLOCK_SIZE: usize = 64;

// ----------------------------------------------------------------------------
// Block

/// A single queue block.
struct Block<T> {
    /// **field_0**: the index of head.
    ///
    /// For example the buffer is `[0, 0, 1, 1, 0]`
    /// (`0` indicates no data), then this index is `2`.
    ///
    /// **field_1**: the cache of buffer state.
    ///
    /// Buffer state is a atomic u64 value, indicate whether
    /// there are elements at each position through bits.
    ///
    /// State is shared between head and tail operation.
    /// So we use this cache to reduce atomic loading.
    head_cache: CachePadded<(usize, u64)>,

    /// **field_0**: the index of tail.
    ///
    /// For example the buffer is `[0, 0, 1, 1, 0]`
    /// (`0` indicates no data), then this index is `4`.
    ///
    /// **field_1**: buffer state.
    ///
    /// Buffer state is a atomic u64 value, indicate whether
    /// there are elements at each position through bits.
    ///
    /// For example the buffer is `[0, 0, 1, 1, 1]`,
    /// then this state value is `0x11100` .
    tail_state: CachePadded<(usize, AtomicU64)>,

    /// data storage.
    slots: [MaybeUninit<T>; BLOCK_SIZE],

    /// Link to the next block, null_ptr if it's tail block.
    next: *mut Block<T>,
}

impl<T> Block<T> {
    /// Create a empty block.
    ///
    /// inline(never): reduce the stack size of `IdleQueue::get`.
    #[inline(never)]
    fn new() -> Box<Self> {
        Box::new(Block {
            head_cache: CachePadded::new((0, 0)),
            tail_state: CachePadded::new((0, AtomicU64::new(0))),
            // SAFETY: Convert full uninit to internal uninit is safe.
            slots: unsafe { <MaybeUninit<[MaybeUninit<T>; BLOCK_SIZE]>>::uninit().assume_init() },
            next: ptr::null_mut(),
        })
    }

    #[inline]
    fn reset(&mut self) {
        self.next = ptr::null_mut();
        self.head_cache.0 = 0;
        self.head_cache.1 = 0;
        self.tail_state.0 = 0;
        self.tail_state.1.store(0, Release);
    }
}

/// Drop remaining initialized elements in a block.
///
/// Only elements in range [head_index, tail_index) are valid.
impl<T> Drop for Block<T> {
    fn drop(&mut self) {
        if core::mem::needs_drop::<T>() {
            let index = self.head_cache.0;
            let end = self.tail_state.0;
            if index < end {
                // SAFETY: `index` is the head position (within [0, BLOCK_SIZE)),
                // `end` is the tail position (within [0, BLOCK_SIZE]), and
                // elements in [index, end) were written by `push` and not yet
                // read by `pop`. The pointer arithmetic and length are valid.
                unsafe {
                    let data = self.slots.as_mut_ptr().add(index) as *mut T;
                    let pslice = ptr::slice_from_raw_parts_mut::<T>(data, end - index);
                    ptr::drop_in_place(pslice);
                }
            }
        }
    }
}

// ----------------------------------------------------------------------------
// IdleQueue

/// Pool of reusable blocks.
///
/// Blocks are recycled only when:
/// - fully popped
/// - detached from queue
struct IdleQueue<T> {
    blocks: SpinLock<Vec<Box<Block<T>>>>,
    max_num: usize,
}

impl<T> IdleQueue<T> {
    /// Create a `IdleQueue` with specific max number.
    #[inline]
    const fn new(idle_limit: usize) -> Self {
        IdleQueue {
            blocks: SpinLock::new(Vec::new()),
            max_num: idle_limit,
        }
    }

    /// Pushes a fully detached block into the idle queue.
    ///
    /// If the idle queue has reached its capacity (`max_num`), the block is
    /// immediately dropped instead of being added.
    #[inline]
    fn push(&self, ptr: *mut Block<T>) {
        let boxed = unsafe { Box::from_raw(ptr) };
        let mut blocks = self.blocks.lock();
        if blocks.len() < self.max_num {
            blocks.push(boxed);
        }
        // Ensure that lockguard drop before boxed,
        // minimize the lock holding time.
        ::core::mem::drop(blocks);
    }

    /// Pushes a fully detached block into the idle queue.
    #[inline]
    fn push_mut(&mut self, ptr: *mut Block<T>) {
        let boxed = unsafe { Box::from_raw(ptr) };
        let blocks = self.blocks.get_mut();
        if blocks.len() < self.max_num {
            blocks.push(boxed);
        }
    }

    /// Get a empty block from idle queue.
    ///
    /// If the idle queue is empty, this function will create
    /// a new block through `Block::new`.
    #[inline]
    fn get(&self) -> *mut Block<T> {
        // minimize the lock holding time.
        let boxed = self.blocks.lock().pop();
        if let Some(mut boxed) = boxed {
            boxed.reset();
            Box::leak(boxed)
        } else {
            Box::leak(<Block<T>>::new())
        }
    }

    /// Get a empty block from idle queue.
    #[inline]
    fn get_mut(&mut self) -> *mut Block<T> {
        let boxed = self.blocks.get_mut().pop();
        if let Some(mut boxed) = boxed {
            boxed.reset();
            Box::leak(boxed)
        } else {
            Box::leak(<Block<T>>::new())
        }
    }
}

// ----------------------------------------------------------------------------
// ListQueue

/// An unbounded MPMC queue with double spin-lock.
///
/// Problems:
/// - Ring-buffer implementations are fast but cannot grow.
/// - Segment-queue implementations avoid a fixed size but do not reuse segments.
///
/// This implementation:
/// - Uses a block-based linked list (similar to a segment queue) and adds a recycler
///   (idle queue) to reuse blocks and avoid repeated allocations.
/// - Reduces false sharing by separating head and tail pointers with CachePadded,
///   following patterns used in crossbeam-like implementations.
/// - Allows concurrent readers and writers without a global lock. Only short critical
///   sections occur when exchanging blocks with the idle queue, which is rare.
///
/// # Examples
///
/// ```
/// use core::sync::atomic::{Ordering, AtomicUsize};
/// use zlim_utils::sync::ListQueue;
/// use std::thread;
///
/// const COUNT: usize = 25_000;
/// const THREADS: usize = 4;
///
/// let q = ListQueue::<usize>::default();
/// let v = (0..COUNT).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>();
///
/// thread::scope(|scope| {
///     // consumer
///     for _ in 0..THREADS {
///         scope.spawn(|| {
///             for _ in 0..COUNT {
///                 let n = loop {
///                     if let Some(x) = q.pop() {
///                         break x;
///                     }
///                 };
///                 v[n].fetch_add(1, Ordering::SeqCst);
///             }
///         });
///     }
///     // producer
///     for _ in 0..THREADS {
///         scope.spawn(|| {
///             for i in 0..COUNT {
///                 q.push(i);
///             }
///         });
///     }
/// });
///
/// for c in v {
///     assert_eq!(c.load(Ordering::SeqCst), THREADS);
/// }
/// ```
pub struct ListQueue<T> {
    /// field_0: block pointer. field_1: block_id.
    head_id: CachePadded<SpinLock<(*mut Block<T>, usize)>>,
    /// field_0: block pointer. field_1: block_id.
    tail_id: CachePadded<SpinLock<(*mut Block<T>, usize)>>,
    /// idle block queue.
    idle: IdleQueue<T>,
    _marker: PhantomData<T>,
}

unsafe impl<T: Send> Sync for ListQueue<T> {}
unsafe impl<T: Send> Send for ListQueue<T> {}
impl<T> UnwindSafe for ListQueue<T> {}
impl<T> RefUnwindSafe for ListQueue<T> {}

impl<T> Drop for ListQueue<T> {
    fn drop(&mut self) {
        let mut ptr = self.head_id.get_mut().0;
        while !ptr.is_null() {
            unsafe {
                let boxed = Box::from_raw(ptr);
                ptr = (*ptr).next;
                ::core::mem::drop(boxed);
            }
        }
    }
}

impl<T> Default for ListQueue<T> {
    /// Create an [`ListQueue`] with [`DEFAULT_LIMIT`](ListQueue::DEFAULT_LIMIT).
    ///
    /// This is equivalent to `new(DEFAULT_LIMIT)`.
    ///
    /// See more information in [`ListQueue::new`].
    #[inline]
    fn default() -> Self {
        Self::new(Self::DEFAULT_LIMIT)
    }
}

impl<T> ListQueue<T> {
    /// Default number of `idle_limit`.
    ///
    /// When the number of blocks in the idle pool reaches `idle_limit`, any
    /// subsequently detached blocks will be dropped immediately instead of being
    /// pushed into the pool, allowing their memory to be released.
    ///
    /// This is useful to bound memory usage in programs that can experience large,
    /// short-lived bursts of activity.
    ///
    /// At present, the average memory usage when the idle queue is full is around 150KB by default.
    ///
    /// This is not suitable for all scenarios, and it is recommended that
    /// users manually specify the limit using [`ListQueue::new`].
    pub const DEFAULT_LIMIT: usize = {
        if size_of::<T>() >= 128 {
            2
        } else if size_of::<T>() >= 32 {
            4
        } else if size_of::<T>() > 8 {
            8
        } else {
            16
        }
    };

    /// Creates a [`ListQueue`] with a specific idle-block limit.
    ///
    /// Note that a block can hold **64** elements.
    ///
    /// Can also call [`ListQueue::default`](Self::default) to use
    /// [`DEFAULT_LIMIT`] if you are unsure what value to choose.
    ///
    /// # Rules
    ///
    /// Regardless of `idle_limit`, one active block is always allocated.
    /// No idle blocks are pre-allocated.
    ///
    /// When the number of blocks in the idle pool reaches `idle_limit`, any
    /// subsequently detached blocks will be dropped immediately instead of being
    /// pushed into the pool, allowing their memory to be released.
    ///
    /// This is useful to bound memory usage in programs that can experience large,
    /// short-lived bursts of activity.
    ///
    /// `idle_limit == 0` is valid, its behavior is similar to crossbeam's
    /// `SegQueue`, without any block reuse.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::ListQueue;
    ///
    /// let q = ListQueue::<i32>::new(4);
    /// q.push(1);
    /// assert_eq!(q.pop(), Some(1));
    /// assert!(q.pop().is_none());
    /// ```
    ///
    /// [`DEFAULT_LIMIT`]: Self::DEFAULT_LIMIT
    #[inline]
    pub fn new(idle_limit: usize) -> Self {
        let idle_queue = IdleQueue::new(idle_limit);
        // Leak block after `IdleQueue::new` to avoid memory leakage if IdleQueue panicked.
        let block = Box::leak(Block::<T>::new());
        Self {
            idle: idle_queue,
            head_id: CachePadded::new(SpinLock::new((block, 0))),
            tail_id: CachePadded::new(SpinLock::new((block, 0))),
            _marker: PhantomData,
        }
    }

    /// Push a value into the queue.
    ///
    /// `push` appends the value into the current tail block.
    ///
    /// If that block fills as a result of the push,
    /// a fresh block is fetched from the idle pool (or allocated)
    /// and linked as the new tail.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::ListQueue;
    ///
    /// let q = ListQueue::default();
    /// q.push(42);
    /// q.push(43);
    /// assert_eq!(q.pop(), Some(42));
    /// assert_eq!(q.pop(), Some(43));
    /// ```
    pub fn push(&self, value: T) {
        let mut guard = self.tail_id.lock();

        // SAFETY: `guard.0` is initialized in `ListQueue::new` and updated by
        // `IdleQueue::get` whenever the tail advances to a new block. The tail
        // lock guarantees exclusive write access, so the pointer is always valid.
        let block = unsafe { &mut *guard.0 };

        let index = block.tail_state.0;
        debug_assert!(index < BLOCK_SIZE);

        unsafe {
            ptr::write(block.slots.as_mut_ptr().add(index) as *mut T, value);
        }

        if index + 1 == BLOCK_SIZE {
            let new_block = self.idle.get();
            block.next = new_block;
            guard.0 = new_block;
            guard.1 = guard.1.wrapping_add(1);
        }

        // Setting bit flag after setting `next` block.
        // Ensure that `next` ptr can be visited in `pop` function.
        block.tail_state.0 = index + 1;
        block.tail_state.1.fetch_or(1 << index, Release);
    }

    /// Pushes an element to the queue with exclusive mutable access.
    ///
    /// Avoids atomic operations and synchronization, assuming
    /// no other threads access the queue concurrently.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::ListQueue;
    ///
    /// let mut q = ListQueue::default();
    ///
    /// q.push_mut(10);
    /// q.push_mut(20);
    /// ```
    pub fn push_mut(&mut self, value: T) {
        let tail = self.tail_id.get_mut();

        // SAFETY: `tail.0` is initialized in `ListQueue::new` and updated whenever
        // the tail advances to a new block. With `&mut self` we have exclusive
        // access, so the pointer is always valid and no other thread can observe it.
        let block = unsafe { &mut *tail.0 };

        let index = block.tail_state.0;
        debug_assert!(index < BLOCK_SIZE);

        unsafe {
            ptr::write(block.slots.as_mut_ptr().add(index) as *mut T, value);
        }

        if index + 1 == BLOCK_SIZE {
            let new_block = self.idle.get_mut();
            block.next = new_block;
            tail.0 = new_block;
            tail.1 = tail.1.wrapping_add(1);
        }

        block.tail_state.0 = index + 1;
        block.tail_state.1.get_mut().bitor_assign(1 << index);
    }

    /// Pop a value from the queue.
    ///
    /// Returns `Some(T)` when an element is available, or `None` when the queue
    /// is currently empty.
    ///
    /// Pop operates on the head block; if the head block becomes empty
    /// as a result of the pop it will be detached and returned to the idle pool
    /// (or dropped if the idle pool is full).
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::ListQueue;
    ///
    /// let q = ListQueue::default();
    /// assert!(q.pop().is_none());
    /// q.push(5);
    /// assert_eq!(q.pop(), Some(5));
    /// ```
    pub fn pop(&self) -> Option<T> {
        let mut guard = self.head_id.lock();
        debug_assert!(!guard.0.is_null());

        // SAFETY: `guard.0` is initialized in `ListQueue::new` and updated by
        // `IdleQueue::get` whenever the head advances to the next block. The head
        // lock guarantees exclusive read access, so the pointer is always valid.
        let block = unsafe { &mut *guard.0 };

        let index = block.head_cache.0;
        debug_assert!(index < BLOCK_SIZE);

        let bit_flag = 1_u64 << index;
        if block.head_cache.1 & bit_flag == 0 {
            // slow path, update cache
            block.head_cache.1 = block.tail_state.1.load(Acquire);
            if block.head_cache.1 & bit_flag == 0 {
                return None;
            }
        }

        let value = unsafe { ptr::read(block.slots.as_ptr().add(index) as *mut T) };

        let new_index = index + 1;

        block.head_cache.0 = new_index;

        if new_index == BLOCK_SIZE {
            let old_ptr = block as *mut Block<T>;
            let next_ptr = block.next;
            // index + 1 == BLOCK_SIZE, so tail_index == BLOCK_SIZE.
            // next_ptr must be set by `push` function.
            debug_assert!(!next_ptr.is_null());
            guard.0 = next_ptr;
            guard.1 = guard.1.wrapping_add(1);

            // Early release to improve parallelism.
            ::core::mem::drop(guard);

            self.idle.push(old_ptr);
        }

        Some(value)
    }

    /// Pops the head element from the queue using an exclusive reference.
    ///
    /// Avoids atomic operations and synchronization, assuming
    /// no other threads access the queue concurrently.
    ///
    /// If the queue is empty, `None` is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::ListQueue;
    ///
    /// let mut q = ListQueue::default();
    ///
    /// q.push(10);
    /// q.push(20);
    /// assert_eq!(q.pop_mut(), Some(10));
    /// assert_eq!(q.pop_mut(), Some(20));
    /// assert!(q.pop_mut().is_none());
    /// ```
    pub fn pop_mut(&mut self) -> Option<T> {
        let head = self.head_id.get_mut();
        debug_assert!(!head.0.is_null());

        // SAFETY: `head.0` is initialized in `ListQueue::new` and updated whenever
        // the head advances to the next block. With `&mut self` we have exclusive
        // access, so the pointer is always valid.
        let block = unsafe { &mut *head.0 };

        let index = block.head_cache.0;
        debug_assert!(index < BLOCK_SIZE);

        let bit_flag = 1_u64 << index;
        if block.head_cache.1 & bit_flag == 0 {
            // slow path, update cache
            block.head_cache.1 = *block.tail_state.1.get_mut();
            if block.head_cache.1 & bit_flag == 0 {
                return None;
            }
        }

        let value = unsafe { ptr::read(block.slots.as_ptr().add(index) as *mut T) };

        let new_index = index + 1;

        block.head_cache.0 = new_index;

        if new_index == BLOCK_SIZE {
            let old_ptr = block as *mut Block<T>;
            let next_ptr = block.next;
            // index + 1 == BLOCK_SIZE, so tail_index == BLOCK_SIZE.
            // next_ptr must be set by `push` function.
            debug_assert!(!next_ptr.is_null());
            head.0 = next_ptr;
            head.1 = head.1.wrapping_add(1);

            self.idle.push_mut(old_ptr);
        }

        Some(value)
    }

    /// Checks if the queue is empty.
    ///
    /// In concurrent scenarios, the return value of this method is time-sensitive:
    /// - **Single-consumer** pattern: Calling from the consumer side is reliable
    /// - **Multi-consumer multi-producer** pattern: The return value may become stale immediately
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::ListQueue;
    ///
    /// let q = ListQueue::default();
    /// assert!(q.is_empty());
    ///
    /// q.push(10);
    /// assert!(!q.is_empty());
    ///
    /// q.pop();
    /// assert!(q.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        let mut guard = self.head_id.lock();
        // SAFETY: `guard.0` is initialized in `ListQueue::new` and updated via
        // `IdleQueue::get`. The head lock guarantees exclusive access, so the
        // block pointer is always valid. We only read head_cache fields.
        let block = unsafe { &mut *guard.0 };

        let index = block.head_cache.0;
        debug_assert!(index < BLOCK_SIZE);

        let bit_flag = 1_u64 << index;
        if block.head_cache.1 & bit_flag == 0 {
            // slow path, update cache
            block.head_cache.1 = block.tail_state.1.load(Acquire);
            return block.head_cache.1 & bit_flag == 0;
        }
        false
    }

    /// Returns the number of elements in the queue.
    ///
    /// ## Thread Safety and Reliability
    ///
    /// The returned value should be treated as a **heuristic approximation**:
    /// - **Single-threaded**: Results are reliable only in single-threaded contexts
    /// - **Multi-threaded (MPMC/MPSC)**: Values are instantaneous and may become stale immediately
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::ListQueue;
    ///
    /// let q = ListQueue::default();
    /// assert_eq!(q.len(), 0);
    ///
    /// q.push(10);
    /// assert_eq!(q.len(), 1);
    ///
    /// q.push(20);
    /// assert_eq!(q.len(), 2);
    /// ```
    pub fn len(&self) -> usize {
        let guard = self.head_id.lock();
        let head_id: usize = guard.1;
        // SAFETY: We hold the head lock, so `guard.0` points to a valid block.
        // `head_cache.0` is a `usize` field with no special bit-pattern
        // restrictions, so reading it through a raw pointer is safe.
        let head_index = unsafe { (&*guard.0).head_cache.0 };
        ::core::mem::drop(guard);

        let guard = self.tail_id.lock();
        let tail_id = guard.1;
        // SAFETY: We hold the tail lock, so `guard.0` points to a valid block.
        // `tail_state.0` is a `usize` field with no special restrictions.
        let tail_index = unsafe { (&*guard.0).tail_state.0 };
        ::core::mem::drop(guard);

        debug_assert!(tail_index >= head_index || tail_id != head_id);

        tail_id.wrapping_sub(head_id) * BLOCK_SIZE + tail_index - head_index
    }

    /// Acquires an exclusive lock for pop operations.
    ///
    /// This method returns a `PopLockGuard` that must be held while
    /// performing pop operations. The lock ensures exclusive access to
    /// the queue's head, preventing race conditions with other pop operations.
    ///
    /// # Example
    ///
    /// ```
    /// # use zlim_utils::sync::ListQueue;
    /// let queue = ListQueue::<i32>::default();
    ///
    /// let mut lock = queue.lock_pop();
    /// if let Some(value) = queue.pop_with_lock(&mut lock) {
    ///     // process value
    /// }
    /// ```
    #[inline]
    pub fn lock_pop(&self) -> PopLockGuard<'_, T> {
        PopLockGuard(self.head_id.lock())
    }

    /// Acquires an exclusive lock for push operations.
    ///
    /// This method returns a `PushLockGuard` that must be held while
    /// performing push operations. The lock ensures exclusive access to
    /// the queue's tail, preventing race conditions with other push operations.
    ///
    /// # Example
    ///
    /// ```
    /// # use zlim_utils::sync::ListQueue;
    /// let queue = ListQueue::default();
    ///
    /// let mut lock = queue.lock_push();
    /// queue.push_with_lock(&mut lock, 42);
    /// ```
    #[inline]
    pub fn lock_push(&self) -> PushLockGuard<'_, T> {
        PushLockGuard(self.tail_id.lock())
    }

    /// Pushes a value into the queue while holding a push lock.
    ///
    /// This is the low-level push operation that requires an already acquired
    /// push lock. The lock must be obtained via [`ListQueue::lock_push`].
    ///
    /// It's undefined behavior push self using other queue's lock.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use zlim_utils::sync::ListQueue;
    /// let queue = ListQueue::default();
    ///
    /// let mut lock = queue.lock_push();
    /// queue.push_with_lock(&mut lock, 42);
    /// ```
    pub fn push_with_lock(&self, lock_guard: &mut PushLockGuard<'_, T>, value: T) {
        // SAFETY: `lock_guard.0.0` is the tail block pointer from the held tail
        // lock guard, initialized in `ListQueue::new` and updated whenever the
        // tail advances to a new block. The lock guarantees mutual exclusion.
        let block = unsafe { &mut *lock_guard.0.0 };

        let index = block.tail_state.0;
        debug_assert!(index < BLOCK_SIZE);

        unsafe {
            ptr::write(block.slots.as_mut_ptr().add(index) as *mut T, value);
        }

        if index + 1 == BLOCK_SIZE {
            let new_block = self.idle.get();
            block.next = new_block;
            lock_guard.0.0 = new_block;
            lock_guard.0.1 = lock_guard.0.1.wrapping_add(1);
        }

        // Setting bit flag after setting `next` block.
        // Ensure that `next` ptr can be visited in `pop` function.
        block.tail_state.0 = index + 1;
        block.tail_state.1.fetch_or(1 << index, Release);
    }

    /// Pops a value from the queue while holding a pop lock.
    ///
    /// This is the low-level pop operation that requires an already acquired
    /// pop lock. The lock must be obtained via [`ListQueue::lock_pop`].
    ///
    /// It's undefined behavior pop self using other queue's lock.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use zlim_utils::sync::ListQueue;
    /// let queue = ListQueue::<i32>::default();
    ///
    /// let mut lock = queue.lock_pop();
    /// if let Some(value) = queue.pop_with_lock(&mut lock) {
    ///     // process value
    /// }
    /// ```
    pub fn pop_with_lock(&self, lock_guard: &mut PopLockGuard<'_, T>) -> Option<T> {
        // SAFETY: `lock_guard.0.0` is the head block pointer from the held head
        // lock guard, initialized in `ListQueue::new` and updated whenever the
        // head advances to the next block. The lock guarantees mutual exclusion.
        let block = unsafe { &mut *lock_guard.0.0 };

        let index = block.head_cache.0;
        debug_assert!(index < BLOCK_SIZE);

        let bit_flag = 1_u64 << index;
        if block.head_cache.1 & bit_flag == 0 {
            // slow path, update cache
            block.head_cache.1 = block.tail_state.1.load(Acquire);
            if block.head_cache.1 & bit_flag == 0 {
                return None;
            }
        }

        let value = unsafe { ptr::read(block.slots.as_ptr().add(index) as *mut T) };

        block.head_cache.0 = index + 1;

        if index + 1 == BLOCK_SIZE {
            let old_ptr = block as *mut Block<T>;
            let next_ptr = block.next;
            // index + 1 == BLOCK_SIZE, so tail_index == BLOCK_SIZE.
            // next_ptr must be set by `push` function.
            debug_assert!(!next_ptr.is_null());
            lock_guard.0.0 = next_ptr;
            lock_guard.0.1 = lock_guard.0.1.wrapping_add(1);

            self.idle.push(old_ptr);
        }

        Some(value)
    }
}

impl<T> fmt::Debug for ListQueue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("ListQueue { .. }")
    }
}

// ----------------------------------------------------------------------------
// Pop Guard

/// A lock representing the `pop` permissions of [`ListQueue`].
#[derive(Debug)]
#[repr(transparent)]
pub struct PopLockGuard<'a, T>(SpinLockGuard<'a, (*mut Block<T>, usize)>);

/// A lock representing the `push` permissions of [`ListQueue`].
#[derive(Debug)]
#[repr(transparent)]
pub struct PushLockGuard<'a, T>(SpinLockGuard<'a, (*mut Block<T>, usize)>);

// ----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::thread::scope;

    use super::ListQueue;

    #[test]
    fn smoke() {
        let q = ListQueue::default();
        q.push(7);
        assert_eq!(q.pop(), Some(7));

        q.push(8);
        assert_eq!(q.pop(), Some(8));
        assert!(q.pop().is_none());
    }

    #[test]
    fn len_empty_full() {
        let q = ListQueue::default();

        assert_eq!(q.len(), 0);
        assert!(q.is_empty());

        q.push(());

        assert_eq!(q.len(), 1);
        assert!(!q.is_empty());

        q.pop().unwrap();

        assert_eq!(q.len(), 0);
        assert!(q.is_empty());
    }

    #[test]
    fn len() {
        let q = ListQueue::default();

        assert_eq!(q.len(), 0);

        for i in 0..50 {
            q.push(i);
            assert_eq!(q.len(), i + 1);
        }

        for i in 0..50 {
            q.pop().unwrap();
            assert_eq!(q.len(), 50 - i - 1);
        }

        assert_eq!(q.len(), 0);
    }

    #[test]
    fn spsc() {
        #[cfg(miri)]
        const COUNT: usize = 50;
        #[cfg(not(miri))]
        const COUNT: usize = 100_000;

        let q = ListQueue::default();

        scope(|scope| {
            scope.spawn(|| {
                for i in 0..COUNT {
                    loop {
                        if let Some(x) = q.pop() {
                            assert_eq!(x, i);
                            break;
                        }
                    }
                }
                assert!(q.pop().is_none());
            });
            scope.spawn(|| {
                for i in 0..COUNT {
                    q.push(i);
                }
            });
        });
    }

    #[test]
    fn mpmc() {
        #[cfg(miri)]
        const COUNT: usize = 50;
        #[cfg(not(miri))]
        const COUNT: usize = 25_000;
        const THREADS: usize = 4;

        let q = ListQueue::<usize>::default();
        let v = (0..COUNT).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>();

        scope(|scope| {
            for _ in 0..THREADS {
                scope.spawn(|| {
                    for _ in 0..COUNT {
                        let n = loop {
                            if let Some(x) = q.pop() {
                                break x;
                            }
                        };
                        v[n].fetch_add(1, Ordering::SeqCst);
                    }
                });
            }
            for _ in 0..THREADS {
                scope.spawn(|| {
                    for i in 0..COUNT {
                        q.push(i);
                    }
                });
            }
        });

        for c in v {
            assert_eq!(c.load(Ordering::SeqCst), THREADS);
        }
    }
}
