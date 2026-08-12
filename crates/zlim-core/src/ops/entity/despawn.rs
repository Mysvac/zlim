use zlim_utils::debug::DebugLocation;

use crate::entity::EntityError;
use crate::ops::EntityOwned;
use crate::ops::world::despawn_internal;

impl EntityOwned<'_> {
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn despawn(self) -> Result<(), EntityError> {
        let caller = DebugLocation::caller();
        self.despawn_with_caller(caller)
    }

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
