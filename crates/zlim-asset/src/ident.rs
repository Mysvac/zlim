//! Provides asset index implementation.

use core::any::TypeId;
use core::fmt::{Debug, Display, Formatter};
use core::hash::Hash;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use uuid::Uuid;
use zlim_core::derive::Error;
use zlim_utils::str::SmolStr;
use zlim_utils::sync::SegQueue;

use crate::asset::Asset;

// -----------------------------------------------------------------------------
// AssetSourceId

/// Identifies the [`AssetSource`] that owns an asset path.
///
/// - [`Default`]: the unnamed primary source (e.g. `assets/`).
/// - [`Name`]: a named secondary source registered with the asset server.
///
/// [`Default`]: AssetSourceId::Default
/// [`Name`]: AssetSourceId::Name
/// [`AssetSource`]: crate::source::AssetSource
#[derive(Default, Clone, Debug, Eq)]
pub enum AssetSourceId {
    /// The unnamed primary source, which is the one an asset path without a `source://` names.
    #[default]
    Default,
    /// A named secondary source, registered with the asset server under this name.
    Name(SmolStr),
}

impl AssetSourceId {
    /// Creates a new [`AssetSourceId`]: `None` becomes [`AssetSourceId::Default`], `Some(name)`
    /// becomes [`AssetSourceId::Name`].
    pub fn new(source: Option<impl Into<SmolStr>>) -> AssetSourceId {
        match source {
            Some(source) => AssetSourceId::Name(source.into()),
            None => AssetSourceId::Default,
        }
    }

    /// Returns the source name, or [`None`] for [`AssetSourceId::Default`].
    #[inline]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            AssetSourceId::Default => None,
            AssetSourceId::Name(v) => Some(v),
        }
    }
}

impl Hash for AssetSourceId {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl PartialEq for AssetSourceId {
    fn eq(&self, other: &Self) -> bool {
        self.as_str().eq(&other.as_str())
    }
}

impl Display for AssetSourceId {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self.as_str() {
            None => f.write_str("AssetSourceId::Default"),
            Some(v) => write!(f, "AssetSourceId::Name({v})"),
        }
    }
}

impl From<&AssetSourceId> for AssetSourceId {
    fn from(value: &AssetSourceId) -> Self {
        value.clone()
    }
}

// This is only implemented for `&'static str` because that is the input `SmolStr::new` takes:
// it is the constructor that never allocates, so a static source name is the cheap case.
impl From<&'static str> for AssetSourceId {
    #[inline]
    fn from(value: &'static str) -> Self {
        AssetSourceId::Name(SmolStr::new(value))
    }
}

impl From<&String> for AssetSourceId {
    fn from(value: &String) -> Self {
        AssetSourceId::Name(SmolStr::from_str(value))
    }
}

impl From<String> for AssetSourceId {
    fn from(value: String) -> Self {
        AssetSourceId::Name(SmolStr::from_str(&value))
    }
}

impl From<Arc<str>> for AssetSourceId {
    fn from(value: Arc<str>) -> Self {
        AssetSourceId::Name(SmolStr::from(value))
    }
}

impl From<Option<&'static str>> for AssetSourceId {
    fn from(value: Option<&'static str>) -> Self {
        match value {
            None => AssetSourceId::Default,
            Some(value) => AssetSourceId::Name(SmolStr::new(value)),
        }
    }
}

impl From<Option<SmolStr>> for AssetSourceId {
    #[inline]
    fn from(value: Option<SmolStr>) -> Self {
        match value {
            None => AssetSourceId::Default,
            Some(v) => AssetSourceId::Name(v),
        }
    }
}

impl From<AssetSourceId> for Option<SmolStr> {
    fn from(value: AssetSourceId) -> Self {
        match value {
            AssetSourceId::Default => None,
            AssetSourceId::Name(smol_str) => Some(smol_str),
        }
    }
}

impl From<()> for AssetSourceId {
    #[inline]
    fn from(_: ()) -> Self {
        AssetSourceId::Default
    }
}

