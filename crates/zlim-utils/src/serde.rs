//! Provides zero-copy String deserailization container.

use std::borrow::Cow;

use serde_core::{Deserialize, de::Visitor};

/// A specialized [`Cow<'a, str>`] used only for deserialization.
///
/// Rust does not yet support specialization, so serde's own impl for
/// `Cow<'a, str>` cannot take advantage of zero-copy deserialization:
/// it always allocates, even when the deserializer could hand out a
/// borrowed `&'de str`. This wrapper fills that gap by forwarding to
/// [`Deserializer::deserialize_string`] and preserving the borrowed
/// path through [`Visitor::visit_borrowed_str`].
///
/// Use it as a temporary stand-in during deserialization and convert it
/// back into a `Cow` when done.
///
/// [`Deserializer::deserialize_string`]: serde_core::Deserializer::deserialize_string
/// [`Visitor::visit_borrowed_str`]: serde_core::de::Visitor::visit_borrowed_str
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct BorrowedStr<'a>(pub Cow<'a, str>);

impl<'de> Deserialize<'de> for BorrowedStr<'de> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde_core::Deserializer<'de>,
    {
        struct CowStrVisitor;

        impl<'de> Visitor<'de> for CowStrVisitor {
            type Value = BorrowedStr<'de>;

            fn expecting(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                f.write_str("a string")
            }

            fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
            where
                E: serde_core::de::Error,
            {
                Ok(BorrowedStr(Cow::Borrowed(v)))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde_core::de::Error,
            {
                Ok(BorrowedStr(Cow::Owned(v.to_owned())))
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: serde_core::de::Error,
            {
                Ok(BorrowedStr(Cow::Owned(v)))
            }
        }

        // `BorrowedStr` wants both the borrowed path (`visit_borrowed_str`) and the
        // owned path (`visit_string`). Serde gives no rule for which entry point a
        // type that may take ownership should use.
        //
        // In current implementations the choice does not matter: RON and serde_json
        // both forward `deserialize_string` directly to `deserialize_str`, letting
        // the underlying string parser decide whether to borrow or copy.
        deserializer.deserialize_str(CowStrVisitor)
    }
}
