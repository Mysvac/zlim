use zlim_utils::debug::DebugLocation;

use crate::entity::EntityId;
use crate::ops::EntityOwned;

impl EntityOwned<'_> {
    /// Clone the current entity and return the spawned entity handle.
    ///
    /// If `recursive` is set to true, it will recursively clone sub entities.
    /// 
    /// Return `None` if `self` is unspawned.
    ///
    /// Due to the existence of component hooks, `self` may be despawned
    /// after this function, and the caller should check it.
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn clone(&mut self, recursive: bool) -> Option<EntityId> {
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
    ) -> Option<EntityId> {
        if self.is_despawned() {
            return None;
        }

        let mut cloner = unsafe { self.world.full_mut().entity_cloner() };

        let result = cloner.spawn_clone_with_caller(self.id, recursive, caller);

        self.relocate();

        Some(result)
    }
}