// -----------------------------------------------------------------------------
// AssetIndex

/// A generational runtime-only identifier for a specific [`Asset`] stored in [`Assets`].
///
/// This is a **runtime-local** value: the allocator starts from zero in every process, and the
/// generation only guards against a slot being reused *within one run*.
///
/// Persisting it is therefore meaningless (the same bits may point at a different
/// asset next time), which is why it deliberately implements neither `Serialize`
/// nor `Deserialize`. For anything that crosses a process boundary use [`AssetId::Uuid`].
///
/// The bit comparison is still exposed through [`to_bits`](Self::to_bits): it is what `Ord` and
/// `Hash` use, and it makes the ordering independent of the target's endianness.
///
/// [`Assets`]: crate::assets::Assets
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C, align(8))]
pub struct AssetIndex {
    #[cfg(target_endian = "little")]
    pub(crate) index: u32,
    pub(crate) generation: u32,
    #[cfg(target_endian = "big")]
    pub(crate) index: u32,
}

impl AssetIndex {
    /// Packs the slot position and generation into a single integer.
    #[inline(always)]
    pub const fn to_bits(self) -> u64 {
        #[expect(unsafe_code, reason = "Stable Conversion")]
        unsafe {
            core::mem::transmute::<Self, u64>(self)
        }
    }

    /// Unpacks an index from [`to_bits`](AssetIndex::to_bits) output.
    #[inline(always)]
    pub const fn from_bits(bits: u64) -> Self {
        #[expect(unsafe_code, reason = "Stable Conversion")]
        unsafe {
            core::mem::transmute::<u64, Self>(bits)
        }
    }
}

impl PartialOrd for AssetIndex {
    #[inline(always)]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AssetIndex {
    #[inline(always)]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.to_bits().cmp(&other.to_bits())
    }
}

impl Hash for AssetIndex {
    #[inline(always)]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.to_bits());
    }
}

impl Debug for AssetIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self, f)
    }
}

impl Display for AssetIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}v{}", self.index, self.generation)
    }
}

// -----------------------------------------------------------------------------
// AssetIndexAllocator

/// Lock-free allocator for [`AssetIndex`] values.
pub(crate) struct AssetIndexAllocator {
    /// The next fresh slot to hand out; also the high-water mark `AssetTable::flush` grows
    /// `storage` to.
    pub next_index: AtomicU32,
    /// Slots returned by `recycle`, waiting for `reserve` to pick one up and bump its generation.
    pub recycled_queue: SegQueue<AssetIndex>,
    /// Slots `reserve` bumped, waiting for `AssetTable::flush` to install them at the new
    /// generation. See `AssetTable::flush`.
    pub recycled: SegQueue<AssetIndex>,
}

