//! Provides `FixedHasher`, `NoopHasher` and `SparseHasher`.

use core::fmt::Debug;
use core::hash::{BuildHasher, Hasher};

use foldhash::SharedSeed;
use foldhash::fast::FoldHasher;

// ----------------------------------------------------------------------------
// FixedHasher

/// A fixed hasher whose hash results depend only on the input.
///
/// It's a type alias for [`foldhash::fast::FoldHasher`].
///
/// It can be created through the [`FixedState::build_hasher`] function
/// or copied from the [`FixedState::HASHER`] constant.
///
/// See the [`FixedState`] docs for more information.
pub type FixedHasher = FoldHasher<'static>;

/// Fixed Hash State based upon a random but fixed seed.
///
/// Based on `foldhash` crate, but changed the fixed seed.
///
/// # Examples
///
/// ```
/// use core::hash::{Hash, Hasher, BuildHasher};
/// use zlim_utils::hash::FixedState;
///
/// let mut hasher = FixedState.build_hasher();
/// 3.hash(&mut hasher);
/// let result = hasher.finish();
///
/// println!("Hash Result {result}"); // Fixed Result
/// ```
#[derive(Copy, Clone, Default, Debug)]
pub struct FixedState;

impl FixedState {
    /// A constant hasher instance with a fixed seed.
    ///
    /// This is the compile-time equivalent of `FixedState::build_hasher()`,
    /// providing a deterministic hasher that can be used in const contexts.
    ///
    /// The seed (`0x9e3779b97f4a7c15`) has been empirically evaluated
    /// and demonstrates good distribution characteristics.
    pub const HASHER: FixedHasher =
        FixedHasher::with_seed(0x9e3779b97f4a7c15, SharedSeed::global_fixed());
}

impl BuildHasher for FixedState {
    type Hasher = FixedHasher;

    #[inline(always)]
    fn build_hasher(&self) -> Self::Hasher {
        Self::HASHER
    }
}

// ----------------------------------------------------------------------------
// NoopHasher

/// A no-op hasher that passes the value straight through as a `u64`.
///
/// It can be created through the [`NoopState::build_hasher`] function
/// or copied from the [`NoopState::HASHER`] constant.
///
/// See the [`NoopState`] docs for more information.
#[derive(Copy, Clone, Debug)]
#[repr(transparent)]
pub struct NoopHasher {
    hash: u64,
}

impl Hasher for NoopHasher {
    #[inline(always)]
    fn finish(&self) -> u64 {
        self.hash
    }

    #[inline(always)]
    fn write_usize(&mut self, i: usize) {
        self.hash = i as u64;
    }

    #[inline(always)]
    fn write_u64(&mut self, i: u64) {
        self.hash = i;
    }

    #[inline(always)]
    fn write_u32(&mut self, i: u32) {
        self.hash = i as u64;
    }

    #[inline(always)]
    fn write_u16(&mut self, i: u16) {
        self.hash = i as u64;
    }

    #[inline(always)]
    fn write_u8(&mut self, i: u8) {
        self.hash = i as u64;
    }

    fn write(&mut self, bytes: &[u8]) {
        // Usually recommended to use `write_u64` directly
        for byte in bytes.iter().rev() {
            // rotate left ensure that `write_u32(10)` is eq to `write_u64(10)`.
            self.hash = self.hash.rotate_left(8).wrapping_add(*byte as u64);
        }
    }
}

/// A fixed hasher without any additional operations.
///
/// It stores a single `u64` and assigns values directly via `write_u64`.
///
/// Other methods call `write`, which adds the input bytes to the `u64` in
/// reverse order and rotates it left. This ensures the results of
/// `write_u64(1234)` and `write_i32(1234)` are the same **if only called once**.
///
/// # Examples
///
/// ```
/// use core::hash::{Hash, Hasher, BuildHasher};
/// use zlim_utils::hash::NoopState;
///
/// let mut hasher = NoopState.build_hasher();
/// 3.hash(&mut hasher);
/// let result = hasher.finish();
///
/// assert_eq!(result, 3_u64);
/// ```
#[derive(Copy, Clone, Default, Debug)]
pub struct NoopState;

impl NoopState {
    /// A constant hasher instance, the same as `NoopState.build_hasher()`.
    pub const HASHER: NoopHasher = NoopHasher { hash: 0 };
}

impl BuildHasher for NoopState {
    type Hasher = NoopHasher;

