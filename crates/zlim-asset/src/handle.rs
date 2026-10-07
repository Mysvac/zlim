//! Asset handles: strong references, stable UUID references and the handle allocator.

use core::any::TypeId;
use core::fmt::Display;
use core::fmt::{Debug, Formatter};
use core::hash::Hash;
use core::marker::PhantomData;
use std::sync::Arc;

use serde::de::Visitor;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zlim_core::derive::Error;
use zlim_core::error::{ZlimError, ZlimResult};
use zlim_core::template::{IntoTemplate, SpecializeTemplate, Template, TemplateContext};
use zlim_reflect::TypePath;
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
#[type_path = "zlim_asset::handle::Handle"]
pub enum Handle<A: Asset> {
    /// A "uuid" reference to an [`Asset`] using a stable-across-runs / const identifier.
    ///
    /// Dropping this handle will not result in the asset being dropped.
    Uuid(Uuid, PhantomData<fn() -> A>),

    /// A "strong" reference to a live (or loading) [`Asset`].
    ///
    /// If a [`Handle`] is [`Handle::Strong`], the [`Asset`]
    /// will be kept alive until the [`Handle`] is dropped.
    Strong(Arc<StrongHandle>),
}

impl<A: Asset> Handle<A> {
    /// The id of the referenced asset.
    ///
    /// For a pending handle, this will return [`AssetId::default()`].
    #[inline]
    pub fn id(&self) -> AssetId<A> {
        match self {
            Self::Uuid(uuid, ..) => AssetId::Uuid { uuid: *uuid },
            Self::Strong(handle) => AssetId::Index {
                index: handle.index,
                marker: PhantomData,
            },
        }
    }

    /// The path this asset was loaded from, if it is a server-managed strong handle.
    #[inline]
    pub fn path(&self) -> Option<&AssetPath<'static>> {
        match self {
            Self::Uuid(..) => None,
            Self::Strong(handle) => handle.path.as_ref(),
        }
    }

    /// Returns `true` if this is a UUID handle.
    #[inline]
    pub const fn is_uuid(&self) -> bool {
        matches!(self, Self::Uuid(..))
    }

    /// Returns `true` if this is a strong handle.
    #[inline]
    pub const fn is_strong(&self) -> bool {
        matches!(self, Self::Strong(..))
    }

    /// Erases the asset type, keeping the type id so the handle can be typed back.
    #[inline]
    pub fn erased(&self) -> ErasedHandle {
        let type_id = TypeId::of::<A>();
        match self {
            Self::Uuid(uuid, ..) => ErasedHandle::Uuid {
                uuid: *uuid,
                type_id,
            },
            Self::Strong(handle) => ErasedHandle::Strong(Arc::clone(handle)),
        }
    }
}

impl<A: Asset> Clone for Handle<A> {
    fn clone(&self) -> Self {
        match self {
            Self::Uuid(uuid, ..) => Self::Uuid(*uuid, PhantomData),
            Self::Strong(handle) => Self::Strong(Arc::clone(handle)),
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
        let name = format!("Handle<{}>", A::type_name());
        let mut writer = f.debug_struct(&name);

        match self {
            Self::Uuid(uuid, ..) => {
                writer.field("uuid", uuid);
            }
            Self::Strong(handle) => {
                writer
                    .field("index", &handle.index)
                    .field("path", &handle.path);
            }
        }

        writer.finish()
    }
}

impl<A: Asset> Hash for Handle<A> {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state)
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
#[derive(Clone, TypePath)]
#[type_path = "zlim_asset::handle::ErasedHandle"]
pub enum ErasedHandle {
    /// A stable UUID reference.
    Uuid {
        /// The referenced UUID.
        uuid: Uuid,
        /// The concrete asset type.
        type_id: TypeId,
    },

