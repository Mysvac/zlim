use crate::entity::EntityId;
use crate::utils::DebugLocation;
use crate::world::World;

impl World {
    /// Forget an entity without dropping its data and without calling components' hooks.
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
                self.entities.update_row(moved).unwrap();
            }
        }
    }
}
