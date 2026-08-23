//! Per-object thread-local storage using a bucket-based allocation strategy.
//!
//! Modified from Amanieu's ThreadLocal: <https://github.com/Amanieu/thread_local-rs/>.
//!
//! Per-thread objects are not destroyed when a thread exits. Instead, objects
//! are only destroyed when the `ThreadLocal` containing them is destroyed.
//!
//! Note that since thread IDs are recycled when a thread exits, it is possible
//! for one thread to retrieve the object of another thread. Since this can only
//! occur after a thread has exited this does not lead to any race conditions.
//!
//! See [`ThreadLocal`] documents for details.
#![expect(unsafe_code, reason = "Implementation of synchronization primitives")]

use core::cell::LazyCell;
use core::cmp::Reverse;
use core::fmt::Debug;
use core::iter::FusedIterator;
use core::ptr;
use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use core::sync::atomic::{AtomicPtr, AtomicUsize};
use std::collections::BinaryHeap;
use std::sync::{Mutex, OnceLock, PoisonError};

// -----------------------------------------------------------------------------
// Thread Index Allocation

/// A unique identifier for a thread, starting from 1.
///
/// The `Reverse` wrapper is used so that `BinaryHeap` can act as a min-heap
/// for the freelist, giving us the smallest available ID first.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
struct ThreadIndex(Reverse<usize>);

/// Global counter for allocating new thread IDs.
static ALLOCATOR: AtomicUsize = AtomicUsize::new(1);

/// Freelist of thread IDs that have been released by exited threads.
/// Using a min-heap ensures we reuse the smallest available ID first.
static FREE_LIST: Mutex<BinaryHeap<Reverse<usize>>> = Mutex::new(BinaryHeap::new());

impl Drop for ThreadIndex {
    /// When a thread exits, return its ID to the freelist for reuse.
    ///
    /// Because this feature, `ThreadIndex` cannot implement `Clone`.
    fn drop(&mut self) {
        FREE_LIST
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(self.0);
    }
}

impl ThreadIndex {
    /// Allocate a new thread ID, reusing a freed ID if available.
    fn alloc() -> Self {
        let opt_id = FREE_LIST
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop();

        if let Some(id) = opt_id {
            Self(id)
        } else {
            let id = ALLOCATOR.try_update(Relaxed, Relaxed, |a| a.checked_add(1));
            Self(Reverse(id.expect("too many threads")))
        }
    }

    /// Get the current thread's slot index.
    #[inline]
    fn get() -> usize {
        THREAD_INDEX.with(|x| x.0.0)
    }
}

thread_local! {
    /// Thread-local storage for the current thread's index.
    ///
    /// `LazyCell` ensures the index is allocated only once per thread,
    /// on first access. This keeps the fast path minimal.
    static THREAD_INDEX: LazyCell<ThreadIndex> = const { LazyCell::new(ThreadIndex::alloc) };
}

// -----------------------------------------------------------------------------
// ThreadLocal Storage

/// The maximum number of buckets. Each bucket can hold 2^n elements.
/// Total capacity: 2 ^ BUCKETS - 1 = usize::MAX - 1 entries.
const BUCKETS: usize = (usize::BITS - 1) as usize;

/// A slot storing a single thread-local value.
type Slot<T> = OnceLock<T>;

