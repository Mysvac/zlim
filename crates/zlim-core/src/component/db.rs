//! [`ComponentDB`] — static per-type metadata and the global lookup registries.

use core::alloc::Layout;
use core::any::TypeId;
use core::fmt::{Debug, Formatter};
use std::sync::{PoisonError, RwLock};

use zlim_reflect::TypeDB;
use zlim_utils::ext::{CachePadded, TypeMap};
use zlim_utils::hash::HashMap;

use super::{Component, ComponentHook, ComponentId, Required};
use crate::clone::ComponentCloner;
use crate::component::ReflectComponent;
use crate::utils::Dropper;

// -----------------------------------------------------------------------------
// Registries
// -----------------------------------------------------------------------------

/// Id-indexed global registry of every registered [`ComponentDB`].
pub(super) static ID_REGISTRY: CachePadded<RwLock<Vec<&'static ComponentDB>>> =
    CachePadded::new(RwLock::new(Vec::new()));

/// [`TypeId`]-indexed global registry of every registered [`ComponentDB`].
pub(super) static TYPE_REGISTRY: CachePadded<RwLock<TypeMap<&'static ComponentDB>>> =
    CachePadded::new(RwLock::new(TypeMap::new()));

/// Type-path-indexed global registry of reflected [`Component`].
pub(super) static PATH_REGISTRY: CachePadded<RwLock<HashMap<&'static str, &'static ComponentDB>>> =
    CachePadded::new(RwLock::new(HashMap::new()));

// -----------------------------------------------------------------------------
// ComponentDB
// -----------------------------------------------------------------------------

/// Static metadata for a single component type.
///
/// Created lazily by [`Component::REGISTER`] and stored in the global
/// `ID_REGISTRY`. Holds type identity, lifecycle hooks, memory layout,
/// clone/drop strategy, and serialization routines — all type-erased so
/// they can be stored homogeneously.
#[repr(C)] // The determined field order can optimize access speed.
pub struct ComponentDB {
    // --------------------------------
    // Ident
    /// Unique identifier assigned at registration time.
    pub id: ComponentId,
    /// Opaque [`TypeId`] for runtime type comparison.
    pub type_id: TypeId,

    // --------------------------------
    // Memory Layout
    /// Memory layout (size + alignment) of `Self`.
    pub layout: Layout,
    /// Optional custom dropper; `None` means standard drop.
    pub dropper: Option<Dropper>,
    /// Cloning strategy for this component.
    pub cloner: ComponentCloner,

    // --------------------------------
    // Required Components
    pub required: Option<Required>,

    // --------------------------------
    // Reflect
    /// Does the component need serialization.
    pub serialize: bool,
    /// Cached type database.
    pub type_db: Option<&'static TypeDB>,
    /// Reflect functions.
    pub reflect: Option<&'static ReflectComponent>,

    // --------------------------------
    // Hook
    /// Hook invoked on first add to an entity.
    pub on_add: Option<ComponentHook>,
    /// Hook invoked when a component is cloned.
    pub on_clone: Option<ComponentHook>,
    /// Hook invoked on every insertion (including updates).
    pub on_insert: Option<ComponentHook>,
    /// Hook invoked when the component is removed from its entity.
    pub on_remove: Option<ComponentHook>,
    /// Hook invoked when the component value is discarded
    /// (i.e. component replace, remove, or entity despawn).
    pub on_discard: Option<ComponentHook>,
    /// Hook invoked when the owning entity is despawned.
    pub on_despawn: Option<ComponentHook>,

    // --------------------------------
    // Names
    /// Fully-qualified type path (e.g. `"my_crate::components::Transform"`).
    pub type_path: &'static str,
    /// Short type name (e.g. `"Transform"`).
    pub type_name: &'static str,
    /// Module path of the type definition.
    pub module_path: &'static str,
}

impl Debug for ComponentDB {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_map()
            .entry(&"id", &self.id)
            .entry(&"type_id", &self.type_id)
            .entry(&"type_path", &self.type_path)
            .finish()
    }
}

impl ComponentDB {
    /// Returns the [`ComponentDB`] for `T`, registering it if this is
    /// the first access.
    ///
    /// Registration is lazy and idempotent: the first call registers `T`
    /// and every later call returns the cached entry.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// #[derive(Component, Clone)]
    /// struct Position {
    ///     x: f32,
    ///     y: f32,
    /// }
    ///
    /// let db = ComponentDB::of::<Position>();
    /// assert_eq!(db.type_name, "Position");
    /// assert_eq!(db.type_id, core::any::TypeId::of::<Position>());
    /// // Repeated lookups return the same static entry:
    /// assert!(core::ptr::eq(db, ComponentDB::of::<Position>()));
    /// ```
    #[inline(always)]
    pub fn of<T: Component>() -> &'static ComponentDB {
        <T as Component>::REGISTER()
    }

    /// Looks up a [`ComponentDB`] by its [`ComponentId`].
    ///
    /// # Panics
    ///
    /// Panics if `id` is out of bounds of the global registry. This is
    /// normally impossible unless the ID was manually constructed.
    pub fn get_by_id(id: ComponentId) -> &'static ComponentDB {
        let item = ID_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id.index())
            .copied();
        // Split to avoid the poison of panic.
        item.unwrap()
    }

    /// Looks up a [`ComponentDB`] by its [`TypeId`].
    ///
    /// Returns `None` if the type has not been registered yet.
    pub fn get_by_type(ty: TypeId) -> Option<&'static ComponentDB> {
        TYPE_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(ty)
            .copied()
    }

    /// Looks up a [`ComponentDB`] by its fully-qualified type path
    /// (e.g. `"my_crate::components::Transform"`).
    ///
    /// This can only find components that support reflection.
    ///
    /// Returns `None` if the type has not been registered or reflected.
    pub fn get_by_path(path: &str) -> Option<&'static ComponentDB> {
        PATH_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(path)
            .copied()
    }
}