impl Debug for AssetIndexAllocator {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AssetIndexAllocator")
            .field("next_index", &self.next_index.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl AssetIndexAllocator {
    /// Highest usable slot position; `reserve` panics instead of handing out a slot beyond it.
    pub(crate) const MAX_ASSET_INDEX: u32 = i32::MAX as u32;

    /// Creates an empty allocator.
    pub(crate) const fn new() -> Self {
        Self {
            next_index: AtomicU32::new(0),
            recycled_queue: SegQueue::new(),
            recycled: SegQueue::new(),
        }
    }

    /// Reserves an [`AssetIndex`], reusing a recycled slot when one is available.
    pub(crate) fn reserve(&self) -> AssetIndex {
        #[cold]
        #[inline(never)]
        fn too_many_assets(next_index: &AtomicU32) -> ! {
            next_index.fetch_sub(1, Ordering::Relaxed);
            panic!("too many assets");
        }

        if let Some(mut recycled) = self.recycled_queue.pop() {
            recycled.generation = recycled.generation.wrapping_add(1);
            self.recycled.push(recycled);
            return recycled;
        }

        let index = self.next_index.fetch_add(1, Ordering::Relaxed);

        if index > Self::MAX_ASSET_INDEX {
            too_many_assets(&self.next_index);
        }

        AssetIndex {
            index,
            generation: 0,
        }
    }

    /// Returns a slot to the allocator so it can be handed out again with a bumped generation.
    pub(crate) fn recycle(&self, index: AssetIndex) {
        self.recycled_queue.push(index);
    }
}

// -----------------------------------------------------------------------------
// AssetId

/// An identifier for an [`Asset`]: either a runtime slot index or a stable UUID.
///
/// This is cheap to [`Copy`]/[`Clone`] and is not directly tied to the lifetime
/// of the Asset. This means it _can_ point to an [`Asset`] that no longer exists.
///
/// Only [`AssetId::Index`] is runtime-local; [`AssetId::Uuid`] is the form to use for a value that
/// crosses a process boundary.
///
/// For an identifier tied to the lifetime of an asset, see [`Handle`].
///
/// [`Handle`]: crate::handle::Handle
pub enum AssetId<A: Asset> {
    /// A runtime slot index.
    Index {
        /// The generation-aware slot index.
        index: AssetIndex,
        marker: PhantomData<fn() -> A>,
    },
    /// A stable UUID reference.
    Uuid {
        /// The UUID this id refers to.
        uuid: Uuid,
    },
}

const DEFAULT_D4: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 1];

impl<A: Asset> AssetId<A> {
    /// The UUID used by [`AssetId::default`].
    pub const DEFAULT_UUID: Uuid = Uuid::from_fields(u32::MAX, u16::MAX, u16::MAX, &DEFAULT_D4);

    /// Returns the slot index when this is an [`AssetId::Index`].
    #[inline]
    pub const fn index(&self) -> Option<AssetIndex> {
        match self {
            Self::Index { index, .. } => Some(*index),
            Self::Uuid { .. } => None,
        }
    }

    /// Returns the UUID when this is an [`AssetId::Uuid`].
    #[inline]
    pub const fn uuid(&self) -> Option<Uuid> {
        match self {
            Self::Index { .. } => None,
            Self::Uuid { uuid } => Some(*uuid),
        }
    }

    /// Returns `true` if this id refers to a UUID asset.
    #[inline]
    pub const fn is_uuid(&self) -> bool {
        matches!(self, Self::Uuid { .. })
    }

    /// Erases the asset type, keeping enough information to type it back.
    #[inline]
    pub const fn erased(self) -> ErasedAssetId {
        let type_id = TypeId::of::<A>();
        match self {
            Self::Index { index, .. } => ErasedAssetId::Index { type_id, index },
            Self::Uuid { uuid } => ErasedAssetId::Uuid { type_id, uuid },
        }
    }
}

impl<A: Asset> Clone for AssetId<A> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: Asset> Copy for AssetId<A> {}

impl<A: Asset> Default for AssetId<A> {
    #[inline]
    fn default() -> Self {
        Self::Uuid {
            uuid: Self::DEFAULT_UUID,
        }
    }
}

impl<A: Asset> Debug for AssetId<A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "AssetId<{}>", A::type_path())?;
        match self {
            Self::Index { index, .. } => write!(f, "{{ index: {index} }}"),
            Self::Uuid { uuid } => write!(f, "{{ uuid: {uuid} }}"),
        }
    }
}

impl<A: Asset> Display for AssetId<A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "AssetId<{}>", A::type_name())?;
        match self {
            Self::Index { index, .. } => write!(f, "{{ index: {index} }}"),
            Self::Uuid { uuid } => write!(f, "{{ uuid: {uuid} }}"),
        }
    }
}

impl<A: Asset> Hash for AssetId<A> {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        // Mix in the type so that ids of different asset types do not collide
        // when a caller stores them in one map keyed by a type-erased id.
        TypeId::of::<A>().hash(state);
        match self {
            Self::Index { index, .. } => index.hash(state),
            Self::Uuid { uuid } => uuid.hash(state),
        }
    }
}