/// Runtime thread-local storage container.
///
/// This structure maintains a collection of thread-local values, organized
/// into buckets for efficient storage and access. Each thread gets a unique
/// slot index, and values are stored in the corresponding slot.
///
/// Unlike the standard [`thread_local!`] macro, [`ThreadLocal`] does not drop
/// data when a thread exits; data lives until the container itself is dropped.
///
/// Also, due to how `ThreadId` is implemented internally, thread-local data can
/// be transferred to other threads (thread A creates it, thread A despawned, then
/// a subsequent thread reuses A's ID). This maximizes memory reuse, but be mindful
/// of the semantics—reset the value explicitly on thread init if necessary.
///
/// It also provides an additional feature: you can iterate over the local values of
/// all threads via the [iter](ThreadLocal::iter) or [iter_mut](ThreadLocal::iter_mut) functions.
///
/// # Examples
///
/// Basic usage of `ThreadLocal`:
///
/// ```
/// # use zlim_utils::ext::ThreadLocal;
/// let tls: ThreadLocal<u32> = ThreadLocal::new();
/// assert_eq!(tls.get(), None);
///
/// assert_eq!(tls.get_or(|| 5), &5);
/// assert_eq!(tls.get(), Some(&5));
/// ```
///
/// Combining thread-local values into a single result:
///
/// ```
/// # use zlim_utils::ext::ThreadLocal;
/// # use std::cell::Cell;
/// # use std::thread;
/// let tls: ThreadLocal<Cell<i32>> = ThreadLocal::new();
///
/// // Create a bunch of threads to do stuff
/// thread::scope(|scope| {
///     for _ in 0..5 {
///         scope.spawn(|| {
///             // Increment a counter to count some event...
///             let cell = tls.get_or(|| Cell::new(0));
///             cell.set(cell.get() + 1);
///         });
///     }
/// });
///
/// // Once all threads are done, collect the counter values
/// // and return the sum of all thread-local counter values.
/// let total = tls.into_iter().fold(0, |x, y| x + y.get());
/// assert_eq!(total, 5);
/// ```
pub struct ThreadLocal<T: Send> {
    /// The maximum value of currently available slot index.
    ///
    /// Since the slot index starts from 1, `maximun == 0` indicates that there are no slots.
    ///
    /// The implementation guarantees that the field takes the
    /// form `0000011111` — all 0s on the left, all 1s on the right.
    ///
    /// - 0b0:  Default
    /// - 0b1:  Bucket(0) is initialized, Max SlotIndex is 1.
    /// - 0b11: Bucket(0) and Bucket(1) is initialized, Max SlotIndex is 3.
    ///
    /// So: Next BucketIndex (should init) == maximun.trailing_ones();
    maximun: AtomicUsize,

    /// Array of bucket pointers. Each bucket contains `2^n` slots.
    ///
    /// ```text
    /// [
    ///   Bucket(0): [Slot(1)],
    ///   Bucket(1): [Slot(2), Slot(3)],
    ///   Bucket(2): [Slot(4), Slot(5), Slot(6), Slot(7)],
    ///   ......
    /// ]
    /// ```
    ///
    /// - SlotIndex != 0
    /// - BucketIndex == (usize::BITS - 1) - SlotIndex.leading_zeros();
    /// - BucketSize == 1 << BucketIndex
    buckets: [AtomicPtr<Slot<T>>; BUCKETS],
}

/// Determines which bucket should be allocated next.
#[inline(always)] // The trails of maximun is always `1`.
const fn should_init(maximun: usize) -> usize {
    maximun.trailing_ones() as usize
}

/// Returns the size (number of slots) of a bucket.
#[inline(always)] // bucket_index <= usize::BITS - 1
const fn bucket_size(bucket_index: usize) -> usize {
    1 << bucket_index
}

/// Maps a slot index to its containing bucket index.
#[inline(always)] // slot_index <= isize::MAX
const fn bucket_index(slot_index: usize) -> usize {
    (usize::BITS - 1 - slot_index.leading_zeros()) as usize
}

// -----------------------------------------------------------------------------
// ThreadLocal Implementations

// SAFETY: ThreadLocal is always Sync, even if T isn't.
unsafe impl<T: Send> Sync for ThreadLocal<T> {}

impl<T: Send> Drop for ThreadLocal<T> {
    fn drop(&mut self) {
        // Drop all thread local value.
        self.clear();
        // Dealloc all bucket memory.
        self.dealloc();
    }
}

