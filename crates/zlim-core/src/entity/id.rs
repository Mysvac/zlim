//! Entity identifiers.
//!
//! An [`EntityId`] is a lightweight handle that uniquely identifies an entity.
//!
//! - [`EntityIndex`] is the slot the entity occupies.
//! - [`EntityVersion`] is the generation of that slot.

use core::cmp::Ordering;
use core::fmt::{Debug, Display};
use core::hash::Hash;
use core::mem;
use core::num::NonZeroU32;

use serde::{Deserialize, Serialize};

// ------------------------------------------------------------------------------
// EntityIndex

/// The index of an entity within the entity array.
///
/// The valid range is `1..u32::MAX`: `0` is an invalid value (reserved to give
/// `Option<EntityIndex>` a niche), and `u32::MAX` is reserved as a placeholder.
#[derive(Clone, Copy, Hash)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct EntityIndex(NonZeroU32);

impl EntityIndex {
    /// Creates a new `EntityIndex` from a raw index value.
    ///
    /// # Panics
    ///
    /// Panics if `index == 0`.
    #[inline(always)]
    pub const fn without_provenance(index: u32) -> Self {
        #[cold]
        #[inline(never)]
        const fn invalid_index() -> ! {
            panic!("EntityIndex cannot be `0`.");
        }

        match NonZeroU32::new(index) {
            Some(val) => Self(val),
            None => invalid_index(),
        }
    }

    /// Returns the raw value of this index.
    ///
    /// The result is always non-zero.
    #[inline(always)]
    pub const fn get(self) -> u32 {
        // SAFETY: `EntityIndex` is `repr(transparent)` over `NonZeroU32`.
        unsafe { mem::transmute::<EntityIndex, u32>(self) }
    }
}

impl Debug for EntityIndex {
    #[inline(always)]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&self.get(), f)
    }
}

impl Display for EntityIndex {
    #[inline(always)]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(&self.get(), f)
    }
}

// ------------------------------------------------------------------------------
// EntityVersion

/// Tracks the version (generation) of an entity occupying a given index.
///
/// Because `EntityVersion` wraps a `u32`, it cannot represent *every* possible
/// generation: after enough reuses of the same index, the version eventually
/// wraps around and aliases an earlier one.
///
/// When that happens, two distinct conceptual entities can end up with equal
/// [`EntityId`] values, so a stale handle may silently resolve to the wrong
/// entity. Callers should therefore avoid holding an `EntityId` across long
/// spans of time.
#[derive(Clone, Copy, Hash)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct EntityVersion(u32);

impl EntityVersion {
    /// The smallest version, `0`.
    pub const MIN: Self = Self(u32::MIN);

    /// The largest version, `u32::MAX`.
    pub const MAX: Self = Self(u32::MAX);

    /// Returns the raw value of this version.
    #[inline(always)]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Returns the next version.
    ///
    /// Overflow wraps, so the version after [`Self::MAX`] is [`Self::MIN`].
    #[must_use]
    #[inline(always)]
    pub const fn next(self) -> EntityVersion {
        Self(self.0.wrapping_add(1))
    }
}

impl Debug for EntityVersion {
    #[inline(always)]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&self.get(), f)
    }
}

impl Display for EntityVersion {
    #[inline(always)]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(&self.get(), f)
    }
}

// ------------------------------------------------------------------------------
// EntityId

/// A unique identifier for an entity, made up of an [`EntityIndex`] and an
/// [`EntityVersion`].
///
/// Entities are frequently created and destroyed, which requires efficient
/// reuse of identifiers. The index names the slot the entity occupies, while
/// the version distinguishes between successive occupants of that slot,
/// preventing a stale handle from accessing data that now belongs to a
/// different entity.
///
/// The struct is guaranteed to have the same representation as a `u64`
/// (8-byte aligned) to enable efficient bitwise operations and serialization.
/// The fields are ordered per target endianness so that the index always
/// occupies the low 32 bits and the version the high 32 bits of
/// [`to_bits`](Self::to_bits), giving consistent behavior across platforms.
#[derive(Clone, Copy)]
#[repr(C, align(8))]
pub struct EntityId {
    #[cfg(target_endian = "little")]
    index: EntityIndex,
    version: EntityVersion,
    #[cfg(target_endian = "big")]
    index: EntityIndex,
}

impl EntityId {
    /// A placeholder handle representing an invalid or uninitialized entity.
    ///
    /// Its index is `u32::MAX` and its version is `u32::MAX`; it is never equal
    /// to a handle returned for a live entity.
    // SAFETY: the all-ones bit pattern is a valid `EntityId`: the index field
    // (`NonZeroU32`) is `u32::MAX`, which is non-zero.
    pub const PLACEHOLDER: Self = unsafe { mem::transmute::<u64, EntityId>(u64::MAX) };

    /// Creates an `EntityId` from its index and version.
    #[inline(always)]
    pub const fn new(index: EntityIndex, version: EntityVersion) -> Self {
        Self { index, version }
    }

    /// Returns the raw index of this entity.
    ///
    /// The result is always non-zero.
    #[inline(always)]
    pub const fn index(self) -> u32 {
        self.index.get()
    }

    /// Returns the raw version (generation) of this entity.
    #[inline(always)]
    pub const fn version(self) -> u32 {
        self.version.get()
    }

    /// Reinterprets this `EntityId` as its underlying `u64` bit pattern.
    ///
    /// The index occupies the low 32 bits and the version the high 32 bits,
    /// regardless of target endianness. The result round-trips through
    /// [`from_bits`](Self::from_bits).
    #[inline(always)]
    pub const fn to_bits(self) -> u64 {
        // SAFETY: `EntityId` is `repr(C, align(8))` with two `u32`-sized fields
        // and no padding, so it has the same layout as a `u64`.
        unsafe { mem::transmute::<EntityId, u64>(self) }
    }

    /// Reconstructs an `EntityId` from a `u64` produced by [`to_bits`](Self::to_bits).
    ///
    /// Returns `None` if `bits` does not encode a valid `EntityId`, i.e. if its
    /// index part is zero.
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
            write!(f, "{}v{}", self.index(), self.version())
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
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        let bits: u64 = Deserialize::deserialize(deserializer)?;

        match EntityId::from_bits(bits) {
            Some(val) => Ok(val),
            None => Err(Error::custom("The EntityIndex cannot be zero.")),
        }
    }
}
