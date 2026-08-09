//! Entity identifiers.

use core::cmp::Ordering;
use core::fmt::{Debug, Display};
use core::hash::Hash;
use core::mem;
use core::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::table::{TableId, TableRow};

// -----------------------------------------------------------------------------
// EntityId

/// A unique identifier for an entity.
///
/// Made up of an [`u32`] `Index` and an [`NonZeroU32`] `Generation`.
///
/// Entities are frequently created and destroyed, which requires efficient
/// reuse of identifiers. The index names the slot the entity occupies, while
/// the generation distinguishes between successive occupants of that slot,
/// preventing a stale handle from accessing data that now belongs to a
/// different entity.
///
/// The struct is guaranteed to have the same representation as a `u64`
/// (8-byte aligned) to enable efficient bitwise operations and serialization.
/// The fields are ordered per target endianness so that the index always
/// occupies the low 32 bits and the version the high 32 bits of
/// [`to_bits`], giving consistent behavior across platforms.
///
/// [`to_bits`]: Self::to_bits
#[derive(Clone, Copy)]
#[repr(C, align(8))]
pub struct EntityId {
    #[cfg(target_endian = "little")]
    pub(super) index: u32,
    pub(super) generation: NonZeroU32,
    #[cfg(target_endian = "big")]
    pub(super) index: u32,
}

impl EntityId {
    /// A placeholder handle representing an invalid or uninitialized entity.
    ///
    /// Its index is `u32::MAX` and its version is `u32::MAX`; it is never equal
    /// to a handle returned for a live entity.
    pub const PLACEHOLDER: Self = Self {
        index: u32::MAX,
        generation: NonZeroU32::MAX,
    };

    /// Returns the raw index of this entity.
    #[inline(always)]
    pub const fn index(self) -> u32 {
        self.index
    }

    /// Returns the raw generation of this entity.
    ///
    /// The result is always non-zero.
    #[inline(always)]
    pub const fn generation(self) -> NonZeroU32 {
        self.generation
    }

    /// Reinterprets this [`EntityId`] as its underlying `u64` bit pattern.
    ///
    /// The index occupies the low 32 bits and the version the high 32 bits,
    /// regardless of target endianness. The result round-trips through [`from_bits`].
    ///
    /// [`from_bits`]: Self::from_bits
    #[inline(always)]
    pub const fn to_bits(self) -> u64 {
        // SAFETY: `EntityId` is `repr(C, align(8))` with two `u32`-sized fields
        // and no padding, so it has the same layout as a `u64`.
        unsafe { mem::transmute::<EntityId, u64>(self) }
    }

    /// Reconstructs an [`EntityId`] from a `u64` produced by [`to_bits`].
    ///
    /// Returns `None` if `bits` does not encode a valid `EntityId`, i.e.
    /// if its generation part is zero.
    ///
    /// [`to_bits`]: Self::to_bits
    #[inline]
    pub const fn from_bits(bits: u64) -> Option<Self> {
        const OFFSET: usize = mem::offset_of!(EntityId, index);

        let ptr: *const u32 = &raw const bits as *const u32;
        if unsafe { *ptr.byte_add(OFFSET) } == 0 {
            core::hint::cold_path();
            None
        } else {
            // SAFETY: the index part is non-zero, so `bits` is a valid
            // `EntityId`, which shares its layout with `u64`.
            Some(unsafe { mem::transmute::<u64, EntityId>(bits) })
        }
    }
}

// -----------------------------------------------------------------------------
// Traits

impl Hash for EntityId {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.to_bits());
    }
}

impl PartialEq for EntityId {
    #[inline]
    fn eq(&self, other: &EntityId) -> bool {
        self.to_bits() == other.to_bits()
    }
}

impl Eq for EntityId {}

impl PartialOrd for EntityId {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EntityId {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.to_bits().cmp(&other.to_bits())
    }
}

impl Debug for EntityId {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self, f)
    }
}

impl Display for EntityId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if *self == Self::PLACEHOLDER {
            f.pad("PLACEHOLDER")
        } else {
            write!(f, "{}v{}", self.index, self.generation)
        }
    }
}

impl Serialize for EntityId {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u64(self.to_bits())
    }
}

impl<'de> Deserialize<'de> for EntityId {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        let bits: u64 = Deserialize::deserialize(deserializer)?;

        match EntityId::from_bits(bits) {
            Some(val) => Ok(val),
            None => Err(Error::custom("The EntityGeneration cannot be zero.")),
        }
    }
}

// -----------------------------------------------------------------------------
// EntityId

#[derive(Clone, Copy, Hash)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub struct Location {
    pub table_id: TableId,
    pub table_row: TableRow,
}

impl Debug for Location {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Table[{}][{}]", self.table_id, self.table_row.0)
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::EntityId;

    #[test]
    fn consistent() {
        let index: u32 = 0x0001_FAAF;
        let generation: u32 = 0x0010_A730;

        let raw: u64 = (index as u64) + ((generation as u64) << 32);
        let id: EntityId = EntityId::from_bits(raw).unwrap();

        assert_eq!(id.index(), index);
        assert_eq!(id.generation().get(), generation);
        assert_eq!(id.to_bits(), raw);
    }
}
