use core::cell::Cell;
use core::fmt::{Debug, Formatter};

/// The maximum exponent of spin count.
const SPIN_LIMIT: u32 = 5;
const SNOOZE_LIMIT: u32 = 4;

/// Performs exponential backoff in spin loops.
///
/// Backing off in spin loops reduces contention and
/// improves overall performance.
///
/// This primitive can execute *YIELD* and *PAUSE* instructions,
/// yield the current thread to the OS scheduler, and tell when
/// is a good time to block the thread using a different synchronization
/// mechanism. Each step of the back off procedure takes roughly
/// twice as long as the previous step.
#[repr(transparent)]
pub struct Backoff {
    step: Cell<u32>,
}

impl Backoff {
    /// Creates a new `Backoff`.
    #[inline(always)]
    pub const fn new() -> Self {
        Self { step: Cell::new(0) }
    }

    /// Backs off in a lock-free loop.
    ///
    /// This method should be used when we need to retry an operation because another thread made
    /// progress.
    ///
    /// The processor may yield using the *YIELD* or *PAUSE* instruction.
    #[inline]
    pub fn spin(&self) {
        let step: u32 = 1 << self.step.get();
        for _ in 0..step {
            core::hint::spin_loop();
        }

        if self.step.get() < SPIN_LIMIT {
            self.step.update(|v| v + 1);
        }
    }

    /// Backs off in a blocking loop.
    ///
    /// This method should be used when we need to wait for another thread to make progress.
    ///
    /// The processor may yield using the *YIELD* or *PAUSE* instruction and the current thread
    /// may yield by giving up a timeslice to the OS scheduler.
    #[inline]
    pub fn snooze(&self) {
        if self.step.get() < SNOOZE_LIMIT {
            let step: u32 = 1 << { self.step.get() << 1 };

            for _ in 0..step {
                core::hint::spin_loop();
            }

            self.step.update(|v| v + 1);
        } else {
            std::thread::yield_now();
        }
    }
}

impl Debug for Backoff {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Backoff").field("step", &self.step).finish()
    }
}

impl Default for Backoff {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}