impl<A: Asset> PartialEq for AssetId<A> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Index { index: x, .. }, Self::Index { index: y, .. }) => x == y,
            (Self::Uuid { uuid: x }, Self::Uuid { uuid: y }) => x == y,
            _ => false,
        }
    }
}

impl<A: Asset> Eq for AssetId<A> {}

impl<A: Asset> PartialOrd for AssetId<A> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<A: Asset> Ord for AssetId<A> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match (self, other) {
            (Self::Index { index: x, .. }, Self::Index { index: y, .. }) => x.cmp(y),
            (Self::Uuid { uuid: x }, Self::Uuid { uuid: y }) => x.cmp(y),
            // Index ids sort before UUID ids, mirroring the "runtime before stable" order.
            (Self::Index { .. }, Self::Uuid { .. }) => core::cmp::Ordering::Less,
            (Self::Uuid { .. }, Self::Index { .. }) => core::cmp::Ordering::Greater,
        }
    }
}

impl<A: Asset> From<AssetIndex> for AssetId<A> {
    #[inline]
    fn from(index: AssetIndex) -> Self {
        Self::Index {
            index,
            marker: PhantomData,
        }
    }
}

impl<A: Asset> From<Uuid> for AssetId<A> {
    #[inline]
    fn from(uuid: Uuid) -> Self {
        Self::Uuid { uuid }
    }
}

// -----------------------------------------------------------------------------
// ErasedAssetId

/// A type-erased asset identifier that still records the concrete asset [`TypeId`].
///
/// This increases the size of the type, but it enables storing asset ids
/// across asset types together and enables comparisons between them.
#[derive(Clone, Copy)]
pub enum ErasedAssetId {
    /// A runtime slot index plus its asset type.
    Index {
        /// The concrete asset type.
        type_id: TypeId,
        /// The generation-aware slot index.
        index: AssetIndex,
    },
    /// A UUID reference plus its asset type.
    Uuid {
        /// The concrete asset type.
        type_id: TypeId,
        /// The UUID this id refers to.
        uuid: Uuid,
    },
}

impl ErasedAssetId {
    /// The concrete asset type this id refers to.
    #[inline(always)]
    pub const fn type_id(&self) -> TypeId {
        match self {
            Self::Index { type_id, .. } | Self::Uuid { type_id, .. } => *type_id,
        }
    }

    /// Returns `true` if this id refers to a runtime asset.
    #[inline]
    pub const fn is_index(&self) -> bool {
        matches!(self, Self::Index { .. })
    }

    /// Returns `true` if this id refers to a UUID asset.
    #[inline]
    pub const fn is_uuid(&self) -> bool {
        matches!(self, Self::Uuid { .. })
    }

    /// Returns the slot index when this is an [`ErasedAssetId::Index`].
    #[inline]
    pub const fn index(&self) -> Option<AssetIndex> {
        match self {
            Self::Index { index, .. } => Some(*index),
            Self::Uuid { .. } => None,
        }
    }

    /// Returns the UUID when this is an [`ErasedAssetId::Uuid`].
    #[inline]
    pub const fn uuid(&self) -> Option<Uuid> {
        match self {
            Self::Uuid { uuid, .. } => Some(*uuid),
            Self::Index { .. } => None,
        }
    }

    /// Types this id back **without** checking the asset type.
    #[inline]
    pub const fn with_type_unchecked<A: Asset>(self) -> AssetId<A> {
        match self {
            Self::Index { index, .. } => AssetId::Index {
                index,
                marker: PhantomData,
            },
            Self::Uuid { uuid, .. } => AssetId::Uuid { uuid },
        }
    }

    /// Types this id back, asserting in debug builds that the type matches.
    #[inline]
    pub fn with_type_debug_checked<A: Asset>(self) -> AssetId<A> {
        debug_assert_eq!(
            self.type_id(),
            TypeId::of::<A>(),
            "The target AssetId<{}>'s TypeId does not match this ErasedAssetId",
            core::any::type_name::<A>(),
        );
        self.with_type_unchecked()
    }

