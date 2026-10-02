//! [`ResourceDB`] — static per-type metadata and the global lookup registries.

use core::alloc::Layout;
use core::any::TypeId;
use core::fmt::{Debug, Formatter};
use std::sync::{PoisonError, RwLock};

use zlim_reflect::TypeDB;
use zlim_utils::ext::{CachePadded, TypeMap};
use zlim_utils::hash::HashMap;

use super::id::ResourceId;
use super::reflect::ReflectResource;
use super::resource::Resource;
use crate::utils::Dropper;

// -----------------------------------------------------------------------------
// Registries
// -----------------------------------------------------------------------------

/// Id-indexed global registry of every registered [`ResourceDB`].
pub(super) static ID_REGISTRY: CachePadded<RwLock<Vec<&'static ResourceDB>>> =
    CachePadded::new(RwLock::new(Vec::new()));

/// [`TypeId`]-indexed global registry of every registered [`ResourceDB`].
pub(super) static TYPE_REGISTRY: CachePadded<RwLock<TypeMap<&'static ResourceDB>>> =
    CachePadded::new(RwLock::new(TypeMap::new()));

/// Type-path-indexed global registry of reflected [`Resource`].
pub(super) static PATH_REGISTRY: CachePadded<RwLock<HashMap<&'static str, &'static ResourceDB>>> =
    CachePadded::new(RwLock::new(HashMap::new()));

// -----------------------------------------------------------------------------
// ResourceDB
// -----------------------------------------------------------------------------

/// Static per-type metadata for a registered [`Resource`].
///
/// Each resource type has exactly one `ResourceDB` instance, allocated as a
/// `&'static` reference during registration. It holds the type's identity
/// (id, type path), reflection metadata (field names and accessors), and
/// memory layout information needed for allocation and destruction.
///
/// `ResourceDB` instances are stored in the process-global registries for
/// O(1) lookup by id, type, or path.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(Resource)]
/// struct Score(u32);
///
/// // `ResourceDB::of` registers the type on first use and returns its
/// // static metadata:
/// let db = ResourceDB::of::<Score>();
/// assert_eq!(db.type_name, "Score");
///
/// // The same metadata is reachable by id or by type.
/// assert!(core::ptr::eq(ResourceDB::get_by_id(db.id), db));
/// assert!(core::ptr::eq(ResourceDB::get_by_type(db.type_id).unwrap(), db));
/// ```
///
/// [`Resource`]: crate::resource::Resource
#[repr(C)] // The determined field order can optimize access speed.
pub struct ResourceDB {
    // --------------------------------
    // Ident
    /// Unique numeric identifier for this resource type.
    pub id: ResourceId,
    /// The [`TypeId`] of the resource type.
    pub type_id: TypeId,

    // --------------------------------
    // Memory Layout
    /// Memory layout of the resource type (size + alignment).
    pub layout: Layout,
    /// Optional dropper function for explicit cleanup.
    pub dropper: Option<Dropper>,

    // --------------------------------
    // Reflect
    /// Does the resource need serialization.
    ///
    /// Unused, always false.
    pub serialize: bool,
    /// Cached type database.
    pub type_db: Option<&'static TypeDB>,
    /// Reflect functions.
    pub reflect: Option<&'static ReflectResource>,

    // --------------------------------
    // Ident
    /// The full type path string (e.g., `"my_crate::MyResource"`).
    ///
    /// Only accurate when the resource supports reflection; otherwise it is
    /// derived from [`core::any::type_name`] and is not a stable identifier.
    pub type_path: &'static str,
    /// The short type name string (e.g., `"MyResource"`).
    pub type_name: &'static str,
    /// The module path where the type is defined.
    pub module_path: &'static str,
}

impl Debug for ResourceDB {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_map()
            .entry(&"id", &self.id)
            .entry(&"type_id", &self.type_id)
            .entry(&"type_path", &self.type_path)
            .finish()
    }
}

impl ResourceDB {
    /// Returns the [`ResourceDB`] metadata for type `T`, registering it if
    /// necessary.
    ///
    /// This is the primary entry point for obtaining resource metadata. It
    /// first checks the type registry for an existing entry; if none is
    /// found, it calls [`Resource::REGISTER`] to create one.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// #[derive(Resource)]
    /// struct Score(u32);
    ///
    /// let db = ResourceDB::of::<Score>();
    /// assert_eq!(db.type_name, "Score");
    /// ```
    ///
    /// [`Resource::REGISTER`]: crate::resource::Resource::REGISTER
    #[inline(always)]
    pub fn of<T: Resource>() -> &'static ResourceDB {
        <T as Resource>::REGISTER()
    }

    /// Looks up a [`ResourceDB`] by its [`ResourceId`].
    ///
    /// # Panics
    ///
    /// Panics if `id` is out of bounds — that is, if it does not correspond
    /// to any registered resource type. This is normally impossible unless
    /// the id was created manually.
    ///
    /// [`ResourceId`]: crate::resource::ResourceId
    pub fn get_by_id(id: ResourceId) -> &'static ResourceDB {
        let item = ID_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id.index())
            .copied();
        // Split to avoid the poison of panic.
        item.unwrap()
    }

    /// Looks up [`ResourceDB`] metadata by [`TypeId`].
    ///
    /// Returns `None` if no resource of the given type has been registered.
    pub fn get_by_type(id: TypeId) -> Option<&'static ResourceDB> {
        TYPE_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .copied()
    }

    /// Looks up [`ResourceDB`] metadata by type path string.
    ///
    /// The path is the full type path as returned by [`TypePath::type_path`]
    /// (e.g., `"my_crate::MyResource"`).
    ///
    /// This can only find resources that support reflection.
    ///
    /// Returns `None` if no resource with the given path has been registered
    /// or reflected.
    ///
    /// [`TypePath::type_path`]: zlim_reflect::TypePath::type_path
    pub fn get_by_path(path: &str) -> Option<&'static ResourceDB> {
        PATH_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(path)
            .copied()
    }
}