impl<T: Send> Default for ThreadLocal<T> {
    /// Creates a new, empty `ThreadLocal`.
    ///
    /// As same as [`ThreadLocal::new`].
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send> ThreadLocal<T> {
    /// Creates a new, empty `ThreadLocal`.
    ///
    /// No memory is allocated until the first thread accesses it.
    #[inline]
    pub const fn new() -> Self {
        Self {
            maximun: AtomicUsize::new(0),
            buckets: [const { AtomicPtr::new(ptr::null_mut()) }; BUCKETS],
        }
    }

    /// Creates a new `ThreadLocal` with pre-allocated capacity.
    ///
    /// This pre-allocates enough buckets to support `capacity` threads
    /// without further reallocation.
    ///
    /// The capacity is rounded up to the nearest power of two.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        let mut this = Self::new();
        this.alloc_mut(capacity);
        this
    }

    /// Clear all data.
    ///
    /// Note that this function does **not** release allocated memory.
    #[inline]
    pub fn clear(&mut self) {
        let maximun: usize = *self.maximun.get_mut();
        let mut index: usize = 1;
        while index <= maximun {
            let _ = self.entry_mut(index).take();
            index += 1;
        }
    }

    /// Deallocates all buckets and their slots.
    ///
    /// Note that this function merely deallocates memory
    /// and does not invoke Drop on the data.
    ///
    /// Be sure to call [`ThreadLocal::clear`] prior to invoking this function.
    fn dealloc(&mut self) {
        let mut maximun = *self.maximun.get_mut();
        while maximun != 0 {
            // `maximun` is the max slot index, so we can use it to get bucket_index.
            let bucket_index: usize = bucket_index(maximun);
            let bucket_size: usize = bucket_size(bucket_index);

            unsafe {
                let bucket_ptr: &mut AtomicPtr<Slot<T>> =
                    self.buckets.get_unchecked_mut(bucket_index);
                let head: *mut Slot<T> = *bucket_ptr.get_mut();
                *bucket_ptr.get_mut() = ptr::null_mut();
                // Pre resetting to avoid repeated dealloc during panic.
                maximun >>= 1;
                *self.maximun.get_mut() = maximun;
                // Dealloc memory
                let raw: *mut [Slot<T>] = ptr::slice_from_raw_parts_mut(head, bucket_size);
                let _drop: Box<[Slot<T>]> = Box::from_raw(raw);
            }
        }
    }

    /// Allocates buckets to support a given slot index (mutable version).
    #[cold]
    #[inline(never)]
    fn alloc_mut(&mut self, slot_index: usize) {
        let mut maximun = 0usize;

        while maximun < slot_index {
            let bucket_index: usize = should_init(maximun);
            let bucket_size: usize = bucket_size(bucket_index);

            let slots: Box<[Slot<T>]> = (0..bucket_size).map(|_| OnceLock::new()).collect();
            let ptr: *mut [Slot<T>] = Box::leak(slots);

            unsafe {
                let bucket_ptr: &mut AtomicPtr<Slot<T>> =
                    self.buckets.get_unchecked_mut(bucket_index);
                *bucket_ptr.get_mut() = ptr as *mut Slot<T>;
            }

            maximun = (*self.maximun.get_mut() << 1) + 1;
            *self.maximun.get_mut() = maximun;
        }
    }

    /// Allocates buckets to support a given slot index (shared version).
    #[cold]
    #[inline(never)]
    fn alloc(&self, slot_index: usize) {
        let mut maximun = self.maximun.load(Acquire);

        while maximun < slot_index {
            let bucket_index: usize = should_init(maximun);
            let bucket_size: usize = bucket_size(bucket_index);

            let slots: Box<[Slot<T>]> = (0..bucket_size).map(|_| OnceLock::new()).collect();
            let ptr: *mut [Slot<T>] = Box::leak(slots);
            let head: *mut Slot<T> = ptr as *mut Slot<T>;

            let bucket_ptr: &AtomicPtr<Slot<T>> =
                unsafe { self.buckets.get_unchecked(bucket_index) };

            let r = bucket_ptr
                .compare_exchange(ptr::null_mut(), head, Release, Relaxed)
                .is_ok();

            if r {
                let old = self.maximun.update(Release, Acquire, |o| (o << 1) | 1);
                maximun = (old << 1) | 1;
            } else {
                let _drop: Box<[Slot<T>]> = unsafe { Box::from_raw(ptr) };
                maximun = self.maximun.load(Acquire);
            }
        }
    }

