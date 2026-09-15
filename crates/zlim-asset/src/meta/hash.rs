use core::fmt::Display;

use futures_lite::{AsyncRead, AsyncReadExt};
use serde::de::Visitor;
use serde::{Deserialize, Serialize};

// -----------------------------------------------------------------------------
// AssetHash

/// A 32-byte content hash used to identify assets and detect changes.
///
/// Serialized as a 64-character ASCII string where each byte is encoded
/// as two characters in the range `0x41..=0x50` (`'A'..='P'`).
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

    /// The zero hash is `AssetHash::default()`. It survives a RON round-trip, and its serialized
    /// form is the two quotes `serde` adds around 64 `A`s, the character a zero nibble encodes to.
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

    // -------------------------------------------------------------------------
    // Encoding

    /// The encoding described by [`AssetHash`]: `'A' + low nibble`, then `'A' + high nibble.
    fn encoded(hash: &AssetHash) -> String {
        let mut text = String::with_capacity(64);

        for byte in hash.0 {
            text.push(char::from(b'A' + (byte & 0b1111)));
            text.push(char::from(b'A' + (byte >> 4)));
        }

        text
    }

    /// Each byte becomes exactly two characters: the low nibble first and then the high one, each
    /// offset from `A`, which keeps the text printable and always 64 characters long. The cases
    /// walk the ends of the nibble range, which is what tells the two halves apart.
    #[test]
    fn the_encoding_uses_one_character_per_nibble() {
        let cases = [
            ([0x00; 32], "AA"),
            ([0x11; 32], "BB"),
            ([0x0F; 32], "PA"),
            ([0xF0; 32], "AP"),
            ([0xFF; 32], "PP"),
        ];

        for (bytes, pair) in cases {
            let hash = AssetHash(bytes);
            let text = hash.display().to_string();

            assert_eq!(text.len(), 64);
            assert_eq!(text, pair.repeat(32));
            assert_eq!(text, encoded(&hash));
            assert!(text.bytes().all(|byte| (b'A'..=b'P').contains(&byte)));
        }
    }

    /// `Display` and the `serde` form must not drift apart: the text a person reads and the text a
    /// `.meta` file stores have to name the same bytes.
    #[test]
    fn display_and_serialization_use_the_same_encoding() {
        let hash = AssetHash(core::array::from_fn(|index| {
            (index as u8).wrapping_mul(37).wrapping_add(0x9E)
        }));

        let displayed = hash.display().to_string();
        let serialized = ron::to_string(&hash).expect("an AssetHash serializes");

        assert_eq!(displayed, encoded(&hash));
        assert_eq!(serialized, format!("\"{displayed}\""));
    }

    #[test]
    fn decoding_inverts_the_encoding() {
        let hash = AssetHash(core::array::from_fn(|index| {
            (index as u8).wrapping_mul(37).wrapping_add(0x9E)
        }));

        let serialized = ron::to_string(&hash).expect("an AssetHash serializes");
        let decoded: AssetHash = ron::from_str(&serialized).expect("the encoding decodes");

        assert_eq!(decoded, hash);

        // The encoding is a bijection, so a hand-written string decodes to the same bytes:
        // `'A'` is a zero low nibble and `'B'` a high nibble of one.
        let handcrafted = format!("\"{}\"", "AB".repeat(32));
        let decoded: AssetHash = ron::from_str(&handcrafted).expect("the encoding decodes");

        assert_eq!(decoded, AssetHash([0x10; 32]));
    }

    #[test]
    fn decoding_rejects_a_hash_of_the_wrong_length() {
        let too_short = format!("\"{}\"", "A".repeat(63));
        let too_long = format!("\"{}\"", "A".repeat(65));

        assert!(ron::from_str::<AssetHash>(&too_short).is_err());
        assert!(ron::from_str::<AssetHash>(&too_long).is_err());
    }

    // -------------------------------------------------------------------------
    // Hashing

    #[test]
    fn fold_hash_returns_the_base_when_there_is_nothing_to_fold() {
        let base = AssetHash([7; 32]);

        assert_eq!(AssetHash::fold_hash(&base, core::iter::empty()), base);
    }

    /// Folding is one blake3 pass over the base and then the inputs in iteration order: the same
    /// inputs give the same result, a different order does not, and the result is neither the base
    /// alone nor what a shorter fold would give.
    ///
    /// The expected value is recomputed by hand, so changing what gets folded — or the order it is
    /// folded in — fails here instead of quietly changing every hash the importer records.
    #[test]
    fn fold_hash_mixes_the_base_and_the_hashes_in_order() {
        let base = AssetHash([1; 32]);
        let first = AssetHash([2; 32]);
        let second = AssetHash([3; 32]);

        let forward = AssetHash::fold_hash(&base, [first, second].iter());
        let again = AssetHash::fold_hash(&base, [first, second].iter());
        let reversed = AssetHash::fold_hash(&base, [second, first].iter());

        assert_eq!(forward, again, "folding must be deterministic");
        assert_ne!(forward, reversed, "folding must depend on the order");
        assert_ne!(forward, base);
        assert_ne!(forward, AssetHash::fold_hash(&base, [first].iter()));

        // The base is hashed first, then the folded hashes in iteration order.
        let mut hasher = blake3::Hasher::new();
        hasher.update(&base.0);
        hasher.update(&first.0);
        hasher.update(&second.0);

        assert_eq!(forward, AssetHash(*hasher.finalize().as_bytes()));
    }

    /// The hash covers the `.meta` bytes first and the asset contents after them, so a change to
    /// either one shows up as a different hash.
    #[test]
    fn async_hash_appends_the_reader_to_the_meta() {
        let meta = b"some meta bytes";
        let contents = b"the asset contents";

        let mut reader: &[u8] = contents;
        let hash = futures_lite::future::block_on(AssetHash::async_hash(meta, &mut reader))
            .expect("hashing a slice cannot fail");

        let mut hasher = blake3::Hasher::new();
        hasher.update(meta);
        hasher.update(contents);

        assert_eq!(hash, AssetHash(*hasher.finalize().as_bytes()));
    }

    #[test]
    fn async_hash_of_an_empty_reader_hashes_the_meta_alone() {
        let meta = b"meta without contents";

        let mut reader: &[u8] = &[];
        let hash = futures_lite::future::block_on(AssetHash::async_hash(meta, &mut reader))
            .expect("hashing a slice cannot fail");

        assert_eq!(hash, AssetHash(*blake3::hash(meta).as_bytes()));
    }
}
