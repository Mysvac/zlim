use core::cell::Cell;
use core::hash::BuildHasher;
use core::sync::atomic::{AtomicUsize, Ordering};

use std::hash::RandomState;

// ----------------------------------------------------------------------------
// XorShift64Star

const FIXED_STATE: u64 = 0x9a7013f475bb8c23;

/// [xorshift*] is a fast pseudorandom number generator which will
/// even tolerate weak seeding, as long as it's not zero.
///
/// [xorshift*]: https://en.wikipedia.org/wiki/Xorshift#xorshift*
#[repr(transparent)]
pub(super) struct XorShift64Star {
    state: Cell<u64>,
}

impl XorShift64Star {
    /// Return `XorShift64Star` with fixed seed.
    /// 
    /// Typically used to initialize in constant context.
    pub const fn fixed() -> Self {
        Self { state: Cell::new(FIXED_STATE) }
    }

    /// Randomize internal state.
    pub fn randomize(&self) {
        static COUNTER: AtomicUsize = AtomicUsize::new(1);

        // Any non-zero seed will do -- this uses the hash of a global counter.
        let mut seed = 0;
        let rs = RandomState::new();
        while seed == 0 {
            // DefaultHasher
            seed = rs.hash_one(COUNTER.fetch_add(1, Ordering::Relaxed))
        }

        self.state.set(seed);
    }

    #[inline]
    fn next(&self) -> u64 {
        let mut x = self.state.get();
        debug_assert_ne!(x, 0);
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state.set(x);
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// Return a value from `0..n`.
    #[inline]
    pub fn next_usize(&self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}