    /// Returns a mutable reference to the slot at the given index.
    fn entry_mut(&mut self, slot_index: usize) -> &mut Slot<T> {
        let bucket_index = bucket_index(slot_index);
        // Bucket(0): [Slot(1)],
        // - slot_index = 0b01, offset = 0 = 0b01 ^ 0b01.
        // Bucket(1): [Slot(2), Slot(3)],
        // - slot_index = 0b10, offset = 0 = 0b10 ^ 0b10.
        // - slot_index = 0b11, offset = 1 = 0b11 ^ 0b10.
        let offset = slot_index ^ (1 << bucket_index);

        unsafe {
            let bucket: &mut AtomicPtr<Slot<T>> = self.buckets.get_unchecked_mut(bucket_index);
            let head: *mut Slot<T> = *bucket.get_mut();
            if !head.is_null() {
                return &mut *head.add(offset);
            }
            self.alloc_mut(slot_index);
            let bucket: &mut AtomicPtr<Slot<T>> = self.buckets.get_unchecked_mut(bucket_index);
            let head: *mut Slot<T> = *bucket.get_mut();
            &mut *(head.add(offset))
        }
    }

    /// Returns a shared reference to the slot at the given index.
    fn entry(&self, slot_index: usize) -> &Slot<T> {
        let bucket_index = bucket_index(slot_index);
        // Bucket(0): [Slot(1)],
        // - slot_index = 0b01, offset = 0 = 0b01 ^ 0b01.
        // Bucket(1): [Slot(2), Slot(3)],
        // - slot_index = 0b10, offset = 0 = 0b10 ^ 0b10.
        // - slot_index = 0b11, offset = 1 = 0b11 ^ 0b10.
        let offset = slot_index ^ (1 << bucket_index);

        unsafe {
            let bucket: &AtomicPtr<Slot<T>> = self.buckets.get_unchecked(bucket_index);
            let head = bucket.load(Acquire);
            if !head.is_null() {
                return &*head.add(offset);
            }
            self.alloc(slot_index);
            &*bucket.load(Acquire).add(offset)
        }
    }

    /// Returns the value for the current thread, if it exists.
    ///
    /// # Example
    ///
    /// ```
    /// # use zlim_utils::ext::ThreadLocal;
    /// let tls = ThreadLocal::<i32>::new();
    /// assert_eq!(tls.get(), None);
    /// ```
    pub fn get(&self) -> Option<&T> {
        let slot_index = ThreadIndex::get();
        self.entry(slot_index).get()
    }

    /// Returns the value for the current thread, creating it if necessary.
    ///
    /// # Example
    ///
    /// ```
    /// # use zlim_utils::ext::ThreadLocal;
    /// let tls = ThreadLocal::<i32>::new();
    /// let value = tls.get_or(|| 42);
    /// assert_eq!(value, &42);
    ///
    /// // Subsequent calls return the same value
    /// let value2 = tls.get_or(|| 100);
    /// assert_eq!(value2, &42);
    /// ```
    ///
    /// # Deadlock
    ///
    /// The `get_or` function behaves similarly to [`OnceLock::get_or_init`].
    ///
    /// It is an error to reentrantly initialize the cell from `f`. Current
    /// implementation deadlocks, but this may be changed to a panic in the future.
    ///
    /// ```no_run
    /// # use zlim_utils::ext::ThreadLocal;
    /// let tls: ThreadLocal<i32> = ThreadLocal::new();
    ///
    /// let value = tls.get_or(|| {
    ///     // nesting `get`, deadlock may occured.
    ///     let _ = tls.get();
    ///     1
    /// });
    /// ```
    pub fn get_or(&self, f: impl FnOnce() -> T) -> &T {
        let slot_index = ThreadIndex::get();
        self.entry(slot_index).get_or_init(f)
    }

