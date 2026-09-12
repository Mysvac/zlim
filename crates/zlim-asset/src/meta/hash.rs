use core::fmt::Display;

use serde::de::Visitor;
use serde::{Deserialize, Serialize};

// -----------------------------------------------------------------------------
// AssetHash

/// A 32-byte content hash used to identify assets and detect changes.
///
/// Serialized as a 64-character ASCII string where each byte is encoded
/// as two characters in the range `0x41..=0x50` (`'A'..='P'`). This is
/// *not* standard hex — nibbles `10..=15` map to `:;<=>?` instead of `a..f`.
///
/// Encoding and decoding are symmetric, and the output is always printable
/// ASCII, so it never needs escaping in string contexts. This also makes
/// both directions easy to vectorize (SIMD).
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetHash(pub [u8; 32]);

// -----------------------------------------------------------------------------
// Serialize & Deserialize

impl Serialize for AssetHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let bytes: &[u8; 32] = &self.0;
        let mut buffer: [u8; 64] = [0; 64];

        for index in 0..32_usize {
            buffer[index << 1] = b'A' + (bytes[index] & 0b1111);
            buffer[(index << 1) + 1] = b'A' + (bytes[index] >> 4);
        }

        #[expect(unsafe_code, reason = "0x41..=0x50 are all valid UTF-8")]
        serializer.serialize_str(unsafe { str::from_utf8_unchecked(&buffer) })
    }
}

impl<'a> Deserialize<'a> for AssetHash {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        struct AssetHashVisitor;

        impl<'de> Visitor<'de> for AssetHashVisitor {
            type Value = AssetHash;

            fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
                formatter.write_str("a hash string of length 64")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if let Some(val) = v.as_bytes().as_array::<64>() {
                    let mut buffer: [u8; 32] = [0; 32];

                    for index in 0..32_usize {
                        let l = val[index << 1].wrapping_sub(b'A');
                        let r = (val[(index << 1) + 1].wrapping_sub(b'A')) << 4;
                        buffer[index] = l | r;
                    }

                    Ok(AssetHash(buffer))
                } else {
                    // `invalid_length` already marked `cold`
                    Err(serde::de::Error::invalid_length(v.len(), &self))
                }
            }
        }

        deserializer.deserialize_str(AssetHashVisitor)
    }
}

// -----------------------------------------------------------------------------
// Normal

impl AssetHash {
    /// The all-zero hash.
    pub const ZERO: AssetHash = AssetHash([0; 32]);
}

impl Default for AssetHash {
    #[inline(always)]
    fn default() -> Self {
        AssetHash::ZERO
    }
}

impl From<[u8; 32]> for AssetHash {
    #[inline(always)]
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}

impl From<AssetHash> for [u8; 32] {
    #[inline(always)]
    fn from(value: AssetHash) -> Self {
        value.0
    }
}

impl AssetHash {
    /// Returns a [`Display`] adapter that renders the hash as its 64-character.
    pub fn display(&self) -> impl Display {
        let bytes: &[u8; 32] = &self.0;
        let mut buffer: [u8; 64] = [0; 64];

        for index in 0..32_usize {
            buffer[index << 1] = b'A' + (bytes[index] & 0b1111);
            buffer[(index << 1) + 1] = b'A' + (bytes[index] >> 4);
        }

        #[expect(unsafe_code, reason = "0x30..=0x3F are all valid UTF-8")]
        return String::from(unsafe { str::from_utf8_unchecked(&buffer) });
    }
}

// -----------------------------------------------------------------------------
// Methods

use futures_lite::{AsyncRead, AsyncReadExt};

impl AssetHash {
    /// Hashes `meta` followed by the full contents of `reader` and `meta` into a single [`AssetHash`].
    ///
    /// NOTE: changing the hashing logic here is a _breaking change_ that requires a new format verion.
    pub async fn async_hash<R>(meta: &[u8], reader: &mut R) -> std::io::Result<Self>
    where
        R: AsyncRead + Unpin + Send,
    {
        let mut buffer: [u8; blake3::CHUNK_LEN] = [0; blake3::CHUNK_LEN];

        let mut hasher = blake3::Hasher::new();
        hasher.update(meta);

        loop {
            let bytes_read = reader.read(&mut buffer).await?;
            if bytes_read != 0 {
                hasher.update(&buffer[..bytes_read]);
            } else {
                return Ok(Self(*hasher.finalize().as_bytes()));
            }
        }
    }

    /// Folds `base` together with the hashes produced by `iter` into a single [`AssetHash`].
    ///
    /// Return `base` directly if `iter` is empty.
    ///
    /// NOTE: changing the hashing logic here is a _breaking change_ that requires a new format verion.
    pub fn fold_hash<'a>(base: &Self, mut iter: impl Iterator<Item = &'a Self>) -> Self {
        if let Some(first) = iter.next() {
            let mut hasher = blake3::Hasher::new();
            hasher.update(&base.0);
            hasher.update(&first.0);
            iter.for_each(|h| {
                hasher.update(&h.0);
            });
            Self(*hasher.finalize().as_bytes())
        } else {
            *base
        }
    }
}

// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::AssetHash;

    #[test]
    fn zero_hash() {
        let hash = AssetHash::default();
        assert_eq!(hash, AssetHash::ZERO);

        let serialized = ron::to_string(&hash).unwrap();
        let deserialized: AssetHash = ron::from_str(&serialized).unwrap();

        assert_eq!(hash, deserialized);

        assert_eq!(serialized.len(), 64 + 2);
        let zeros = &serialized.as_bytes()[1..=64];
        assert!(zeros.iter().all(|&x| x == b'A'));
    }

    #[test]
    fn roundtrip_via_ron() {
        let original = AssetHash([
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
            0xEE, 0xFF, 0x0F, 0xF0, 0x1E, 0xE1, 0x2D, 0xD2, 0x3C, 0xC3, 0x4B, 0xB4, 0x5A, 0xA5,
            0x69, 0x96, 0x78, 0x87,
        ]);

        let serialized = ron::to_string(&original).unwrap();
        let deserialized: AssetHash = ron::from_str(&serialized).unwrap();

        assert_eq!(original, deserialized);
    }
}
