use zlim_utils::debug::DebugLocation;

use crate::entity::{EntityId, Location};
use crate::ops::EntityMut;
use crate::table::TableId;
use crate::utils::ForgetEntityOnPanic;
use crate::world::World;

impl World {
    /// Spawn a new entity with uninitialized component data.
    ///
    /// Although the spawned entity has a `ChildOf` relationship, it will not
    /// be automatically added to the parent entity's `Children` collection.
    ///
    /// In other words, the input `child_of` is just a placeholder, You must
    /// manually complete it after the entity's data has been fully initialized.
    ///
    /// # Panic
    /// - Panics if `child_of` is `Some` but the target entity is not spawned.
    ///
    /// # Safety
    /// - The spawned entity's component data is uninitialized.
    ///   Accessing component data before initialization is undefined behavior.
    ///
    /// - The input `TableId` must point to a intialized Table.
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub unsafe fn spawn_uninit(
        &mut self,
        table_id: TableId,
        child_of: Option<EntityId>,
    ) -> EntityMut<'_> {
        let caller = DebugLocation::caller();
        unsafe { self.spawn_uninit_with_caller(table_id, caller, child_of) }
    }

    /// # Safety
    /// - The spawned entity's component data is uninitialized.
    ///   Accessing component data before initialization is undefined behavior.
    ///
    /// - The input `TableId` must point to a intialized Table.
    #[inline(never)]
    pub(crate) unsafe fn spawn_uninit_with_caller(
        &mut self,
        table_id: TableId,
        caller: DebugLocation,
        child_of: Option<EntityId>,
    ) -> EntityMut<'_> {
        let entity = self.allocator.alloc_mut();

        #[cfg(debug_assertions)]
        if let Err(e) = self.entities.check_spawnable(entity) {
            ::core::hint::cold_path();
            panic!("entity {entity} cannot spawned: {e}.\n\t{caller}")
        }
        // No need to check `child_of`, it's just a placeholder.

        let world = self.cell();

        let guard = ForgetEntityOnPanic {
            entity,
            world,
            caller,
        };

        let world = unsafe { world.full_mut() };

        let this_run = world.this_run_fast();
        let last_run = world.last_run();

        let table = unsafe { world.tables.get_unchecked_mut(table_id) };

        let table_row = unsafe { table.alloc_row(entity) };

        let location = Location {
            table_id,
            table_row,
        };

        world
            .entities
            .insert_uninit(entity, child_of, location)
            .unwrap();

        ::core::mem::forget(guard);

        EntityMut {
            id: entity,
            table,
            location,
            last_run,
            this_run,
        }
    }
}