    /// Returns an iterator over all thread-local values.
    ///
    /// Note that the iterator's length is fixed at the moment of its creation.
    /// Any thread-local data that comes into existence after the iterator has
    /// been created **may** not be reflected in the iteration.
    ///
    /// # Example
    ///
    /// ```
    /// # use zlim_utils::ext::ThreadLocal;
    /// let tls = ThreadLocal::new();
    /// // ... populate from multiple threads ...
    /// let total: i32 = tls.iter().sum();
    /// ```
    #[inline]
    pub fn iter(&self) -> Iter<'_, T> {
        let maximun = self.maximun.load(Acquire);
        Iter {
            data: self,
            maximun,
            index: 1,
        }
    }

    /// Returns a mutable iterator over all thread-local values.
    ///
    /// Since this borrows `self` mutably, it guarantees that no other threads
    /// are accessing the values, allowing safe mutation and faster accession.
    ///
    /// # Example
    ///
    /// ```
    /// # use zlim_utils::ext::ThreadLocal;
    /// let mut tls = ThreadLocal::<i32>::new();
    /// // ... populate ...
    /// for value in tls.iter_mut() {
    ///     *value += 1;
    /// }
    /// ```
    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        let maximun = *self.maximun.get_mut();
        IterMut {
            data: self,
            maximun,
            index: 1,
        }
    }
}

impl<T: Send + Default> ThreadLocal<T> {
    /// Returns the value for the current thread, or creates a default one.
    ///
    /// # Example
    ///
    /// ```
    /// # use zlim_utils::ext::ThreadLocal;
    /// let tls: ThreadLocal<u32> = ThreadLocal::new();
    /// assert_eq!(tls.get_or_default(), &0);
    /// ```
    pub fn get_or_default(&self) -> &T {
        let slot_index = ThreadIndex::get();
        self.entry(slot_index).get_or_init(T::default)
    }
}

impl<T: Send + Debug> Debug for ThreadLocal<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ThreadLocal {{ local_data: {:?} }}", self.get())
    }
}

// -----------------------------------------------------------------------------
// IntoIterator Implementations

impl<T: Send> IntoIterator for ThreadLocal<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    #[inline]
    fn into_iter(mut self) -> Self::IntoIter {
        let maximun = *self.maximun.get_mut();
        IntoIter {
            data: self,
            maximun,
            index: 1,
        }
    }
}

impl<'a, T: Send> IntoIterator for &'a ThreadLocal<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T: Send> IntoIterator for &'a mut ThreadLocal<T> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

// -----------------------------------------------------------------------------
// Iterators

/// An iterator that moves values out of a `ThreadLocal`.
#[derive(Debug)]
pub struct IntoIter<T: Send> {
    data: ThreadLocal<T>,
    maximun: usize,
    index: usize,
}

impl<T: Send> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        while self.index <= self.maximun {
            let r: Option<T> = self.data.entry_mut(self.index).take();
            self.index += 1;
            if r.is_some() {
                return r;
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some((self.maximun + 1).saturating_sub(self.index)))
    }
}

/// An iterator over shared references to thread-local values.
///
/// Note that the iterator's length is fixed at the moment of its creation.
/// Any thread-local data that comes into existence after the iterator has
/// been created **may** not be reflected in the iteration.
#[derive(Debug)]
pub struct Iter<'a, T: Send> {
    data: &'a ThreadLocal<T>,
    maximun: usize,
    index: usize,
}

impl<'a, T: Send> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        while self.index <= self.maximun {
            let entry = self.data.entry(self.index);
            let r = entry.get();
            self.index += 1;
            if r.is_some() {
                return r;
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some((self.maximun + 1).saturating_sub(self.index)))
    }
}

/// A mutable iterator over thread-local values.
#[derive(Debug)]
pub struct IterMut<'a, T: Send> {
    data: &'a mut ThreadLocal<T>,
    maximun: usize,
    index: usize,
}

