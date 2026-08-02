//! A global string-interning pool backed by a pre-hashed `HashSet`.
use core::hash::{Hash, Hasher};
use std::sync::{PoisonError, RwLock};

use hashbrown::Equivalent;

use crate::hash::{FixedState, HashSet, NoopState};

// ----------------------------------------------------------------------------
// SHS — static hash string (owned pool entry)

/// Pre-computed hash + `&'static str` pair stored in the global pool.
///
/// A separate `SHS` type is necessary because `RwLock<HashSet<HS<'static>>>`
/// would shorten the lifetime returned by [`HashSet::get`] from `HS<'static>`
/// to `HS<'_>`.  Splitting into `SHS` (owned) and `HS` (borrowed key) avoids
/// `unsafe` lifetime transmutes.
#[derive(Clone, Copy)]
#[expect(clippy::upper_case_acronyms, reason = "Static-Hash-String")]
struct SHS {
    s: &'static str,
    h: u64,
}

impl Hash for SHS {
    #[inline(always)]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.h);
    }
}

impl PartialEq for SHS {
    #[inline(always)]
    fn eq(&self, other: &SHS) -> bool {
        self.s == other.s
    }
}

impl Eq for SHS {}

// ----------------------------------------------------------------------------
// HS — borrowed lookup key

/// Borrowed key for querying the pool without allocating.
struct HS<'a> {
    s: &'a str,
    h: u64,
}

impl Hash for HS<'_> {
    #[inline(always)]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.h);
    }
}

impl Equivalent<SHS> for HS<'_> {
    #[inline(always)]
    fn equivalent(&self, other: &SHS) -> bool {
        self.s == other.s
    }
}

// ----------------------------------------------------------------------------
// POOL

/// Global interning pool.
///
/// Uses [`NoopState`] because hashing is delegated entirely to the
/// pre-computed hash stored in [`SHS`] and [`HS`].
static POOL: RwLock<HashSet<SHS, NoopState>> = RwLock::new(HashSet::with_hasher(NoopState));

// ----------------------------------------------------------------------------
// intern_str

/// Intern a string, returning a `&'static str` that lives until program exit.
///
/// # How it works
///
/// 1. Hash the input with [`FixedState`] to produce a deterministic `u64`.
///
/// 2. Read-lock the pool and look up an existing entry via hash and string.
///
/// 3. **Hit** → return the cached `&'static str`.
///
/// 4. **Miss** → promote the string to a [`Global`] static allocation,
///    write-lock the pool, and insert a new [`SHS`] entry.
///
/// [`Global`]: crate::mem::Global
#[inline(never)] // No need to inline.
pub fn intern_str(s: &str) -> &'static str {
    // Hash once outside the lock so the read path is as cheap as possible.
    let mut hasher = FixedState::HASHER;
    s.hash(&mut hasher);
    let h = hasher.finish();

    let slot = POOL
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&HS { s, h })
        .copied();

    if let Some(shs) = slot {
        return shs.s;
    }

    // Slow path — allocate a permanent copy.
    let s: &'static str = crate::mem::Global::alloc_str(s);

    // High concurrency has a probability of multiple allocations
    // of the same string, but the probability is low and acceptable.
    POOL.write()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert(SHS { s, h }) // not `get_or_insert_with`, separate locks
        .s
    // ↑ If inserting concurrently, prioritize using internal values
    // to ensure consistency with string pointers.
}

// ----------------------------------------------------------------------------
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
