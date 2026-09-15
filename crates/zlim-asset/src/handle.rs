//! Asset handles: strong references, stable UUID references and the handle allocator.

use core::any::TypeId;
use core::fmt::{Debug, Formatter};
use core::hash::Hash;
use core::marker::PhantomData;
use std::sync::Arc;

use uuid::Uuid;
use zlim_core::derive::Error;
use zlim_path::TypePath;
use zlim_utils::hash::Equivalent;
use zlim_utils::sync::SegQueue;

use crate::asset::Asset;
use crate::ident::{AssetId, AssetIndex, AssetIndexAllocator, ErasedAssetId};
use crate::path::AssetPath;

// -----------------------------------------------------------------------------
// StrongHandle

/// The internal "strong" [`Asset`] handle storage for [`Handle::Strong`].
///
/// When this is dropped, the [`Asset`] will be freed.
///
/// It also stores some asset metadata for easy access from handles.
#[derive(TypePath, Debug)]
#[type_path = "zlim_asset::handle::StrongHandle"]
pub struct StrongHandle {
    pub(crate) index: AssetIndex,
    pub(crate) type_id: TypeId,
    pub(crate) path: Option<AssetPath<'static>>,
    pub(crate) drop_sender: Arc<SegQueue<DropEvent>>,
    /// `true` when the slot was created by the asset server.
    pub(crate) asset_server_managed: bool,
}

impl Drop for StrongHandle {
    fn drop(&mut self) {
        self.drop_sender.push(DropEvent {
            index: self.index,
            type_id: self.type_id,
            asset_server_managed: self.asset_server_managed,
        });
    }
}

// -----------------------------------------------------------------------------
// DropEvent

/// Notifies the [`AssetHandleProvider`] that one strong handle of an asset was dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DropEvent {
    /// The slot index of the dropped handle.
    pub(crate) index: AssetIndex,
    pub(crate) type_id: TypeId,
    /// Whether the slot was managed by the asset server.
    pub(crate) asset_server_managed: bool,
}

// -----------------------------------------------------------------------------
// AssetHandleProvider

/// Provides [`Handle`] and [`ErasedHandle`] for a specific asset type.
///
/// This should **only** be used for one specific asset type.
#[derive(Clone)]
pub struct AssetHandleProvider {
    allocator: Arc<AssetIndexAllocator>,
    drop_events: Arc<SegQueue<DropEvent>>,
    type_id: TypeId,
}

impl Debug for AssetHandleProvider {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "AssetHandleProvider({:?})", self.type_id)
    }
}

impl AssetHandleProvider {
    /// Creates a provider that shares an existing allocator.
    #[inline]
    pub(crate) fn new(type_id: TypeId, allocator: Arc<AssetIndexAllocator>) -> Self {
        let drop_events = Arc::new(SegQueue::new());
        Self {
            allocator,
            drop_events,
            type_id,
        }
    }

    /// The [`Asset`] type this provider allocates handles for.
    #[inline]
    pub(crate) fn type_id(&self) -> TypeId {
        self.type_id
    }

    /// Pops one pending drop event, if any.
    #[inline]
    pub(crate) fn try_recv(&self) -> Option<DropEvent> {
        self.drop_events.pop()
    }

    /// Returns `true` while a [`StrongHandle`] drop event is waiting to be processed.
    #[inline]
    pub(crate) fn has_drop_event(&self) -> bool {
        !self.drop_events.is_empty()
    }

    /// Allocates a new strong handle (a fresh slot plus its [`StrongHandle`]) for the server.
    pub(crate) fn alloc_handle(
        &self,
        path: Option<AssetPath<'static>>,
        asset_server_managed: bool,
    ) -> Arc<StrongHandle> {
        let index = self.allocator.reserve();
        Arc::new(StrongHandle {
            index,
            type_id: self.type_id,
            path,
            drop_sender: self.drop_events.clone(),
            asset_server_managed,
        })
    }