    /// Types this id back, panicking when the asset type does not match.
    #[inline]
    #[track_caller]
    pub fn with_type<A: Asset>(self) -> AssetId<A> {
        #[cold]
        #[inline(never)]
        #[track_caller]
        fn mismatch(name: &'static str) -> ! {
            panic!("The target AssetId<{name}>'s TypeId does not match this ErasedAssetId")
        }

        match self.try_with_type::<A>() {
            Ok(id) => id,
            Err(_) => mismatch(core::any::type_name::<A>()),
        }
    }

    /// Types this id back, returning an error when the asset type does not match.
    #[inline]
    pub fn try_with_type<A: Asset>(self) -> Result<AssetId<A>, AssetIdTypeError> {
        let actual = self.type_id();
        let expect = TypeId::of::<A>();

        if actual != expect {
            ::core::hint::cold_path();
            return Err(AssetIdTypeError {
                type_name: core::any::type_name::<A>(),
                expect,
                actual,
            });
        }

        Ok(self.with_type_unchecked())
    }
}

impl Hash for ErasedAssetId {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        match self {
            Self::Index { type_id, index } => {
                type_id.hash(state);
                index.hash(state);
            }
            Self::Uuid { type_id, uuid } => {
                type_id.hash(state);
                uuid.hash(state);
            }
        }
    }
}

impl PartialEq for ErasedAssetId {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Index {
                    type_id: t1,
                    index: i1,
                },
                Self::Index {
                    type_id: t2,
                    index: i2,
                },
            ) => t1 == t2 && i1 == i2,
            (
                Self::Uuid {
                    type_id: t1,
                    uuid: u1,
                },
                Self::Uuid {
                    type_id: t2,
                    uuid: u2,
                },
            ) => t1 == t2 && u1 == u2,
            _ => false,
        }
    }
}

impl Eq for ErasedAssetId {}

impl PartialOrd for ErasedAssetId {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ErasedAssetId {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match (self, other) {
            (
                Self::Index {
                    type_id: t1,
                    index: i1,
                },
                Self::Index {
                    type_id: t2,
                    index: i2,
                },
            ) => t1.cmp(t2).then_with(|| i1.cmp(i2)),
            (
                Self::Uuid {
                    type_id: t1,
                    uuid: u1,
                },
                Self::Uuid {
                    type_id: t2,
                    uuid: u2,
                },
            ) => t1.cmp(t2).then_with(|| u1.cmp(u2)),
            (Self::Index { .. }, Self::Uuid { .. }) => core::cmp::Ordering::Less,
            (Self::Uuid { .. }, Self::Index { .. }) => core::cmp::Ordering::Greater,
        }
    }
}

impl Debug for ErasedAssetId {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self, f)
    }
}

impl Display for ErasedAssetId {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut writer = f.debug_struct("ErasedAssetId");
        match self {
            Self::Index { type_id, index } => {
                writer.field("type_id", type_id).field("index", index);
            }
            Self::Uuid { type_id, uuid } => {
                writer.field("type_id", type_id).field("uuid", uuid);
            }
        }
        writer.finish()
    }
}

impl<A: Asset> From<AssetId<A>> for ErasedAssetId {
    #[inline]
    fn from(value: AssetId<A>) -> Self {
        value.erased()
    }
}

impl<A: Asset> TryFrom<ErasedAssetId> for AssetId<A> {
    type Error = AssetIdTypeError;

    fn try_from(value: ErasedAssetId) -> Result<Self, Self::Error> {
        value.try_with_type::<A>()
    }
}

impl<A: Asset> PartialEq<ErasedAssetId> for AssetId<A> {
    #[inline]
    fn eq(&self, other: &ErasedAssetId) -> bool {
        other.eq(self)
    }
}

