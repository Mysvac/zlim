use core::sync::atomic::{AtomicBool, Ordering};

// -----------------------------------------------------------------------------
// OnceFlag

/// Wrapper around an [`AtomicBool`](core::sync::atomic::AtomicBool).
///
/// This is usually faster than [`Once`](std::sync::Once)
/// and generates smaller code.
///
/// But it only ensures that the expression is executed once,
/// does not guarantee that other threads can see the result
/// of the expression in concurrent scenarios.  
///
/// (In other words, other threads may see `flag = false`
/// while the processing of the expression itself has not yet completed.)
///
/// # Example
///
/// ```
/// use zlim_utils::sync::OnceFlag;
///
/// let flag = OnceFlag::new();
/// let mut count = 0;
///
/// for _ in 0..5 {
///     if flag.set() {
///         count += 1;
///     }
/// }
/// assert_eq!(count, 1);
/// # // test
/// # let flag = OnceFlag::default();
/// # for _ in 0..5 {
/// #     if flag.set() {
/// #         count += 1;
/// #     }
/// # }
/// # assert_eq!(count, 2);
/// ```
#[repr(transparent)]
pub struct OnceFlag(AtomicBool);

impl OnceFlag {
    /// Create new object, default inner value is `true`.
    #[inline]
    pub const fn new() -> Self {
        Self(AtomicBool::new(true))
    }

    /// Set inner value to `false` and return old value.
    #[inline]
    pub fn set(&self) -> bool {
        self.0.swap(false, Ordering::Relaxed)
    }
}

impl Default for OnceFlag {
    /// Call `new`, default inner value is `true`.
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
