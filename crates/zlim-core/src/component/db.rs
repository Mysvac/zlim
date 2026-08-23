//! [`ComponentDB`] — static per-type metadata and the global lookup registries.

use core::alloc::Layout;
use core::any::TypeId;
use core::fmt::{Debug, Formatter};
use std::sync::{PoisonError, RwLock};

use zlim_utils::ext::TypeMap;
use zlim_utils::hash::HashMap;

use super::alias::*;
use super::{Component, ComponentHook, ComponentId, Required};
use crate::clone::ComponentCloner;
use crate::utils::Dropper;

// -----------------------------------------------------------------------------
// Registries
// -----------------------------------------------------------------------------

/// Id-indexed global registry of every registered [`ComponentDB`].
pub(super) static ID_REGISTRY: RwLock<Vec<&'static ComponentDB>> = RwLock::new(Vec::new());

/// [`TypeId`]-indexed global registry of every registered [`ComponentDB`].
pub(super) static TYPE_REGISTRY: RwLock<TypeMap<&'static ComponentDB>> =
    RwLock::new(TypeMap::new());

/// Type-path-indexed global registry of every registered [`ComponentDB`].
pub(super) static PATH_REGISTRY: RwLock<HashMap<&'static str, &'static ComponentDB>> =
    RwLock::new(HashMap::new());

// -----------------------------------------------------------------------------
// ComponentDB
// -----------------------------------------------------------------------------

/// Static metadata for a single component type.
///
/// Created lazily by [`Component::register`] and stored in the global
/// `ID_REGISTRY`. Holds type identity, lifecycle hooks, field introspection
/// function pointers, memory layout, clone/drop strategy, and serialization
/// routines — all type-erased so they can be stored homogeneously.
pub struct ComponentDB {
    // --------------------------------
    // Ident
    /// Unique identifier assigned at registration time.
    pub id: ComponentId,
    /// Opaque [`TypeId`] for runtime type comparison.
    pub type_id: TypeId,

    /// Fully-qualified type path (e.g. `"my_crate::components::Transform"`).
    pub type_path: &'static str,
    /// Short type name (e.g. `"Transform"`).
    pub type_name: &'static str,
    /// Module path of the type definition.
    pub module_path: &'static str,

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
    /// Hook invoked when the component value is discarded (i.e. component
    /// replace, remove, or entity despawn).
    pub on_discard: Option<ComponentHook>,
    /// Hook invoked when the owning entity is despawned.
    pub on_despawn: Option<ComponentHook>,

    // --------------------------------
    // Required Components
    pub required: Option<Required>,

    // --------------------------------
    // Editor accessor
    /// Names of fields readable via `get_field`.
    pub getter: &'static [&'static str],
    /// Names of fields writable via `set_field`.
    pub setter: &'static [&'static str],
    /// Type-erased accessor for reflected field reads.
    pub get_field_func: GetFieldFunc,
    /// Type-erased accessor for reflected field writes.
    pub set_field_func: SetFieldFunc,

    // --------------------------------
    // Memory Layout
    /// Memory layout (size + alignment) of `Self`.
    pub layout: Layout,
    /// Cloning strategy for this component.
    pub cloner: ComponentCloner,
    /// Optional custom dropper; `None` means standard drop.
    pub dropper: Option<Dropper>,
    /// Type-erased entity-remapping function.
    pub map_entities: MapEntitiesFunc,

    // --------------------------------
    // Serialization
    /// Type-erased serialization function pointer, `None` when the component
    /// does not support serialization.
    pub serialize: Option<SerializeFunc>,
    /// Type-erased deserialization function pointer, `None` when the
    /// component does not support serialization.
    pub deserialize: Option<DeserializeFunc>,
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
    /// use zlim_reflect::derive::TypePath;
    ///
    /// #[derive(TypePath, Component, Clone)]
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
        if let Some(db) = ComponentDB::get_by_type(TypeId::of::<T>()) {
            return db;
        }

        <T as Component>::register()
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

    /// Looks up a [`ComponentDB`] by its fully-qualified type path (e.g.
    /// `"my_crate::components::Transform"`).
    ///
    /// Returns `None` if the type has not been registered yet.
    pub fn get_by_path(path: &str) -> Option<&'static ComponentDB> {
        PATH_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(path)
            .copied()
    }
}