    /// A reference-counted slot reference.
    Strong(Arc<StrongHandle>),
}

impl ErasedHandle {
    /// The default UUID handle for a given asset type.
    #[inline]
    pub const fn default_for_type(type_id: TypeId) -> Self {
        let uuid = ErasedAssetId::DEFAULT_UUID;
        Self::Uuid { uuid, type_id }
    }

    /// The concrete asset type this handle refers to.
    #[inline]
    pub fn type_id(&self) -> TypeId {
        match self {
            Self::Uuid { type_id, .. } => *type_id,
            Self::Strong(handle) => handle.type_id,
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
            Self::Uuid { .. } => None,
            Self::Strong(handle) => handle.path.as_ref(),
        }
    }

    /// Returns `true` if this is a UUID handle.
    #[inline]
    pub const fn is_uuid(&self) -> bool {
        matches!(self, Self::Uuid { .. })
    }

    /// Returns `true` if this is a strong handle.
    #[inline]
    pub const fn is_strong(&self) -> bool {
        matches!(self, Self::Strong(..))
    }

    /// Types this handle back **without** checking the asset type.
    #[inline]
    pub fn with_type_unchecked<A: Asset>(self) -> Handle<A> {
        match self {
            Self::Uuid { uuid, .. } => Handle::Uuid(uuid, PhantomData),
            Self::Strong(handle) => Handle::Strong(handle),
        }
    }

    /// Types this handle back, asserting in debug builds that the type matches.
    #[inline]
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn with_type_debug_checked<A: Asset>(self) -> Handle<A> {
        #[cold]
        #[inline(never)]
        #[cfg(debug_assertions)]
        #[cfg_attr(debug_assertions, track_caller)]
        fn mismatch(name: &str) -> ! {
            panic!("The target Handle<{name}>'s TypeId does not match this ErasedHandle")
        }

        #[cfg(debug_assertions)]
        if self.type_id() != TypeId::of::<A>() {
            mismatch(core::any::type_name::<A>());
        }

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
            Self::Uuid { uuid, type_id } => {
                writer.field("uuid", uuid).field("type", type_id);
            }
            Self::Strong(handle) => {
                writer
                    .field("index", &handle.index)
                    .field("path", &handle.path);
            }
        }

        writer.finish()
    }
}

impl Hash for ErasedHandle {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state)
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
        TypeId::of::<A>() == other.type_id()
            && match (self, other) {
                (Handle::Uuid(x, ..), ErasedHandle::Uuid { uuid: y, .. }) => *x == *y,
                (Handle::Strong(x), ErasedHandle::Strong(y)) => x.index == y.index,
                _ => false,
            }
    }
}

