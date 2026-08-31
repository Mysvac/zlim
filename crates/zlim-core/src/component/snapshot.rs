//! [`Components`] — a local snapshot of the global component registry.
#![expect(clippy::len_without_is_empty, reason = "useless")]

use core::any::TypeId;
use core::fmt::{Debug, Formatter};
use std::sync::PoisonError;

use zlim_utils::ext::TypeMap;
use zlim_utils::hash::HashMap;

use super::db::{ComponentDB, ID_REGISTRY, PATH_REGISTRY, TYPE_REGISTRY};
use super::{Component, ComponentId};

// -----------------------------------------------------------------------------
// Components
// -----------------------------------------------------------------------------

/// A local snapshot of the global component registry.
///
/// Created from the global `ID_REGISTRY`, `TYPE_REGISTRY`, and
/// `PATH_REGISTRY` at construction time. Provides fast local lookups
/// by id, type path, or [`TypeId`] without needing to acquire the global
/// locks.
pub struct Components {
    /// Ordered list of all currently-known component descriptors.
    dbs: Vec<&'static ComponentDB>,
    /// Indexed by [`TypeId`] for O(1) lookup.
    type_map: TypeMap<&'static ComponentDB>,
    /// Indexed by type path string for O(1) lookup.
    path_map: HashMap<&'static str, &'static ComponentDB>,
}

impl Debug for Components {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&self.dbs, f)
    }
}

impl Components {
    pub(crate) const fn new() -> Self {
        Self {
            dbs: Vec::new(),
            type_map: TypeMap::new(),
            path_map: HashMap::new(),
        }
    }
}

impl Components {
    /// Returns the number of registered components.
    #[inline]
    pub fn len(&self) -> usize {
        self.dbs.len()
    }

    /// Returns the [`ComponentDB`] for component type `C`.
    ///
    /// Checks the local `type_map` first (fast path). If the type is not
    /// yet in this snapshot, falls back to lazy registration via
    /// [`Component::register`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// #[derive(TypePath, Component, Clone)]
    /// struct Position;
    ///
    /// let world = World::alloc();
    ///
    /// // Register the component, then look it up through the world's
    /// // per-world snapshot:
    /// let _ = ComponentDB::of::<Position>();
    /// let components = world.components();
    ///
    /// assert_eq!(components.get::<Position>().type_name, "Position");
    /// ```
    pub fn get<C: Component>(&self) -> &'static ComponentDB {
        if let Some(&r) = self.type_map.get(TypeId::of::<C>()) {
            return r;
        }
        ::core::hint::cold_path();
        <C as Component>::register()
    }

    /// Looks up a [`ComponentDB`] by its [`ComponentId`].
    ///
    /// First checks the local `dbs` slice via index (fast path). On a miss
    /// falls back to the global `ID_REGISTRY`.
    #[inline]
    pub fn get_by_id(&self, id: ComponentId) -> Option<&'static ComponentDB> {
        if let Some(info) = self.dbs.get(id.index()) {
            return Some(*info);
        }

        #[cold]
        #[inline(never)]
        fn slow_path(id: ComponentId) -> Option<&'static ComponentDB> {
            ID_REGISTRY
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .get(id.index())
                .copied()
        }

        slow_path(id)
    }

    /// Looks up a [`ComponentDB`] by its fully-qualified type path.
    ///
    /// First checks the local `path_map` (fast path). On a miss falls
    /// back to the global `PATH_REGISTRY`.
    #[inline]
    pub fn get_by_path(&self, path: &str) -> Option<&'static ComponentDB> {
        if let Some(info) = self.path_map.get(path) {
            return Some(*info);
        }

        #[cold]
        #[inline(never)]
        fn slow_path(path: &str) -> Option<&'static ComponentDB> {
            PATH_REGISTRY
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .get(path)
                .copied()
        }

        slow_path(path)
    }

    /// Looks up a [`ComponentDB`] by its [`TypeId`].
    ///
    /// First checks the local `type_map` (fast path). On a miss falls
    /// back to the global `TYPE_REGISTRY`.
    #[inline]
    pub fn get_by_type(&self, ty: TypeId) -> Option<&'static ComponentDB> {
        if let Some(info) = self.type_map.get(ty) {
            return Some(*info);
        }

        #[cold]
        #[inline(never)]
        fn slow_path(ty: TypeId) -> Option<&'static ComponentDB> {
            TYPE_REGISTRY
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .get(ty)
                .copied()
        }

        slow_path(ty)
    }
}

// -----------------------------------------------------------------------------
// Update
// -----------------------------------------------------------------------------

impl Components {
    /// Refreshes this snapshot from the global `ID_REGISTRY`.
    ///
    /// Picks up any component types that were registered after this
    /// `Components` instance was created. New entries are appended to
    /// `dbs`, `type_map`, and `path_map`.
    #[inline]
    pub(crate) fn update(&mut self) {
        let r = ID_REGISTRY.read().unwrap_or_else(PoisonError::into_inner);
        let data = r.as_slice();
        let new_len = data.len();
        let old_len = self.dbs.len();

        if old_len < new_len {
            ::core::hint::cold_path();
            self.dbs.extend_from_slice(&data[old_len..]);

            ::core::mem::drop(r);

            for &item in &self.dbs[old_len..] {
                self.type_map.insert(item.type_id, item);
                self.path_map.insert(item.type_path, item);
            }
        }
    }
}

// -----------------------------------------------------------------------------