    /// Builds a strong handle for an already reserved slot.
    pub(crate) fn build_handle(
        &self,
        index: AssetIndex,
        path: Option<AssetPath<'static>>,
        asset_server_managed: bool,
    ) -> Arc<StrongHandle> {
        Arc::new(StrongHandle {
            index,
            type_id: self.type_id,
            path,
            drop_sender: self.drop_events.clone(),
            asset_server_managed,
        })
    }

    /// Reserves a new strong [`ErasedHandle`] (with a new [`ErasedAssetId`]).
    ///
    /// The stored [`Asset`] [`TypeId`] in the [`ErasedHandle`] will match
    /// the [`Asset`] [`TypeId`] assigned to this [`AssetHandleProvider`].
    #[must_use]
    pub fn reserve_handle(&self) -> ErasedHandle {
        let index = self.allocator.reserve();
        ErasedHandle::Strong(self.build_handle(index, None, false))
    }
}

// -----------------------------------------------------------------------------
// Handle

/// A handle to a specific [`Asset`] of type `A`.
///
/// Handles act as abstract "references" to assets, whose data are stored
/// in the [`Assets<A>`] resource, avoiding the need to store multiple
/// copies of the same data.
///
/// - If a [`Handle`] is [`Handle::Strong`], the [`Asset`] will be
///   kept alive until the [`Handle`] is dropped.
///
/// - If a [`Handle`] is [`Handle::Uuid`], it does not necessarily
///   reference a live [`Asset`], nor will it keep assets alive.
///
/// Modifying a *handle* will change which existing asset is referenced,
/// but modifying the *asset* (by mutating the [`Assets`] resource) will
/// change the asset for all handles referencing it.
///
/// [`Handle`] can be cloned. If a [`Handle::Strong`] is cloned, the
/// referenced [`Asset`] will not be freed until _all_ instances of the
/// [`Handle`] are dropped.
///
/// [`Handle::Strong`], via [`StrongHandle`] also provides access to useful
/// [`Asset`] metadata, such as the [`AssetPath`] (if it exists).
///
/// [`Assets`]: crate::assets::Assets
/// [`Assets<A>`]: crate::assets::Assets
#[derive(TypePath)]
pub enum Handle<A: Asset> {
    /// A "strong" reference to a live (or loading) [`Asset`].
    ///
    /// If a [`Handle`] is [`Handle::Strong`], the [`Asset`]
    /// will be kept alive until the [`Handle`] is dropped.
    Strong(Arc<StrongHandle>),

    /// A "uuid" reference to an [`Asset`] using a stable-across-runs / const identifier.
    ///
    /// Dropping this handle will not result in the asset being dropped.
    Uuid(Uuid, PhantomData<fn() -> A>),
}

impl<A: Asset> Handle<A> {
    /// The id of the referenced asset.
    #[inline]
    pub fn id(&self) -> AssetId<A> {
        match self {
            Self::Strong(handle) => AssetId::Index {
                index: handle.index,
                marker: PhantomData,
            },
            Self::Uuid(uuid, _) => AssetId::Uuid { uuid: *uuid },
        }
    }

    /// The path this asset was loaded from, if it is a server-managed strong handle.
    #[inline]
    pub fn path(&self) -> Option<&AssetPath<'static>> {
        match self {
            Self::Strong(handle) => handle.path.as_ref(),
            Self::Uuid(..) => None,
        }
    }

    /// Returns `true` if this is a strong handle.
    #[inline]
    pub const fn is_strong(&self) -> bool {
        matches!(self, Self::Strong(_))
    }

    /// Returns `true` if this is a UUID handle.
    #[inline]
    pub const fn is_uuid(&self) -> bool {
        matches!(self, Self::Uuid(..))
    }

    /// Erases the asset type, keeping the type id so the handle can be typed back.
    #[inline]
    pub fn erased(&self) -> ErasedHandle {
        match self {
            Self::Strong(handle) => ErasedHandle::Strong(Arc::clone(handle)),
            Self::Uuid(uuid, _) => {
                let type_id = TypeId::of::<A>();
                let uuid = *uuid;
                ErasedHandle::Uuid { type_id, uuid }
            }
        }
    }
}

