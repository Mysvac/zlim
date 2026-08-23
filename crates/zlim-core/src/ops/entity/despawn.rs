//! Entity despawn methods implemented on `EntityOwned`.

use zlim_utils::debug::DebugLocation;

use crate::entity::EntityError;
use crate::ops::EntityOwned;
use crate::ops::world::despawn_internal;

impl EntityOwned<'_> {
    /// Despawns this entity, recursively despawning all of its descendants.
    ///
    /// # Errors
    ///
    /// Returns [`EntityError`] if the entity is not spawned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::derive::Component;
    ///
    /// #[derive(TypePath, Component, Clone)]
    /// struct Marker;
    ///
    /// let mut world = World::alloc();
    /// let entity = world.spawn(Marker, None);
    /// let id = entity.id();
    ///
    /// // Despawn the entity (and, recursively, any descendants).
    /// entity.despawn().unwrap();
    ///
    /// // The ID is no longer backed by storage.
    /// assert!(world.get_entity_owned(id).is_err());
    /// ```
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn despawn(self) -> Result<(), EntityError> {
        let caller = DebugLocation::caller();
        self.despawn_with_caller(caller)
    }

    /// Despawns this entity, recursively despawning all of its descendants.
    ///
    /// Returns `false` (and does nothing) if the entity is not spawned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let entity = world.spawn((), None);
    /// let id = entity.id();
    ///
    /// assert!(entity.try_despawn());
    /// assert!(world.get_entity_owned(id).is_err());
    /// ```
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn try_despawn(self) -> bool {
        let caller = DebugLocation::caller();
        self.try_despawn_with_caller(caller)
    }

    #[inline]
    pub(crate) fn despawn_with_caller(self, caller: DebugLocation) -> Result<(), EntityError> {
        if self.is_despawned() {
            return Err(EntityError::NotSpawned(self.id));
        }
        let id = self.id;
        let world = self.into_world();
        despawn_internal(world, id, caller);
        Ok(())
    }

    #[inline]
    pub(crate) fn try_despawn_with_caller(self, caller: DebugLocation) -> bool {
        if self.is_despawned() {
            return false;
        }
        let id = self.id;
        let world = self.into_world();
        despawn_internal(world, id, caller);
        true
    }
}
