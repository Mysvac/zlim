#![expect(unsafe_code, reason = "SpinLock requires unsafe code.")]

use core::cell::UnsafeCell;
use core::fmt;
use core::ops::{Deref, DerefMut};
use core::panic::{RefUnwindSafe, UnwindSafe};

use super::futex::Futex;

// -----------------------------------------------------------------------------
// SpinLock

/// A mutual exclusion primitive useful for protecting shared data.
///
/// Which will block the current thread until the lock is acquired.
/// Similar to `Mutex`, but this is always busy-waiting.
///
/// # Examples
///
/// ```
/// use std::{sync::Arc, thread};
/// use zlim_utils::sync::SpinLock;
///
/// let vec = Arc::new(SpinLock::new(Vec::new()));
///
/// thread::scope(|s|{
///     for _ in 0..100 {
///         s.spawn(|| vec.lock().push(1));
///     }
/// });
///
/// assert_eq!(vec.lock().len(), 100);
/// ```
pub struct SpinLock<T: ?Sized> {
    futex: Futex,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for SpinLock<T> {}
unsafe impl<T: ?Sized + Send> Sync for SpinLock<T> {}
impl<T: ?Sized> UnwindSafe for SpinLock<T> {}
impl<T: ?Sized> RefUnwindSafe for SpinLock<T> {}

impl<T> SpinLock<T> {
    /// Creates a new spin-lock in an unlocked state ready for use.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::SpinLock;
    ///
    /// let mutex = SpinLock::new(0);
    /// ```
    #[inline]
    pub const fn new(t: T) -> Self {
        SpinLock {
            futex: Futex::new(),
            data: UnsafeCell::new(t),
        }
    }
}

impl<T: ?Sized> SpinLock<T> {
    /// Acquires a lock guard, blocking the current thread until it is able to do so.
    ///
    /// Uses exponential backoff with [`Backoff`](super::Backoff) to reduce
    /// contention under high pressure. When contention is expected to be low,
    /// consider using [`quick_lock`](Self::quick_lock) instead.
    #[inline]
    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        self.futex.lock();
        SpinLockGuard { lock: self }
    }

    /// Acquires a lock guard, blocking the current thread until it is able to do so.
    ///
    /// Unlike [`lock`](Self::lock), this variant uses only
    /// [`core::hint::spin_loop`] without exponential backoff or thread yielding,
    /// making it suitable for very short critical sections with low contention.
    #[inline]
    pub fn quick_lock(&self) -> SpinLockGuard<'_, T> {
        self.futex.quick_lock();
        SpinLockGuard { lock: self }
    }

    /// Returns `true` if the lock is currently held.
    ///
    /// This is an instantaneous observation and may become outdated immediately
    /// in concurrent code.
    pub fn is_locked(&self) -> bool {
        self.futex.is_locked()
    }

    /// Attempts to acquire this lock.
    ///
    /// If the lock could not be acquired at this time, then [`None`] is returned.
    ///
    /// Otherwise, an RAII guard is returned. The lock will be unlocked when the
    /// guard is dropped.
    #[inline]
    pub fn try_lock(&self) -> Option<SpinLockGuard<'_, T>> {
        if self.futex.try_lock() {
            Some(SpinLockGuard { lock: self })
        } else {
            None
        }
    }

    /// Consumes this spin-lock, returning the underlying data.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::SpinLock;
    ///
    /// let spin = SpinLock::new(0);
    /// assert_eq!(spin.into_inner(), 0);
    /// ```
    #[inline(always)]
    pub fn into_inner(self) -> T
    where
        T: Sized,
    {
        self.data.into_inner()
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::SpinLock;
    ///
    /// let mut lock = SpinLock::new(0);
    /// *lock.get_mut() = 10;
    /// assert_eq!(*lock.lock(), 10);
    /// ```
    #[inline]
    pub fn get_mut(&mut self) -> &mut T {
        // SAFETY: We hold `&mut self`, so no other reference to the data
        // can exist. The borrow checker guarantees exclusive access.
        unsafe { &mut *self.data.get() }
    }
}

impl<T> From<T> for SpinLock<T> {
    /// Creates a new mutex in an unlocked state ready for use.
    /// This is equivalent to [`SpinLock::new`].
    #[inline]
    fn from(t: T) -> Self {
        SpinLock::new(t)
    }
}

impl<T: Default> Default for SpinLock<T> {
    /// Creates a `SpinLock<T>`, with the `Default` value for T.
    #[inline]
    fn default() -> SpinLock<T> {
        SpinLock::new(Default::default())
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for SpinLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("SpinLock");
        match self.try_lock() {
            Some(guard) => {
                d.field("data", &&*guard);
            }
            None => {
                d.field("data", &"<locked>");
            }
        }
        d.finish_non_exhaustive()
    }
}

// -----------------------------------------------------------------------------
// SpinLockGuard

/// An RAII implementation of a "scoped lock" of a spin-lock.
///
/// When this structure is dropped (falls out of scope), the lock will be unlocked.
pub struct SpinLockGuard<'a, T: ?Sized + 'a> {
    lock: &'a SpinLock<T>,
}

// !Send
// SAFETY: SpinLockGuard is Sync when T: Sync because it provides
// read-only access to T via Deref, and T: Sync ensures safe shared
// reads. The guard holds the lock so no writer races against reads.
unsafe impl<T: ?Sized + Sync> Sync for SpinLockGuard<'_, T> {}
impl<T: UnwindSafe + ?Sized> UnwindSafe for SpinLockGuard<'_, T> {}
impl<T: RefUnwindSafe + ?Sized> RefUnwindSafe for SpinLockGuard<'_, T> {}

