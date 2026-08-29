//! Entity identifiers.

use core::cmp::Ordering;
use core::fmt::{Debug, Display};
use core::hash::Hash;
use core::mem;
use core::num::NonZeroU32;

use serde::{Deserialize, Serialize};
use zlim_reflect::Reflect;
use zlim_reflect::ops::Opaque;

use crate::table::{TableId, TableRow};

// -----------------------------------------------------------------------------
// EntityId

/// A unique identifier for an entity.
///
/// Composed of a 32-bit index and a non-zero 32-bit generation. The index
/// names the slot the entity occupies, while the generation distinguishes
/// between successive occupants of that slot.
///
/// Entities are frequently created and destroyed, which requires efficient
/// reuse of identifiers. The generation prevents a stale handle from
/// accessing data that now belongs to a different entity: when a slot is
/// recycled its generation is advanced, so old handles no longer match.
///
/// The struct is guaranteed to have the same representation as a `u64`
/// (8-byte aligned) to enable efficient bitwise operations and
/// serialization. The fields are ordered per target endianness so that the
/// index always occupies the low 32 bits and the generation the high 32
/// bits of [`to_bits`], giving consistent behavior across platforms.
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// let mut world = World::alloc();
///
/// // Spawning an entity hands out a fresh, unique `EntityId`.
/// let id = world.spawn_empty(None).id();
///
/// // The id round-trips through its raw `u64` bit pattern.
/// assert_eq!(EntityId::from_bits(id.to_bits()), Some(id));
/// ```
///
/// [`to_bits`]: Self::to_bits
#[repr(C, align(8))]
#[derive(Reflect, Clone, Copy)]
#[reflect(Opaque, Debug, Clone, Eq, Hash, Serialize, Deserialize)]
#[type_path = "zlim_core::entity::EntityId"]
pub struct EntityId {
    #[cfg(target_endian = "little")]
    pub(super) index: u32,
    pub(super) generation: NonZeroU32,
    #[cfg(target_endian = "big")]
    pub(super) index: u32,
}

impl Opaque for EntityId {
    fn apply_str(&mut self, v: &str) -> Result<(), String> {
        match v.parse::<u64>() {
            Ok(v) => match NonZeroU32::try_from((v >> 32) as u32) {
                Ok(generation) => {
                    self.index = v as u32;
                    self.generation = generation;
                    Ok(())
                }
                Err(e) => Err(e.to_string()),
            },
            Err(e) => Err(e.to_string()),
        }
    }

    fn stringify(&self) -> String {
        self.to_bits().to_string()
    }
}

impl EntityId {
    /// A placeholder handle representing an invalid or uninitialized entity.
    ///
    /// Its index is `u32::MAX` and its generation is `u32::MAX`; it is never
    /// equal to a handle returned for a live entity.
    pub const PLACEHOLDER: Self = Self {
        index: u32::MAX,
        generation: NonZeroU32::MAX,
    };

    /// Creates a EntityId with the given `index` and `generation`.
    ///
    /// Valid IDs must be created by allocators, so this function can
    /// only be used for debugging or creating placeholders.
    pub fn new(index: u32, generation: NonZeroU32) -> Self {
        Self { index, generation }
    }

    /// Returns the raw index of this entity.
    ///
    /// The index names the slot the entity occupies and is reused once the
    /// entity is despawned.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let id = world.spawn_empty(None).id();
    ///
    /// // The first spawned entity occupies index 1.
    /// assert_eq!(id.index(), 1);
    /// ```
    #[inline(always)]
    pub const fn index(self) -> u32 {
        self.index
    }

    /// Returns the raw generation of this entity.
    ///
    /// The result is always non-zero; it is advanced whenever the slot is
    /// recycled so stale handles no longer match.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let id = world.spawn_empty(None).id();
    ///
    /// // Freshly spawned entities start with generation 1.
    /// assert_eq!(id.generation().get(), 1);
    /// ```
    #[inline(always)]
    pub const fn generation(self) -> NonZeroU32 {
        self.generation
    }

    /// Reinterprets this [`EntityId`] as its underlying `u64` bit pattern.
    ///
    /// The index occupies the low 32 bits and the generation the high 32
    /// bits, regardless of target endianness. The result round-trips through
    /// [`from_bits`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let id = world.spawn_empty(None).id();
    ///
    /// let bits = id.to_bits();
    /// assert_eq!(bits & 0xFFFF_FFFF, id.index() as u64);
    /// assert_eq!(bits >> 32, id.generation().get() as u64);
    /// ```
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
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// // Low 32 bits: index 2. High 32 bits: generation 1.
    /// let id = EntityId::from_bits(0x0000_0001_0000_0002).unwrap();
    /// assert_eq!(id.index(), 2);
    /// assert_eq!(id.generation().get(), 1);
    ///
    /// // A zero generation is never a valid `EntityId`.
    /// assert!(EntityId::from_bits(0x0000_0000_0000_0002).is_none());
    /// ```
    ///
    /// [`to_bits`]: Self::to_bits
    #[inline]
    pub const fn from_bits(bits: u64) -> Option<Self> {
        const OFFSET: usize = mem::offset_of!(EntityId, generation);

        let ptr: *const u32 = &raw const bits as *const u32;
        if unsafe { *ptr.byte_add(OFFSET) } == 0 {
            core::hint::cold_path();
            None
        } else {
            // SAFETY: the generation part is non-zero, so `bits` is a valid
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
// Location

/// The storage location of a spawned entity.
///
/// Combines the [`TableId`] and [`TableRow`] that identify where the entity's
/// components live within the world's table storage. The location is cleared
/// when the entity is despawned; use [`Entities::locate`] to query it.
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// let mut world = World::alloc();
/// let id = world.spawn((), None).id();
///
/// // A spawned entity's location names the table and row storing its data.
/// let location = world.entities().locate(id).unwrap();
/// assert_eq!(location.table_row.0, 0);
/// ```
///
/// [`Entities::locate`]: crate::entity::Entities::locate
#[derive(Clone, Copy, Hash)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub struct Location {
    /// The table (archetype) the entity currently occupies.
    pub table_id: TableId,
    /// The row within that table holding the entity's components.
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
    use zlim_reflect::ops::Opaque;

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

    #[test]
    fn opaque_impl() {
        let index: u32 = 0x0001_FAAF;
        let generation: u32 = 0x0010_A730;

        let raw: u64 = (index as u64) + ((generation as u64) << 32);
        let id: EntityId = EntityId::from_bits(raw).unwrap();

        let mut mid = id;
        let s = mid.stringify();

        mid.apply_str(&s).unwrap();

        assert_eq!(mid, id);
    }
}

// -----------------------------------------------------------------------------