impl<A: Asset> Clone for Handle<A> {
    fn clone(&self) -> Self {
        match self {
            Self::Strong(handle) => Self::Strong(Arc::clone(handle)),
            Self::Uuid(uuid, _) => Self::Uuid(*uuid, PhantomData),
        }
    }
}

impl<A: Asset> Default for Handle<A> {
    #[inline]
    fn default() -> Self {
        Handle::Uuid(AssetId::<A>::DEFAULT_UUID, PhantomData)
    }
}

impl<A: Asset> Debug for Handle<A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "Handle<{}>", A::type_name())?;
        match self {
            Self::Strong(handle) => {
                let i = handle.index;
                if let Some(p) = &handle.path {
                    write!(f, "{{ index: {i}, path: {p} }}")
                } else {
                    write!(f, "{{ index: {i}, path: None }}")
                }
            }
            Self::Uuid(uuid, _) => write!(f, "{{ uuid: {uuid} }}"),
        }
    }
}

impl<A: Asset> Hash for Handle<A> {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

impl<A: Asset> PartialEq for Handle<A> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl<A: Asset> Eq for Handle<A> {}

impl<A: Asset> PartialOrd for Handle<A> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<A: Asset> Ord for Handle<A> {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.id().cmp(&other.id())
    }
}

/// Lets `HashMap<Handle<A>, _>` be queried with a borrowed [`AssetId<A>`].
impl<A: Asset> Equivalent<Handle<A>> for AssetId<A> {
    #[inline]
    fn equivalent(&self, key: &Handle<A>) -> bool {
        *self == key.id()
    }
}

impl<A: Asset> From<&Handle<A>> for AssetId<A> {
    #[inline]
    fn from(value: &Handle<A>) -> Self {
        value.id()
    }
}

impl<A: Asset> From<&Handle<A>> for ErasedAssetId {
    #[inline]
    fn from(value: &Handle<A>) -> Self {
        value.id().erased()
    }
}

impl<A: Asset> From<&mut Handle<A>> for AssetId<A> {
    #[inline]
    fn from(value: &mut Handle<A>) -> Self {
        value.id()
    }
}

impl<A: Asset> From<&mut Handle<A>> for ErasedAssetId {
    #[inline]
    fn from(value: &mut Handle<A>) -> Self {
        value.id().erased()
    }
}

impl<A: Asset> From<Uuid> for Handle<A> {
    #[inline]
    fn from(uuid: Uuid) -> Self {
        Handle::Uuid(uuid, PhantomData)
    }
}

// -----------------------------------------------------------------------------
// ErasedHandle

/// A handle whose asset type is only known at runtime, but **is** recorded.
///
/// This allows handles across [`Asset`] types to be stored together and compared.
#[derive(Clone)]
pub enum ErasedHandle {
    /// A reference-counted slot reference.
    Strong(Arc<StrongHandle>),

    /// A stable UUID reference.
    Uuid {
        /// The referenced UUID.
        uuid: Uuid,
        /// The concrete asset type.
        type_id: TypeId,
    },
}

impl ErasedHandle {
    /// The default UUID handle for a given asset type.
    #[inline]
    pub const fn default_for_type(type_id: TypeId) -> Self {
        let uuid = AssetId::<()>::DEFAULT_UUID;
        Self::Uuid { uuid, type_id }
    }

    /// The concrete asset type this handle refers to.
    #[inline]
    pub fn type_id(&self) -> TypeId {
        match self {
            Self::Strong(handle) => handle.type_id,
            Self::Uuid { type_id, .. } => *type_id,
        }
    }

