use core::borrow::Borrow;
use core::fmt::{Debug, Display};
use core::hash::{Hash, Hasher};
use core::ops::Deref;
use std::borrow::Cow;
use std::sync::Arc;

use serde_core::de::{self, Visitor};
use serde_core::{Deserialize, Serialize};
use smol_str::SmolStr;

use crate::hash::FixedState;

// ----------------------------------------------------------------------------
// HashStr

/// A string type that stores a pre-computed hash alongside a [`SmolStr`].
///
/// `HashStr` caches the hash of its string content at construction time,
/// making [`Hash`] and equality checks `O(1)`. This is especially useful
/// when the string is used as a key in hash-based collections and is
/// compared or hashed frequently.
///
/// The hash is computed using [`FixedState`], the engine's deterministic
/// hasher, so hash values are stable within a single process execution.
///
/// # Examples
///
/// ```
/// use zlim_utils::str::HashStr;
///
/// let a = HashStr::from_str("hello");
/// let b = HashStr::from_str("hello");
/// let c = HashStr::from_str("world");
///
/// assert_eq!(a, b);
/// assert_ne!(a, c);
/// assert_eq!(a.hash(), b.hash());
/// ```
#[derive(Clone)]
pub struct HashStr {
    s: SmolStr,
    hash: u64,
}

// ----------------------------------------------------------------------------
// Traits

impl Default for HashStr {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for HashStr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(self.s.as_str(), f)
    }
}

impl Display for HashStr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self.s.as_str(), f)
    }
}

impl PartialEq for HashStr {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.s.as_str() == other.s.as_str()
    }
}

impl PartialEq<str> for HashStr {
    fn eq(&self, other: &str) -> bool {
        self.s.as_str() == other
    }
}

impl PartialEq<&str> for HashStr {
    fn eq(&self, other: &&str) -> bool {
        self.s.as_str() == *other
    }
}

impl Eq for HashStr {}

impl PartialOrd for HashStr {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialOrd<str> for HashStr {
    fn partial_cmp(&self, other: &str) -> Option<core::cmp::Ordering> {
        str::partial_cmp(self.s.as_str(), other)
    }
}

impl Ord for HashStr {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.hash
            .cmp(&other.hash)
            .then_with(|| self.s.as_str().cmp(other.s.as_str()))
    }
}

impl Hash for HashStr {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

impl Deref for HashStr {
    type Target = str;
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.s.as_str()
    }
}

impl Borrow<str> for HashStr {
    #[inline]
    fn borrow(&self) -> &str {
        self.s.as_str()
    }
}

impl AsRef<str> for HashStr {
    #[inline]
    fn as_ref(&self) -> &str {
        self.s.as_str()
    }
}

impl AsRef<std::ffi::OsStr> for HashStr {
    #[inline]
    fn as_ref(&self) -> &std::ffi::OsStr {
        AsRef::<std::ffi::OsStr>::as_ref(self.s.as_str())
    }
}

impl AsRef<std::path::Path> for HashStr {
    #[inline]
    fn as_ref(&self) -> &std::path::Path {
        AsRef::<std::path::Path>::as_ref(self.s.as_str())
    }
}

impl From<&str> for HashStr {
    fn from(value: &str) -> Self {
        Self {
            hash: Self::get_hash(value),
            s: SmolStr::new(value),
        }
    }
}

impl From<Arc<str>> for HashStr {
    fn from(value: Arc<str>) -> Self {
        Self {
            hash: Self::get_hash(&value),
            s: SmolStr::from(value),
        }
    }
}

impl From<String> for HashStr {
    fn from(value: String) -> Self {
        Self {
            hash: Self::get_hash(&value),
            s: SmolStr::new(value.as_str()),
        }
    }
}

impl From<Cow<'_, str>> for HashStr {
    fn from(value: Cow<'_, str>) -> Self {
        let value = AsRef::<str>::as_ref(&value);
        Self {
            hash: Self::get_hash(value),
            s: SmolStr::new(value),
        }
    }
}

impl From<super::SmolStr> for HashStr {
    fn from(value: super::SmolStr) -> Self {
        let s = value.0.as_str();
        Self {
            hash: Self::get_hash(s),
            s: value.0,
        }
    }
}

// ----------------------------------------------------------------------------
// Methods

impl HashStr {
    /// Pre-computed hash value for the empty string `""`.
    const EMPTY_STR_HASH: u64 = 0x38742ca63a7d366d;

    /// Compute the [`FixedState`] hash of a string slice.
    #[inline]
    fn get_hash(s: &str) -> u64 {
        let mut hasher = FixedState::HASHER;
        s.hash(&mut hasher);
        hasher.finish()
    }

    /// Creates a new empty `HashStr`.
    #[inline]
    pub const fn new() -> Self {
        Self {
            s: SmolStr::new_static(""),
            hash: Self::EMPTY_STR_HASH,
        }
    }

    /// Returns the pre-computed hash of this string.
    ///
    /// The hash is computed once at construction time using [`FixedState`];
    /// this method returns the cached value in `O(1)`.
    #[inline]
    pub const fn hash(&self) -> u64 {
        self.hash
    }

    /// Returns the length of `self` in bytes.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.s.len()
    }

    /// Returns `true` if `self` has a length of zero bytes.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.s.is_empty()
    }

    /// Returns a `&str` slice of this `HashStr`.
    #[inline]
    pub fn as_str(&self) -> &str {
        self.s.as_str()
    }

    /// Creates a `HashStr` from a string slice.
    ///
    /// The hash is computed eagerly using [`FixedState`].
    #[expect(clippy::should_implement_trait, reason = "Use this instead")]
    pub fn from_str(s: &str) -> Self {
        Self {
            hash: Self::get_hash(s),
            s: SmolStr::new(s),
        }
    }
}

impl Serialize for HashStr {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde_core::Serializer,
    {
        serializer.serialize_str(self.s.as_str())
    }
}

impl<'a> Deserialize<'a> for HashStr {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde_core::Deserializer<'a>,
    {
        deserializer.deserialize_str(HashStrVisitor)
    }
}

struct HashStrVisitor;

impl Visitor<'_> for HashStrVisitor {
    type Value = HashStr;

    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        formatter.write_str("a string")
    }

    #[inline]
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(HashStr::from_str(v))
    }
}

// ----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::HashStr;
    use crate::hash::FixedState;
    use core::hash::{Hash, Hasher};

    #[test]
    fn empty_str_hash() {
        let mut hash = FixedState::HASHER;
        "".hash(&mut hash);
        assert_eq!(hash.finish(), HashStr::EMPTY_STR_HASH);
    }
}
