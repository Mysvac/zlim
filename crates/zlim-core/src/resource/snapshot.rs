//! [`Resources`] — a local snapshot of the global resource registry.
#![expect(clippy::len_without_is_empty, reason = "useless")]

use core::any::TypeId;
use core::fmt::{Debug, Formatter};
use std::sync::PoisonError;

use zlim_utils::ext::TypeMap;
use zlim_utils::hash::HashMap;

use super::db::{ID_REGISTRY, PATH_REGISTRY, ResourceDB, TYPE_REGISTRY};
use super::id::ResourceId;
use super::resource::Resource;

// -----------------------------------------------------------------------------
// Resources
// -----------------------------------------------------------------------------

/// A local snapshot of the global resource registries for fast access
/// within a [`World`].
///
/// This snapshot is initialized from the static registries and can be
/// incrementally updated when new types are registered after construction
/// (the world does this automatically through its periodic update, see
/// [`World::update_basic`]).
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
/// let mut world = World::alloc();
/// let resources = world.resources();
///
/// // Look up the metadata for a resource type, registering it if needed:
/// let db = resources.get::<Score>();
/// assert_eq!(db.type_name, "Score");
/// ```
///
/// [`World`]: crate::world::World
/// [`World::update_basic`]: crate::world::World::update_basic
pub struct Resources {
    /// All registered resource databases, indexed by [`ResourceId`].
    dbs: Vec<&'static ResourceDB>,
    /// O(1) lookup by [`TypeId`].
    type_map: TypeMap<&'static ResourceDB>,
    /// O(1) lookup by type path string.
    path_map: HashMap<&'static str, &'static ResourceDB>,
}

impl Debug for Resources {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&self.dbs, f)
    }
}

impl Resources {
    pub(crate) fn new() -> Self {
        crate::cfg::debug! {
            let start = ::zlim_os::time::Instant::now();
        }

        let dbs: Vec<&'static ResourceDB> = ID_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();

        let hint = dbs.len() + (dbs.len() >> 1);
        let mut type_map = TypeMap::with_capacity(hint);
        let mut path_map = HashMap::with_capacity(hint);

        for &item in &dbs {
            type_map.insert(item.type_id, item);
            path_map.insert(item.type_path, item);
        }

        crate::cfg::debug! {
            zlim_log::debug!("`zlim_core::Resources` initialized: {:?}`", start.elapsed());
        }

        Self {
            dbs,
            type_map,
            path_map,
        }
    }
}

impl Resources {
    /// Returns the number of registered resource types in this snapshot.
    #[inline]
    pub fn len(&self) -> usize {
        self.dbs.len()
    }

    /// Returns the [`ResourceDB`] metadata for type `R`, falling back to
    /// the global registries if not yet in this snapshot.
    ///
    /// If `R` has never been registered anywhere, this triggers a new
    /// registration.
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
    /// let mut world = World::alloc();
    /// let db = world.resources().get::<Score>();
    /// assert_eq!(db.type_name, "Score");
    /// ```
    ///
    /// [`ResourceDB`]: crate::resource::ResourceDB
    pub fn get<R: Resource>(&self) -> &'static ResourceDB {
        if let Some(&r) = self.type_map.get(TypeId::of::<R>()) {
            return r;
        }
        ::core::hint::cold_path();
        <R as Resource>::register()
    }

    /// Looks up [`ResourceDB`] metadata by [`ResourceId`].
    ///
    /// Checks the local cache first; falls back to the global registry
    /// on a miss. Returns `None` if the id is out of bounds.
    ///
    /// [`ResourceDB`]: crate::resource::ResourceDB
    /// [`ResourceId`]: crate::resource::ResourceId
    pub fn get_by_id(&self, id: ResourceId) -> Option<&'static ResourceDB> {
        if let Some(info) = self.dbs.get(id.index()) {
            return Some(*info);
        }

        #[cold]
        #[inline(never)]
        fn slow_path(id: ResourceId) -> Option<&'static ResourceDB> {
            ID_REGISTRY
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .get(id.index())
                .copied()
        }

        slow_path(id)
    }

    /// Looks up [`ResourceDB`] metadata by type path string.
    ///
    /// Checks the local cache first; falls back to the global registry
    /// on a miss. Returns `None` if no resource with the given path has
    /// been registered.
    ///
    /// [`ResourceDB`]: crate::resource::ResourceDB
    pub fn get_by_path(&self, path: &str) -> Option<&'static ResourceDB> {
        if let Some(info) = self.path_map.get(path) {
            return Some(*info);
        }

        #[cold]
        #[inline(never)]
        fn slow_path(path: &str) -> Option<&'static ResourceDB> {
            PATH_REGISTRY
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .get(path)
                .copied()
        }

        slow_path(path)
    }

    /// Looks up [`ResourceDB`] metadata by [`TypeId`].
    ///
    /// Checks the local cache first; falls back to the global registry
    /// on a miss. Returns `None` if no resource with the given [`TypeId`] has
    /// been registered.
    ///
    /// [`ResourceDB`]: crate::resource::ResourceDB
    pub fn get_by_type(&self, ty: TypeId) -> Option<&'static ResourceDB> {
        if let Some(info) = self.type_map.get(ty) {
            return Some(*info);
        }

        #[cold]
        #[inline(never)]
        fn slow_path(ty: TypeId) -> Option<&'static ResourceDB> {
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

impl Resources {
    /// Synchronizes this snapshot with the global registries.
    ///
    /// Any resource types that were registered after this `Resources` was
    /// created are appended to the internal vectors and maps. This is a
    /// cheap no-op if no new registrations have occurred.
    pub(crate) fn update(&mut self) {
        let r = ID_REGISTRY.read().unwrap_or_else(PoisonError::into_inner);
        let data = r.as_slice();
        let new_len = data.len();
        let old_len = self.dbs.len();

        if old_len < new_len {
            self.dbs.extend_from_slice(&data[old_len..]);

            ::core::mem::drop(r);

            for &item in &self.dbs[old_len..] {
                self.type_map.insert(item.type_id, item);
                self.path_map.insert(item.type_path, item);
            }
        }
    }
}