    /// The id of the referenced asset.
    #[inline]
    pub fn id(&self) -> ErasedAssetId {
        match self {
            Self::Strong(handle) => ErasedAssetId::Index {
                type_id: handle.type_id,
                index: handle.index,
            },
            Self::Uuid { type_id, uuid } => ErasedAssetId::Uuid {
                type_id: *type_id,
                uuid: *uuid,
            },
        }
    }

    /// The path this asset was loaded from, if it is a server-managed strong handle.
    #[inline]
    pub fn path(&self) -> Option<&AssetPath<'static>> {
        match self {
            Self::Strong(handle) => handle.path.as_ref(),
            Self::Uuid { .. } => None,
        }
    }

    /// Returns `true` if this is a strong handle.
    #[inline]
    pub const fn is_strong(&self) -> bool {
        matches!(self, Self::Strong(_))
    }

    /// Returns `true` if this is a UUID handle.
    #[inline]
    pub const fn is_uuid(&self) -> bool {
        matches!(self, Self::Uuid { .. })
    }

    /// Types this handle back **without** checking the asset type.
    #[inline]
    pub fn with_type_unchecked<A: Asset>(self) -> Handle<A> {
        match self {
            Self::Strong(handle) => Handle::Strong(handle),
            Self::Uuid { uuid, .. } => Handle::Uuid(uuid, PhantomData),
        }
    }

    /// Types this handle back, asserting in debug builds that the type matches.
    #[inline]
    pub fn with_type_debug_checked<A: Asset>(self) -> Handle<A> {
        debug_assert_eq!(
            self.type_id(),
            TypeId::of::<A>(),
            "The target Handle<{}>'s TypeId does not match this ErasedHandle",
            core::any::type_name::<A>(),
        );
        self.with_type_unchecked()
    }

    /// Types this handle back, panicking when the asset type does not match.
    #[inline]
    #[track_caller]
    pub fn with_type<A: Asset>(self) -> Handle<A> {
        #[cold]
        #[inline(never)]
        #[track_caller]
        fn mismatch(name: &'static str) -> ! {
            panic!("The target Handle<{name}>'s TypeId does not match this ErasedHandle")
        }

        match self.try_with_type::<A>() {
            Ok(handle) => handle,
            Err(_) => mismatch(core::any::type_name::<A>()),
        }
    }

    /// Types this handle back, returning an error when the asset type does not match.
    #[inline]
    pub fn try_with_type<A: Asset>(self) -> Result<Handle<A>, AssetHandleTypeError> {
        let actual = self.type_id();
        let expect = TypeId::of::<A>();

        if actual != expect {
            ::core::hint::cold_path();
            return Err(AssetHandleTypeError {
                type_name: core::any::type_name::<A>(),
                expect,
                actual,
            });
        }

        Ok(self.with_type_unchecked())
    }
}

impl Debug for ErasedHandle {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut writer = f.debug_struct("ErasedHandle");
        match self {
            Self::Strong(handle) => {
                writer
                    .field("type_id", &handle.type_id)
                    .field("index", &handle.index)
                    .field("path", &handle.path);
            }
            Self::Uuid { type_id, uuid } => {
                writer.field("type_id", type_id).field("uuid", uuid);
            }
        }
        writer.finish()
    }
}

impl Hash for ErasedHandle {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

impl PartialEq for ErasedHandle {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Eq for ErasedHandle {}

impl PartialOrd for ErasedHandle {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ErasedHandle {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.id().cmp(&other.id())
    }
}

impl<A: Asset> From<Handle<A>> for ErasedHandle {
    #[inline]
    fn from(value: Handle<A>) -> Self {
        value.erased()
    }
}

impl<A: Asset> From<&Handle<A>> for ErasedHandle {
    #[inline]
    fn from(value: &Handle<A>) -> Self {
        value.erased()
    }
}

impl<A: Asset> From<&mut Handle<A>> for ErasedHandle {
    #[inline]
    fn from(value: &mut Handle<A>) -> Self {
        value.erased()
    }
}

impl<A: Asset> TryFrom<ErasedHandle> for Handle<A> {
    type Error = AssetHandleTypeError;

