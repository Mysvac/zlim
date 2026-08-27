//! [`ResourceDB`] — static per-type metadata and the global lookup registries.

use core::alloc::Layout;
use core::any::TypeId;
use core::fmt::{Debug, Formatter};
use std::sync::{PoisonError, RwLock};

use zlim_utils::ext::TypeMap;
use zlim_utils::hash::HashMap;

use super::alias::{DeserializeFunc, GetFieldFunc, SerializeFunc, SetFieldFunc};
use super::id::ResourceId;
use super::resource::Resource;
use crate::utils::Dropper;

// -----------------------------------------------------------------------------
// Registries
// -----------------------------------------------------------------------------

/// Id-indexed global registry of every registered [`ResourceDB`].
pub(super) static ID_REGISTRY: RwLock<Vec<&'static ResourceDB>> = RwLock::new(Vec::new());

/// [`TypeId`]-indexed global registry of every registered [`ResourceDB`].
pub(super) static TYPE_REGISTRY: RwLock<TypeMap<&'static ResourceDB>> = RwLock::new(TypeMap::new());

/// Type-path-indexed global registry of every registered [`ResourceDB`].
pub(super) static PATH_REGISTRY: RwLock<HashMap<&'static str, &'static ResourceDB>> =
    RwLock::new(HashMap::new());

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
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Resource)]
/// struct Score(u32);
///
/// // `ResourceDB::of` registers the type on first use and returns its
/// // static metadata:
/// let db = ResourceDB::of::<Score>();
/// assert_eq!(db.type_name, "Score");
///
/// // The same metadata is reachable by id, type, or path.
/// assert!(core::ptr::eq(ResourceDB::get_by_id(db.id), db));
/// assert!(core::ptr::eq(ResourceDB::get_by_type(db.type_id).unwrap(), db));
/// assert!(core::ptr::eq(
///     ResourceDB::get_by_path(db.type_path).unwrap(),
///     db,
/// ));
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

    /// The full type path string (e.g., `"my_crate::MyResource"`).
    pub type_path: &'static str,
    /// The short type name string (e.g., `"MyResource"`).
    pub type_name: &'static str,
    /// The module path where the type is defined.
    pub module_path: &'static str,

    // --------------------------------
    // Editor accessor
    /// Field names readable via `get_field`.
    pub getter: &'static [&'static str],
    /// Field names writable via `set_field`.
    pub setter: &'static [&'static str],
    /// Type-erased function to read a field by name.
    pub get_field_func: GetFieldFunc,
    /// Type-erased function to write a field by name.
    pub set_field_func: SetFieldFunc,

    // --------------------------------
    // Memory Layout
    /// Memory layout of the resource type (size + alignment).
    pub layout: Layout,
    /// Optional dropper function for explicit cleanup.
    pub dropper: Option<Dropper>,

    // --------------------------------
    // Serialization
    /// Type-erased serialization function pointer, `None` when the resource
    /// does not support serialization.
    pub serialize: Option<SerializeFunc>,
    /// Type-erased deserialization function pointer, `None` when the
    /// resource does not support serialization.
    pub deserialize: Option<DeserializeFunc>,
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
    /// found, it calls [`Resource::register`] to create one.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_reflect::derive::TypePath;
    ///
    /// #[derive(TypePath, Resource)]
    /// struct Score(u32);
    ///
    /// let db = ResourceDB::of::<Score>();
    /// assert_eq!(db.type_name, "Score");
    /// ```
    ///
    /// [`Resource::register`]: crate::resource::Resource::register
    #[inline(always)]
    pub fn of<T: Resource>() -> &'static ResourceDB {
        if let Some(db) = ResourceDB::get_by_type(TypeId::of::<T>()) {
            return db;
        }

        <T as Resource>::register()
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
    /// (e.g., `"my_crate::MyResource"`). Returns `None` if no resource with
    /// the given path has been registered.
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
