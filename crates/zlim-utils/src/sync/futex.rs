use super::Backoff;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};

/// A user level spin-lock without any resources.
///
/// # Examples
///
/// ```ignore
/// use core::cell::Cell;
/// use zlim_utils::sync::Futex;
///
/// struct Foo {
///     data: Cell<i32>,
///     futex: Futex,
/// }
///
/// // thread safe
/// impl Foo {
///     fn get(&self) -> i32 {
///         self.futex.lock();
///         let v = self.data.get();
///         self.futex.unlock();
///         v
///     }
///     fn set(&self, value: i32) {
///         self.futex.lock();
///         self.data.set(value);
///         self.futex.unlock();
///     }
/// }
/// ```
#[repr(transparent)]
pub(super) struct Futex {
    state: AtomicBool,
}

impl Futex {
    #[expect(
        clippy::declare_interior_mutable_const,
        reason = "const used as a default/empty sentinel value"
    )]
    pub const UNLOCKED: Self = Self {
        state: AtomicBool::new(false),
    };

    /// Return `true` if futex is locked.
    #[inline(always)]
    pub fn is_locked(&self) -> bool {
        self.state.load(Acquire)
    }

    /// Try to lock self.
    ///
    /// - Return `true` if lock self successfully.
    /// - Return `false` if this futex has already been locked by other.
    ///
    /// This function will not block the thread, regardless of its state before unlocking.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use zlim_utils::sync::Futex;
    /// let futex = Futex::new();
    ///
    /// assert_eq!(futex.try_lock(), true);
    /// assert_eq!(futex.try_lock(), false);
    ///
    /// futex.unlock();
    /// assert_eq!(futex.try_lock(), true);
    /// ```
    #[inline]
    pub fn try_lock(&self) -> bool {
        self.state
            .compare_exchange(false, true, Acquire, Relaxed)
            .is_ok()
    }

    /// Lock self and busy waiting until it's successful.
    ///
    /// Unlike [`Futex::lock`], this function will continuously check state.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use zlim_utils::sync::Futex;
    /// let futex = Futex::new();
    ///
    /// futex.quick_lock();
    ///
    /// # assert!( futex.is_locked() );
    /// // do something
    ///
    /// futex.unlock();
    /// ```
    #[inline]
    pub fn quick_lock(&self) {
        loop {
            if self.try_lock() {
                return;
            }

            while self.state.load(Relaxed) {
                core::hint::spin_loop();
            }
        }
    }

    /// Lock self and busy waiting until it's successful.
    ///
    /// When multiple attempts are unsuccessful,
    /// this will perform some additional spin-loop to reduce atomic operation overhead.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use zlim_utils::sync::Futex;
    /// let futex = Futex::new();
    ///
    /// futex.lock();
    ///
    /// # assert!( futex.is_locked() );
    /// // do something
    ///
    /// futex.unlock();
    /// ```
    #[inline]
    pub fn lock(&self) {
        let backoff = Backoff::new();
        loop {
            if self.try_lock() {
                return;
            }

            while self.state.load(Relaxed) {
                backoff.snooze();
            }
        }
    }

    /// Force unlock a futex.
    ///
    /// This function will not block the thread, regardless of its state before unlocking.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use zlim_utils::sync::Futex;
    /// let futex = Futex::new();
    ///
    /// futex.unlock();
    /// assert!( !futex.is_locked() );
    ///
    /// futex.lock();
    /// assert!( futex.is_locked() );
    ///
    /// futex.unlock();
    /// assert!( !futex.is_locked() );
    /// ```
    #[inline(always)]
    pub fn unlock(&self) {
        self.state.store(false, Release);
    }
}
