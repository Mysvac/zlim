//! Unsafe entity-forget method implemented on `World`.

use zlim_log as log;
use zlim_utils::debug::DebugLocation;

use crate::entity::EntityId;
use crate::world::World;

impl World {
    /// Forget an entity without dropping its data and without calling components' hooks.
    ///
    /// This function has a high overhead and requires iterating all tables to stably
    /// delete component data without relying on the Entity Location.
    ///
    /// Typically used for cleaning up entities that caused a panic.
    ///
    /// # Safety
    /// This operation is **extremely unsafe** and should be used with extreme caution.
    #[cold]
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub unsafe fn forget(&mut self, entity: EntityId) {
        unsafe {
            self.forget_with_caller(entity, DebugLocation::caller());
        }
    }

    /// # Safety
    /// This operation is **extremely unsafe** and should be used with extreme caution.
    #[cold]
    #[inline(never)]
    pub(crate) unsafe fn forget_with_caller(&mut self, entity: EntityId, caller: DebugLocation) {
        let world_id = self.id();

        log::warn!(
            "Entity<{entity}>(in World<{world_id}>) was forgotten, may leaking memory: {caller}."
        );

        let _ = self.entities.remove_one(entity);

        for table in self.tables.iter_mut() {
            if let Some(row) = table.get_table_row(entity) {
                let moved = unsafe { table.dealloc_row::<false>(row) };
                let _ = self.entities.update_row(moved);
            }
        }
    }
}