impl<A: Asset> PartialEq<Handle<A>> for ErasedHandle {
    #[inline]
    fn eq(&self, other: &Handle<A>) -> bool {
        PartialEq::eq(other, self)
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

use crate::ident::{TypedAssetIndex, TypedIndexError};

impl<A: Asset> TryFrom<&Handle<A>> for TypedAssetIndex {
    type Error = TypedIndexError;

    #[inline]
    fn try_from(handle: &Handle<A>) -> Result<Self, Self::Error> {
        match handle {
            Handle::Uuid(uuid, ..) => Err(TypedIndexError::uuid(*uuid)),
            Handle::Strong(handle) => Ok(Self::new(handle.index, handle.type_id)),
        }
    }
}

impl TryFrom<&ErasedHandle> for TypedAssetIndex {
    type Error = TypedIndexError;

    #[inline]
    fn try_from(handle: &ErasedHandle) -> Result<Self, Self::Error> {
        match handle {
            ErasedHandle::Uuid { uuid, .. } => Err(TypedIndexError::uuid(*uuid)),
            ErasedHandle::Strong(handle) => Ok(Self::new(handle.index, handle.type_id)),
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

/// A [`Template`] that produces a [`Handle`].
pub enum HandleTemplate<T: Asset> {
    /// Creates a [`Handle`] by calling `AssetServer::load` on the given [`AssetPath`].
    Path(AssetPath<'static>),
    /// Creates a [`Handle`] by cloning the given [`Handle`] value.
    Handle(Handle<T>),
}

impl<T: Asset> Default for HandleTemplate<T> {
    fn default() -> Self {
        Self::Handle(Handle::default())
    }
}

// Do not implement `Clone`: a `Clone + Unpin` type gets
// a `Template` implementation automatically (the blanket impl).

impl<T: Asset> From<Handle<T>> for HandleTemplate<T> {
    fn from(value: Handle<T>) -> Self {
        Self::Handle(value)
    }
}

impl<I: Into<AssetPath<'static>>, T: Asset> From<I> for HandleTemplate<T> {
    fn from(value: I) -> Self {
        Self::Path(value.into())
    }
}

impl<A: Asset> Template for HandleTemplate<A> {
    type Output = Handle<A>;

    fn build_template(&self, context: &mut TemplateContext) -> ZlimResult<Handle<A>> {
        use crate::server::AssetServer;
        let path = match self {
            Self::Path(path) => path,
            Self::Handle(handle) => return Ok(handle.clone()),
        };
        match context.get_resource::<AssetServer>() {
            None => Err(ZlimError::error("Missing AssetServer")),
            Some(server) => Ok(server.load::<A>(path)),
        }
    }

    fn clone_template(&self) -> Self {
        match self {
            Self::Path(path) => Self::Path(path.clone()),
            Self::Handle(handle) => Self::Handle(handle.clone()),
        }
    }
}

/// Pseudo specialization
impl<T: Asset> Unpin for Handle<T> where for<'a> [()]: SpecializeTemplate {}

/// Tag specialized IntoTemplate
impl<T: Asset> SpecializeTemplate for Handle<T> {}

impl<T: Asset> IntoTemplate for Handle<T> {
    type Template = HandleTemplate<T>;

    fn into_template(self) -> Self::Template {
        HandleTemplate::from(self)
    }
}

// -----------------------------------------------------------------------------
// HandleReference

/// A reference to an asset handle used for serialization and deserialization.
///
/// - [`HandleReference::Uuid`]: reference the asset by its UUID
/// - [`HandleReference::Path`]: reference the asset by its path
///
/// # Format:
///
/// - Uuid Handle: `urn:uuid:$uuid`.<br/>
///   For example: `urn:uuid:67e55044-10b1-426f-9247-bb680e5fe0c8`.
///
/// - Strong Handle with Path: `$path`<br/>
///   For example: `http://example.png`.
///
/// - Strong Handle without Path: `urn:uuid:$default_uuid`.<br/>
///   For example: `urn:uuid:ffffffff-ffff-ffff-0000-000000000000`.
///
/// Must be free of leading and trailing whitespace, or
/// deserialization may fail or yield an unexpected asset path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandleReference<'a> {
    Uuid(Uuid),
    Path(AssetPath<'a>),
}

impl<'a> HandleReference<'a> {
    /// Converts this into an "owned" value.
    pub fn into_owned(self) -> HandleReference<'static> {
        match self {
            Self::Uuid(uuid) => HandleReference::Uuid(uuid),
            Self::Path(path) => HandleReference::Path(path.into_owned()),
        }
    }

    /// Clones this into an "owned" value.
    pub fn clone_owned(&self) -> HandleReference<'a> {
        match self {
            Self::Uuid(uuid) => HandleReference::Uuid(*uuid),
            Self::Path(path) => HandleReference::Path(path.clone_owned()),
        }
    }

    /// Reborrows self with a smaller lifetime.
    pub fn reborrow(&self) -> HandleReference<'_> {
        match self {
            Self::Uuid(uuid) => HandleReference::Uuid(*uuid),
            Self::Path(path) => HandleReference::Path(path.reborrow()),
        }
    }
}

impl Display for HandleReference<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            HandleReference::Uuid(uuid) => Display::fmt(uuid.as_urn(), f),
            HandleReference::Path(path) => Display::fmt(path, f),
        }
    }
}