impl<T: ?Sized> Deref for SpinLockGuard<'_, T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &T {
        // SAFETY: The SpinLockGuard holds the lock, providing exclusive
        // access to the data. No other thread can mutate T concurrently.
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T: ?Sized> DerefMut for SpinLockGuard<'a, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: The SpinLockGuard holds the lock, providing exclusive
        // access to the data. The `&mut self` on the guard ensures no
        // other reference to the guard or its data exists.
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T: ?Sized> Drop for SpinLockGuard<'a, T> {
    /// The dropping of the SpinLockGuard will release the lock it was created from.
    #[inline]
    fn drop(&mut self) {
        self.lock.futex.unlock();
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for SpinLockGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for SpinLockGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::sync::mpsc::channel;
    use std::thread;

    use super::SpinLock;

    #[test]
    fn smoke() {
        let m = SpinLock::new(());
        drop(m.lock());
        drop(m.lock());
    }

    #[test]
    fn lots_and_lots() {
        const J: u32 = 1000;
        const K: u32 = 5;

        let m = Arc::new(SpinLock::new(0));

        fn inc(m: &SpinLock<u32>) {
            for _ in 0..J {
                *m.lock() += 1;
            }
        }

        let (tx, rx) = channel();
        for _ in 0..K {
            let tx2 = tx.clone();
            let m2 = m.clone();
            thread::spawn(move || {
                inc(&m2);
                tx2.send(()).unwrap();
            });
            let tx2 = tx.clone();
            let m2 = m.clone();
            thread::spawn(move || {
                inc(&m2);
                tx2.send(()).unwrap();
            });
        }

        drop(tx);
        for _ in 0..2 * K {
            rx.recv().unwrap();
        }
        assert_eq!(*m.lock(), J * K * 2);
    }

    #[test]
    fn try_lock() {
        let m = SpinLock::new(());
        *m.try_lock().unwrap() = ();
    }

    #[test]
    fn test_into_inner_drop() {
        struct Foo(Arc<AtomicUsize>);
        impl Drop for Foo {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let num_drops = Arc::new(AtomicUsize::new(0));
        let m = SpinLock::new(Foo(num_drops.clone()));
        assert_eq!(num_drops.load(Ordering::SeqCst), 0);
        {
            let _inner = m.into_inner();
            assert_eq!(num_drops.load(Ordering::SeqCst), 0);
        }
        assert_eq!(num_drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_mutex_arc_nested() {
        // Tests nested mutexes and access
        // to underlying data.
        let arc = Arc::new(SpinLock::new(1));
        let arc2 = Arc::new(SpinLock::new(arc));
        let (tx, rx) = channel();
        let _t = thread::spawn(move || {
            let lock = arc2.lock();
            let lock2 = lock.lock();
            assert_eq!(*lock2, 1);
            tx.send(()).unwrap();
        });
        rx.recv().unwrap();
    }

    #[test]
    fn test_mutex_unsized() {
        let mutex: &SpinLock<[i32]> = &SpinLock::new([1, 2, 3]);
        {
            let b = &mut *mutex.lock();
            b[0] = 4;
            b[2] = 5;
        }
        let comp: &[i32] = &[4, 2, 5];
        assert_eq!(&*mutex.lock(), comp);
    }

    #[cfg(panic = "unwind")]
    mod helper {
        use core::any::Any;
        use core::panic::AssertUnwindSafe;
        use core::sync::atomic;
        use std::boxed::Box;
        use std::{panic, thread};

        pub fn test_unwind_panic<R>(f: impl FnOnce() -> R) -> Result<R, Box<dyn Any + Send>> {
            let prev_hook = panic::take_hook();
            panic::set_hook(Box::new(|_| {}));
            let result = panic::catch_unwind(AssertUnwindSafe(f));
            panic::set_hook(prev_hook);
            result
        }

        pub fn test_thread_panic<F, T>(f: F) -> Result<T, Box<dyn Any + Send>>
        where
            F: FnOnce() -> T,
            F: Send + 'static,
            T: Send + 'static,
        {
            let prev_hook = panic::take_hook();
            panic::set_hook(Box::new(|_| {}));
            atomic::fence(atomic::Ordering::SeqCst);
            let result = thread::spawn(f).join();
            panic::set_hook(prev_hook);
            result
        }
    }

    #[cfg(panic = "unwind")] // Requires unwinding support.
    #[test]
    fn test_panics() {
        use helper::test_unwind_panic;

        let spin = SpinLock::new(42);

        let catch_unwind_result1 = test_unwind_panic(|| {
            let _guard1 = spin.lock();

            panic!("test panic with spin once");
        });
        assert!(catch_unwind_result1.is_err());

        let catch_unwind_result2 = test_unwind_panic(|| {
            let _guard2 = spin.lock();

            panic!("test panic with spin twice");
        });
        assert!(catch_unwind_result2.is_err());

        let catch_unwind_result3 = test_unwind_panic(|| {
            let _guard3 = spin.lock();

            panic!("test panic with spin thrice");
        });
        assert!(catch_unwind_result3.is_err());
    }

    #[cfg(panic = "unwind")] // Requires unwinding support.
    #[test]
    fn test_mutex_arc_access_in_unwind() {
        use helper::test_thread_panic;

        let arc = Arc::new(SpinLock::new(1));
        let arc2 = arc.clone();

        let _ = test_thread_panic(move || -> () {
            struct Unwinder {
                i: Arc<SpinLock<i32>>,
            }
            impl Drop for Unwinder {
                fn drop(&mut self) {
                    *self.i.lock() += 1;
                }
            }
            let _u = Unwinder { i: arc2 };
            panic!();
        });

        let lock = arc.lock();
        assert_eq!(*lock, 2);
    }
}
