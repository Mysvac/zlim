//! Entity cloning method implemented on `EntityOwned`.

use zlim_utils::debug::DebugLocation;

use crate::entity::{EntityError, EntityId};
use crate::ops::EntityOwned;

impl EntityOwned<'_> {
    /// Clone the current entity and return the spawned entity handle.
    ///
    /// If `recursive` is set to true, it will recursively clone sub entities.
    ///
    /// Returns `Err` if `self` is unspawned.
    ///
    /// Due to the existence of component hooks, `self` may be despawned
    /// after this function, and the caller should check it.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::derive::Component;
    ///
    /// #[derive(TypePath, Component, Clone, PartialEq, Debug)]
    /// struct Name(String);
    ///
    /// let mut world = World::alloc();
    /// let mut entity = world.spawn(Name("hero".into()), None);
    ///
    /// // `recursive = false` copies only this entity's own components.
    /// let clone_id = entity.clone(false).unwrap();
    /// drop(entity); // release the world borrow before re-acquiring a handle
    /// let clone = world.entity_owned(clone_id);
    /// assert_eq!(clone.get::<Name>(), Some(&Name("hero".into())));
    /// ```
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn clone(&mut self, recursive: bool) -> Result<EntityId, EntityError> {
        self.clone_with_caller(recursive, DebugLocation::caller())
    }

    /// Clone the current entity and return the spawned entity handle.
    ///
    /// Return `None` if self is unspawned.
    #[inline(never)]
    pub(crate) fn clone_with_caller(
        &mut self,
        recursive: bool,
        caller: DebugLocation,
    ) -> Result<EntityId, EntityError> {
        self.validate()?;

        let mut cloner = unsafe { self.world.full_mut().entity_cloner() };

        let result = cloner.spawn_clone_with_caller(self.id, recursive, caller);

        self.relocate();

        Ok(result)
    }
}
