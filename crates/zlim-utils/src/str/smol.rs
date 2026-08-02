use core::borrow::Borrow;
use core::fmt::{Debug, Display};
use core::hash::{Hash, Hasher};
use core::ops::Deref;
use std::borrow::Cow;
use std::sync::Arc;

use serde_core::de::{self, Visitor};
use serde_core::{Deserialize, Serialize};
use smol_str::SmolStr as Inner;

// ----------------------------------------------------------------------------
// SmolStr

/// A small-buffer-optimized, immutable string type.
///
/// Wraps [`smol_str::SmolStr`] from the `smol_str` crate, providing a
/// consistent API surface for the engine while allowing the underlying
/// implementation to evolve independently.
///
/// # Key properties
///
/// - `size_of::<SmolStr>() == 24` (same as `String` on 64-bit platforms)
/// - `Clone` is `O(1)`
/// - Strings ≤ 23 bytes are stack-allocated
/// - Longer strings are heap-allocated, unless they match the internal
///   whitespace pattern of the underlying `smol_str` crate
/// - Can be constructed from a `&'static str` without allocation
///
/// Unlike `String`, `SmolStr` is immutable — it does not support
/// mutation after construction.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct SmolStr(pub(super) Inner);

impl Default for SmolStr {
    #[inline(always)]
    fn default() -> Self {
        Self(Inner::new_static(""))
    }
}

impl SmolStr {
    /// Constructs a `SmolStr` from a statically allocated string.
    ///
    /// This never allocates.
    ///
    /// For non-static str, use [`SmolStr::from_str`] or [`SmolStr::from`] instead.
    #[inline(always)]
    pub const fn new(s: &'static str) -> Self {
        Self(Inner::new_static(s))
    }

    /// Create a `SmolStr` from a `str`, heap-allocating if necessary.
    ///
    /// For static str, use [`SmolStr::new`] instead.
    #[expect(clippy::should_implement_trait, reason = "Use this instead")]
    #[inline(always)]
    pub fn from_str(s: &str) -> Self {
        Self(Inner::new(s))
    }

    /// Returns a `&str` slice of this SmolStr.
    #[inline(always)]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the length of `self` in bytes.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if `self` has a length of zero bytes.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Debug for SmolStr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(self.0.as_str(), f)
    }
}

impl Display for SmolStr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self.0.as_str(), f)
    }
}

impl Hash for SmolStr {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_str().hash(state);
    }
}

impl Deref for SmolStr {
    type Target = str;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

impl Borrow<str> for SmolStr {
    #[inline(always)]
    fn borrow(&self) -> &str {
        self.0.as_str()
    }
}

impl AsRef<str> for SmolStr {
    #[inline(always)]
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl AsRef<[u8]> for SmolStr {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        self.0.as_str().as_bytes()
    }
}

impl AsRef<std::ffi::OsStr> for SmolStr {
    fn as_ref(&self) -> &std::ffi::OsStr {
        AsRef::<std::ffi::OsStr>::as_ref(self.0.as_str())
    }
}

impl AsRef<std::path::Path> for SmolStr {
    fn as_ref(&self) -> &std::path::Path {
        AsRef::<std::path::Path>::as_ref(self.0.as_str())
    }
}

impl PartialEq<str> for SmolStr {
    fn eq(&self, other: &str) -> bool {
        self.0.as_str() == other
    }
}

impl PartialEq<&str> for SmolStr {
    fn eq(&self, other: &&str) -> bool {
        self.0.as_str() == *other
    }
}

impl PartialOrd<str> for SmolStr {
    fn partial_cmp(&self, other: &str) -> Option<core::cmp::Ordering> {
        str::partial_cmp(self.0.as_str(), other)
    }
}

impl From<&str> for SmolStr {
    #[inline(always)]
    fn from(value: &str) -> Self {
        Self(Inner::new(value))
    }
}

impl From<&&str> for SmolStr {
    #[inline(always)]
    fn from(value: &&str) -> Self {
        Self(Inner::new(*value))
    }
}

impl From<Arc<str>> for SmolStr {
    #[inline(always)]
    fn from(value: Arc<str>) -> Self {
        Self(Inner::from(value))
    }
}

impl From<&Arc<str>> for SmolStr {
    #[inline(always)]
    fn from(value: &Arc<str>) -> Self {
        Self(Inner::from(value.clone()))
    }
}

impl From<String> for SmolStr {
    #[inline(always)]
    fn from(value: String) -> Self {
        Self(Inner::new(value.as_str()))
    }
}

impl From<&String> for SmolStr {
    #[inline(always)]
    fn from(value: &String) -> Self {
        Self(Inner::new(value.as_str()))
    }
}

impl From<Cow<'_, str>> for SmolStr {
    #[inline(always)]
    fn from(value: Cow<'_, str>) -> Self {
        Self(Inner::new(AsRef::<str>::as_ref(&value)))
    }
}

impl From<Inner> for SmolStr {
    #[inline(always)]
    fn from(value: Inner) -> Self {
        Self(value)
    }
}

impl From<SmolStr> for Arc<str> {
    #[inline(always)]
    fn from(text: SmolStr) -> Self {
        <Arc<str>>::from(text.0)
    }
}

impl From<SmolStr> for String {
    #[inline(always)]
    fn from(text: SmolStr) -> Self {
        text.0.as_str().to_owned()
    }
}

impl Serialize for SmolStr {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde_core::Serializer,
    {
        serializer.serialize_str(self.0.as_str())
    }
}

impl<'a> Deserialize<'a> for SmolStr {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde_core::Deserializer<'a>,
    {
        deserializer.deserialize_str(SmolStrVisitor)
    }
}

struct SmolStrVisitor;

impl Visitor<'_> for SmolStrVisitor {
    type Value = SmolStr;

    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        formatter.write_str("a string")
    }

    #[inline]
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(SmolStr::from_str(v))
    }

    #[inline]
    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
        Ok(SmolStr::from_str(&v))
    }
}