impl<'a> Serialize for HandleReference<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            HandleReference::Uuid(uuid) => {
                use uuid::fmt::Urn;
                let mut buffer = [0; Urn::LENGTH];
                uuid.as_urn()
                    .encode_lower(&mut buffer)
                    .serialize(serializer)
            }
            HandleReference::Path(path) => AssetPath::stringify(path).serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for HandleReference<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ReferenceVisitor;

        impl<'de> Visitor<'de> for ReferenceVisitor {
            type Value = HandleReference<'de>;

            fn expecting(&self, formatter: &mut Formatter) -> core::fmt::Result {
                formatter.write_str("a UUID string or an asset path")
            }

            fn visit_borrowed_str<E: serde::de::Error>(
                self,
                v: &'de str,
            ) -> Result<Self::Value, E> {
                HandleReference::parse(v).map_err(serde::de::Error::custom)
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                match HandleReference::parse(v) {
                    Ok(x) => Ok(x.into_owned()),
                    Err(e) => Err(serde::de::Error::custom(e)),
                }
            }
        }

        deserializer.deserialize_str(ReferenceVisitor)
    }
}

impl HandleReference<'_> {
    pub fn parse(s: &str) -> Result<HandleReference<'_>, String> {
        #[cold]
        #[inline(never)]
        fn cold_to_string<E: Display>(e: E) -> String {
            e.to_string()
        }

        if s.starts_with("urn:uuid") {
            use core::str::FromStr;
            match uuid::fmt::Urn::from_str(s) {
                Ok(x) => Ok(HandleReference::Uuid(x.into_uuid())),
                Err(e) => Err(cold_to_string(e)),
            }
        } else {
            match AssetPath::try_parse(s) {
                Ok(x) => Ok(HandleReference::Path(x)),
                Err(e) => Err(cold_to_string(e)),
            }
        }
    }

    fn stringify_with_prefix(&self, prefix: &str) -> String {
        match self {
            HandleReference::Uuid(uuid) => {
                use uuid::fmt::Urn;
                let mut buffer = [0; Urn::LENGTH];
                let s2 = uuid.as_urn().encode_lower(&mut buffer[5..]);
                let mut res = String::with_capacity(Urn::LENGTH + 3 + prefix.len());
                res.push('[');
                res.push_str(prefix);
                res.push_str("]|");
                res.push_str(s2);
                res
            }
            HandleReference::Path(path) => {
                let s2 = AssetPath::stringify(path);
                // TODO: This can reduce memory allocation once,
                // but the code will become more cumbersome.
                let mut res = String::with_capacity(s2.len() + 3 + prefix.len());
                res.push('[');
                res.push_str(prefix);
                res.push_str("]|");
                res.push_str(&s2);
                res
            }
        }
    }
}

// -----------------------------------------------------------------------------
// TypedHandleReference