impl<'a, T: Send> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        // SAFETY:
        // We use a raw pointer to `self.data` to work around the borrow
        // checker: `entry_mut` borrows `self.data` mutably, and we need
        // to call it multiple times in the loop. Each iteration accesses
        // a *different* slot (`self.index` is strictly increasing), so
        // the mutable references returned by `entry_mut` never alias.
        // The `&'a mut ThreadLocal<T>` in `self.data` guarantees that
        // no other code can observe the aliasing during iteration.
        unsafe {
            let ptr: *mut ThreadLocal<T> = self.data as *mut ThreadLocal<T>;
            while self.index <= self.maximun {
                let entry = (*ptr).entry_mut(self.index);
                let r = entry.get_mut();
                self.index += 1;
                if r.is_some() {
                    return r;
                }
            }
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some((self.maximun + 1).saturating_sub(self.index)))
    }
}

impl<T: Send> FusedIterator for IterMut<'_, T> {}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::ThreadLocal;

    use core::cell::RefCell;
    use core::sync::atomic::AtomicUsize;
    use core::sync::atomic::Ordering::Relaxed;
    use std::sync::Arc;
    use std::thread;

    fn make_create() -> Arc<dyn Fn() -> usize + Send + Sync> {
        let count = AtomicUsize::new(0);
        Arc::new(move || count.fetch_add(1, Relaxed))
    }

    #[test]
    fn same_thread() {
        let create = make_create();
        let mut tls = ThreadLocal::new();
        assert_eq!(None, tls.get());
        assert_eq!("ThreadLocal { local_data: None }", format!("{:?}", tls));
        assert_eq!(0, *tls.get_or(|| create()));
        assert_eq!(Some(&0), tls.get());
        assert_eq!(0, *tls.get_or(|| create()));
        assert_eq!(Some(&0), tls.get());
        assert_eq!(0, *tls.get_or(|| create()));
        assert_eq!(Some(&0), tls.get());
        assert_eq!("ThreadLocal { local_data: Some(0) }", format!("{:?}", tls));
        tls.clear();
        assert_eq!(None, tls.get());
    }

    #[test]
    fn different_thread() {
        let create = make_create();
        let tls = Arc::new(ThreadLocal::new());
        assert_eq!(None, tls.get());
        assert_eq!(0, *tls.get_or(|| create()));
        assert_eq!(Some(&0), tls.get());

        let tls2 = tls.clone();
        let create2 = create.clone();
        thread::spawn(move || {
            assert_eq!(None, tls2.get());
            assert_eq!(1, *tls2.get_or(|| create2()));
            assert_eq!(Some(&1), tls2.get());
        })
        .join()
        .unwrap();

        assert_eq!(Some(&0), tls.get());
        assert_eq!(0, *tls.get_or(|| create()));
    }

    #[test]
    fn iter() {
        let tls = Arc::new(ThreadLocal::new());
        tls.get_or(|| Box::new(1));

        let tls2 = tls.clone();
        thread::spawn(move || {
            tls2.get_or(|| Box::new(2));
            let tls3 = tls2.clone();
            thread::spawn(move || {
                tls3.get_or(|| Box::new(3));
            })
            .join()
            .unwrap();
            drop(tls2);
        })
        .join()
        .unwrap();

        let mut tls = Arc::try_unwrap(tls).unwrap();

        let mut v = tls.iter().map(|x| **x).collect::<Vec<i32>>();
        v.sort_unstable();
        assert_eq!(vec![1, 2, 3], v);

        let mut v = tls.iter_mut().map(|x| **x).collect::<Vec<i32>>();
        v.sort_unstable();
        assert_eq!(vec![1, 2, 3], v);

        let mut v = tls.into_iter().map(|x| *x).collect::<Vec<i32>>();
        v.sort_unstable();
        assert_eq!(vec![1, 2, 3], v);
    }

    #[test]
    fn test_drop() {
        let local = ThreadLocal::new();
        struct Dropped(Arc<AtomicUsize>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.fetch_add(1, Relaxed);
            }
        }

        let dropped = Arc::new(AtomicUsize::new(0));
        local.get_or(|| Dropped(dropped.clone()));
        assert_eq!(dropped.load(Relaxed), 0);
        drop(local);
        assert_eq!(dropped.load(Relaxed), 1);
    }

    #[test]
    fn is_sync() {
        fn foo<T: Sync>() {}
        foo::<ThreadLocal<String>>();
        foo::<ThreadLocal<RefCell<String>>>();
    }
}