    #[inline]
    fn try_from(value: ErasedHandle) -> Result<Self, Self::Error> {
        value.try_with_type()
    }
}

// -----------------------------------------------------------------------------
// Cross Operations

impl<A: Asset> PartialEq<ErasedHandle> for Handle<A> {
    #[inline]
    fn eq(&self, other: &ErasedHandle) -> bool {
        TypeId::of::<A>() == other.type_id() && self.id() == other.id()
    }
}

impl<A: Asset> PartialEq<Handle<A>> for ErasedHandle {
    #[inline]
    fn eq(&self, other: &Handle<A>) -> bool {
        TypeId::of::<A>() == self.type_id() && self.id() == other.id()
    }
}

impl<A: Asset> PartialOrd<ErasedHandle> for Handle<A> {
    #[inline]
    fn partial_cmp(&self, other: &ErasedHandle) -> Option<core::cmp::Ordering> {
        if TypeId::of::<A>() != other.type_id() {
            None
        } else {
            self.id().partial_cmp(&other.id())
        }
    }
}

impl<A: Asset> PartialOrd<Handle<A>> for ErasedHandle {
    #[inline]
    fn partial_cmp(&self, other: &Handle<A>) -> Option<core::cmp::Ordering> {
        Some(other.partial_cmp(self)?.reverse())
    }
}

impl From<&ErasedHandle> for ErasedAssetId {
    #[inline]
    fn from(value: &ErasedHandle) -> Self {
        value.id()
    }
}

// -----------------------------------------------------------------------------
// TypedAssetIndex

use crate::ident::{TypedAssetIndex, UuidNotSupportedError};

impl<A: Asset> TryFrom<&Handle<A>> for TypedAssetIndex {
    type Error = UuidNotSupportedError;

    #[inline]
    fn try_from(handle: &Handle<A>) -> Result<Self, Self::Error> {
        match handle {
            Handle::Strong(handle) => Ok(Self::new(handle.index, handle.type_id)),
            Handle::Uuid(uuid, _) => Err(UuidNotSupportedError(*uuid)),
        }
    }
}

impl TryFrom<&ErasedHandle> for TypedAssetIndex {
    type Error = UuidNotSupportedError;

    #[inline]
    fn try_from(handle: &ErasedHandle) -> Result<Self, Self::Error> {
        match handle {
            ErasedHandle::Strong(handle) => Ok(Self::new(handle.index, handle.type_id)),
            ErasedHandle::Uuid { uuid, .. } => Err(UuidNotSupportedError(*uuid)),
        }
    }
}

// -----------------------------------------------------------------------------
// Errors

/// Returned when an [`ErasedHandle`] is typed back as the wrong asset type.
#[derive(Error, Debug, Clone)]
#[error("ErasedHandle({actual:?}) cannot be converted into Handle<{type_name}>({expect:?})")]
pub struct AssetHandleTypeError {
    /// The (debug) type name we tried to convert to.
    type_name: &'static str,
    /// The type id we tried to convert to.
    expect: TypeId,
    /// The type id we tried to convert from.
    actual: TypeId,
}

impl AssetHandleTypeError {
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
// uuid_handle!

/// Creates a [`Handle`] from a string literal containing a UUID.
///
/// # Examples
///
/// ```
/// # use zlim_asset::handle::Handle;
/// # use zlim_asset::uuid_handle;
/// # type Image = ();
/// const IMAGE: Handle<Image> = uuid_handle!("1347c9b7-c46a-48e7-b7b8-023a354b7cac");
#[macro_export]
macro_rules! uuid_handle {
    ($uuid:expr) => {
        $crate::handle::Handle::Uuid($crate::uuid::uuid!($uuid), ::core::marker::PhantomData)
    };
}

// -----------------------------------------------------------------------------