/// A reference to an asset handle with type id used for serialization and deserialization.
///
/// The `asset` field refers to the underlying asset through a [`HandleReference`]:
/// - [`HandleReference::Uuid`]: reference the asset by its UUID
/// - [`HandleReference::Path`]: reference the asset by its path
///
/// # Format:
///
/// `[$type]|$asset`
///
/// - Uuid Handle: `[$type]|urn:uuid:$uuid`.<br/>
///   For example: `[Image]|urn:uuid:67e55044-10b1-426f-9247-bb680e5fe0c8`.
///
/// - Strong Handle with Path: `[$type]|$path`<br/>
///   For example: `[Image]|http://example.png`.
///
/// - Strong Handle without Path: `[$type]|urn:uuid:$default_uuid`.<br/>
///   For example: `[Image]|urn:uuid:ffffffff-ffff-ffff-0000-000000000000`.
///
/// Both `$type` and `$asset` must be free of leading and trailing whitespace,
/// or deserialization may fail or yield an unexpected asset path.
///
/// # TypeId & TypePath
///
/// The [`TypeId`] is not serialized directly. Instead, it is converted into a
/// [`TypePath`] and serialized in that form. During deserialization the reverse
/// lookup is performed via functions such as [`asset::get_type_path_by_type_id`],
/// which resolves the `type` field back into a [`TypeId`].
///
/// Type identifiers are registered by the `init_asset` function, so the lookup
/// only succeeds for asset types that have been initialized.
///
/// When serialized, the type name is mapped to `"ErasedHandle"` (i.e. the struct is
/// emitted under that name), so that a [`TypedHandleReference`] round-trips through the
/// same representation regardless of the concrete asset type.
///
/// [`asset::get_type_path_by_type_id`]: crate::asset::get_type_path_by_type_id
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedHandleReference<'a> {
    // serde: "type"
    pub type_id: TypeId,
    // serde: "asset"
    pub reference: HandleReference<'a>,
}

impl Display for TypedHandleReference<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let id: TypeId = self.type_id;
        use crate::asset::get_type_path_by_type_id;
        let path = get_type_path_by_type_id(id).unwrap_or("__unknown__");
        Display::fmt(&self.reference.stringify_with_prefix(path), f)
    }
}

impl<'a> TypedHandleReference<'a> {
    /// Converts this into an "owned" value.
    pub fn into_owned(self) -> TypedHandleReference<'static> {
        TypedHandleReference {
            type_id: self.type_id,
            reference: self.reference.into_owned(),
        }
    }

    /// Clones this into an "owned" value.
    pub fn clone_owned(&self) -> TypedHandleReference<'a> {
        TypedHandleReference {
            type_id: self.type_id,
            reference: self.reference.clone_owned(),
        }
    }
}

impl<'a> Serialize for TypedHandleReference<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use crate::asset::get_type_path_by_type_id;
        use serde::ser::Error;

        let id: TypeId = self.type_id;
        let Some(path) = get_type_path_by_type_id(id) else {
            return Err(Error::custom(format!("missing TypeDB for id `{id:?}`")));
        };
        self.reference
            .stringify_with_prefix(path)
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TypedHandleReference<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[cold]
        #[inline(never)]
        fn invalid_erase_handle(s: &str) -> String {
            format!("invalid ErasedHandle, expect: `[$type]|$asset`, actual: `{s}`")
        }

        struct TypeReferenceVisitor;

        impl<'de> Visitor<'de> for TypeReferenceVisitor {
            type Value = TypedHandleReference<'de>;

            fn expecting(&self, formatter: &mut Formatter) -> core::fmt::Result {
                formatter.write_str("handle reference with type: `[$type]|$asset`")
            }

            fn visit_borrowed_str<E: serde::de::Error>(
                self,
                full_path: &'de str,
            ) -> Result<Self::Value, E> {
                use crate::asset::get_type_id_by_type_path;

                let Some(s1) = full_path.strip_prefix('[') else {
                    let e = invalid_erase_handle(full_path);
                    return Err(serde::de::Error::custom(e));
                };

                let Some((ty, asset)) = s1.split_once("]|") else {
                    let e = invalid_erase_handle(full_path);
                    return Err(serde::de::Error::custom(e));
                };

                let Some(type_id) = get_type_id_by_type_path(ty) else {
                    ::core::hint::cold_path();
                    let e = format!("missing Type for path `{ty}`");
                    return Err(serde::de::Error::custom(e));
                };
                let reference = HandleReference::parse(asset).map_err(serde::de::Error::custom)?;

                Ok(TypedHandleReference { type_id, reference })
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                self.visit_borrowed_str(v)
                    .map(TypedHandleReference::into_owned)
            }
        }

        deserializer.deserialize_str(TypeReferenceVisitor)
    }
}

// -----------------------------------------------------------------------------