impl<A: Asset> PartialEq<AssetId<A>> for ErasedAssetId {
    fn eq(&self, other: &AssetId<A>) -> bool {
        if self.type_id() != TypeId::of::<A>() {
            return false;
        }
        match (self, other) {
            (Self::Index { index: i1, .. }, AssetId::Index { index: i2, .. }) => i1 == i2,
            (Self::Uuid { uuid: u1, .. }, AssetId::Uuid { uuid: u2 }) => u1 == u2,
            _ => false,
        }
    }
}

impl<A: Asset> PartialOrd<ErasedAssetId> for AssetId<A> {
    #[inline]
    fn partial_cmp(&self, other: &ErasedAssetId) -> Option<core::cmp::Ordering> {
        if TypeId::of::<A>() != other.type_id() {
            return None;
        }
        match (self, other) {
            (Self::Index { index: i1, .. }, ErasedAssetId::Index { index: i2, .. }) => {
                Some(i1.cmp(i2))
            }
            (Self::Uuid { uuid: u1 }, ErasedAssetId::Uuid { uuid: u2, .. }) => Some(u1.cmp(u2)),
            (Self::Index { .. }, ErasedAssetId::Uuid { .. }) => Some(core::cmp::Ordering::Less),
            (Self::Uuid { .. }, ErasedAssetId::Index { .. }) => Some(core::cmp::Ordering::Greater),
        }
    }
}

impl<A: Asset> PartialOrd<AssetId<A>> for ErasedAssetId {
    #[inline]
    fn partial_cmp(&self, other: &AssetId<A>) -> Option<core::cmp::Ordering> {
        other.partial_cmp(self).map(core::cmp::Ordering::reverse)
    }
}

// -----------------------------------------------------------------------------
// TypedAssetIndex

/// An asset index bundled with its (dynamic) type.
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub struct TypedAssetIndex {
    /// The concrete asset type.
    pub type_id: TypeId,
    /// The generation-aware slot index.
    pub index: AssetIndex,
}

impl TypedAssetIndex {
    /// Creates a typed index from the slot position and the asset type it belongs to — in that
    /// order, which is the reverse of the field order.
    #[inline(always)]
    pub const fn new(index: AssetIndex, type_id: TypeId) -> Self {
        Self { index, type_id }
    }
}

impl Display for TypedAssetIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TypedAssetIndex")
            .field("index", &self.index)
            .field("type_id", &self.type_id)
            .finish()
    }
}

impl From<TypedAssetIndex> for ErasedAssetId {
    fn from(value: TypedAssetIndex) -> Self {
        Self::Index {
            type_id: value.type_id,
            index: value.index,
        }
    }
}

impl TryFrom<ErasedAssetId> for TypedAssetIndex {
    type Error = UuidNotSupportedError;

    #[inline]
    fn try_from(asset_id: ErasedAssetId) -> Result<Self, Self::Error> {
        match asset_id {
            ErasedAssetId::Index { type_id, index } => Ok(Self { index, type_id }),
            ErasedAssetId::Uuid { uuid, .. } => Err(UuidNotSupportedError(uuid)),
        }
    }
}

// -----------------------------------------------------------------------------
// Errors

/// Returned when a UUID asset id is used where an index-backed id is required.
#[derive(Error, Debug, Clone)]
#[error("Attempted to create a TypedAssetIndex from a Uuid({_0})")]
pub struct UuidNotSupportedError(pub(crate) Uuid);

/// Returned when an [`ErasedAssetId`] is typed back as the wrong asset type.
#[derive(Error, Debug, Clone)]
#[error("ErasedAssetId({actual:?}) cannot be converted into AssetId<{type_name}>({expect:?})")]
pub struct AssetIdTypeError {
    /// The (debug) type name we tried to convert to.
    type_name: &'static str,
    /// The type id we tried to convert to.
    expect: TypeId,
    /// The type id we tried to convert from.
    actual: TypeId,
}

impl AssetIdTypeError {
    /// The expected asset type.
    #[inline]
    pub const fn expected(&self) -> TypeId {
        self.expect
    }

    /// The actual asset type.
    #[inline]
    pub const fn actual(&self) -> TypeId {
        self.actual
    }
}

// -----------------------------------------------------------------------------