    #[inline(always)]
    fn build_hasher(&self) -> Self::Hasher {
        Self::HASHER
    }
}

// ----------------------------------------------------------------------------
// SparseHasher

/// A fast hasher that provides uniformly distributed values starting from 0.
///
/// It can be created through the [`SparseState::build_hasher`] function
/// or copied from the [`SparseState::HASHER`] constant.
///
/// See the [`SparseState`] docs for more information.
#[derive(Copy, Clone, Debug)]
#[repr(transparent)]
pub struct SparseHasher {
    hash: u64,
}

/// From `bevy_ecs`.
///
/// SwissTable (and thus `hashbrown`) cares about two things from the hash:
///
/// - H1: low bits (masked by `2ⁿ-1`) to pick the slot in which to store the item.
/// - H2: high 7 bits are used to SIMD optimize hash collision probing.
///
/// For more see <https://abseil.io/about/design/swisstables#metadata-layout>.
///
/// This hash function assumes that the entity ids are still well-distributed,
/// so for H1 leaves the entity id alone in the low bits so that id locality
/// will also give memory locality for things spawned together.
/// For H2, take advantage of the fact that while multiplication doesn't
/// spread entropy to the low bits, it's incredibly good at spreading it
/// upward, which is exactly where we need it the most.
///
/// While this does include the generation in the output, it doesn't do so
/// *usefully*.  H1 won't care until you have over 3 billion entities in
/// the table, and H2 won't care until something hits generation 33 million.
/// Thus the comment suggesting that this is best for live entities,
/// where there won't be generation conflicts where it would matter.
///
/// The high 32 bits of this are ⅟φ for Fibonacci hashing.  That works
/// particularly well for hashing for the same reason as described in
/// <https://extremelearning.com.au/unreasonable-effectiveness-of-quasirandom-sequences/>
/// It loses no information because it has a modular inverse.
/// (Specifically, `0x144c_bc89_u32 * 0x9e37_79b9_u32 == 1`.)
///
/// The low 32 bits make that part of the just product a pass-through.
const UPPER_PHI: u64 = 0x9e37_79b9_0000_0001;

impl Hasher for SparseHasher {
    #[inline(always)]
    fn finish(&self) -> u64 {
        self.hash
    }

    #[inline(always)]
    fn write_usize(&mut self, i: usize) {
        self.hash = UPPER_PHI.wrapping_mul(i as u64);
    }

    #[inline(always)]
    fn write_u64(&mut self, i: u64) {
        self.hash = UPPER_PHI.wrapping_mul(i);
    }

    #[inline(always)]
    fn write_u32(&mut self, i: u32) {
        self.hash = UPPER_PHI.wrapping_mul(i as u64);
    }

    #[inline(always)]
    fn write_u16(&mut self, i: u16) {
        self.hash = UPPER_PHI.wrapping_mul(i as u64);
    }

    #[inline(always)]
    fn write_u8(&mut self, i: u8) {
        self.hash = UPPER_PHI.wrapping_mul(i as u64);
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.hash <<= 2;
            self.hash ^= UPPER_PHI.wrapping_mul(*byte as u64);
        }
    }
}

/// A very fast hash that is only designed to work on generational indices.
///
/// For example, `EntityIndex` in ECS module, it's uniformly distributed starting from 0.
#[derive(Copy, Clone, Default, Debug)]
pub struct SparseState;

impl SparseState {
    /// A constant hasher instance, the same as `SparseState.build_hasher()`.
    pub const HASHER: SparseHasher = SparseHasher { hash: 0 };
}

impl BuildHasher for SparseState {
    type Hasher = SparseHasher;

    #[inline(always)]
    fn build_hasher(&self) -> Self::Hasher {
        Self::HASHER
    }
}

// ----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use core::any::TypeId;
    use core::hash::{Hash, Hasher};

    #[test]
    fn noop_typeid_hash() {
        struct TestNoopHasher(u64);

        impl Hasher for TestNoopHasher {
            fn finish(&self) -> u64 {
                self.0
            }

            fn write_u64(&mut self, i: u64) {
                self.0 = i;
            }

            fn write(&mut self, _bytes: &[u8]) {
                panic!()
            }
        }

        let id = TypeId::of::<u32>();
        let mut hasher = TestNoopHasher(0);
        id.hash(&mut hasher);
        core::hint::black_box(hasher.finish());
    }
}
