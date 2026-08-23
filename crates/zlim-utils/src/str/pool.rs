//! A global string-interning pool backed by a pre-hashed `HashSet`.
use core::hash::{Hash, Hasher};
use std::sync::{PoisonError, RwLock};

use crate::hash::{Equivalent, FixedState};
use crate::hash::{HashSet, NoopState};

// -----------------------------------------------------------------------------
// Helper

struct HashStr<'a> {
    hash: u64,
    s: &'a str,
}

impl Hash for HashStr<'_> {
    #[inline(always)]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

impl<'a> HashStr<'a> {
    /// Creates a new `HashStr` from a string slice.
    ///
    /// The hash is computed once at construction time using [`FixedState`].
    #[inline]
    fn new(s: &'a str) -> HashStr<'a> {
        let mut state = FixedState::HASHER;
        s.hash(&mut state);
        let hash = state.finish();
        HashStr { s, hash }
    }
}

struct HS {
    h: u64,
    s: &'static str,
}

impl Hash for HS {
    #[inline(always)]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.h);
    }
}

impl PartialEq for HS {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.h == other.h && self.s == other.s
    }
}

impl Eq for HS {}

impl Equivalent<HS> for HashStr<'_> {
    #[inline(always)]
    fn equivalent(&self, key: &HS) -> bool {
        self.s == key.s
    }
}

// -----------------------------------------------------------------------------
// Pool

/// Global interning pool.
static POOL: RwLock<HashSet<HS, NoopState>> = RwLock::new(HashSet::with_hasher(NoopState));

// -----------------------------------------------------------------------------
// intern_str

/// Intern a string, returning a `&'static str` that lives until program exit.
///
/// # How it works
///
/// 1. Hash the input with [`FixedState`](crate::hash::FixedState) to produce a
///    deterministic `u64`.
///
/// 2. Read-lock the pool and look up an existing entry via hash and string.
///
/// 3. **Hit** → return the cached `&'static str`.
///
/// 4. **Miss** → promote the string to a [`Global`] static allocation,
///    write-lock the pool, and insert a new entry.
///
/// [`Global`]: crate::mem::Global
#[inline(never)] // No need to inline.
pub fn intern_str<'a>(s: &'a str) -> &'static str {
    // Hash once outside the lock so the read path is as cheap as possible.
    let hs: HashStr<'a> = HashStr::new(s);

    {
        let pool = POOL.read().unwrap_or_else(PoisonError::into_inner);

        if let Some(hs) = pool.get(&hs) {
            return hs.s;
        }
    }

    // Slow path — allocate a permanent copy.
    let s: &'static str = crate::mem::Global::alloc_str(s);

    let hs = HS { s, h: hs.hash };

    // High concurrency has a probability of multiple allocations
    // of the same string, but the probability is low and acceptable.
    POOL.write()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert(hs) // not `get_or_insert_with`, separate locks
        .s
    // ↑ If inserting concurrently, prioritize using internal values
    // to ensure consistency with string pointers.
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::intern_str;

    #[test]
    fn double_intern() {
        let a = intern_str("123456");
        let b = intern_str("123456");
        assert_eq!(a, "123456");
        assert_eq!(b, "123456");
        assert_eq!(a.as_ptr(), b.as_ptr());
        assert_ne!(a.as_ptr(), "123456".as_ptr());
    }
}
