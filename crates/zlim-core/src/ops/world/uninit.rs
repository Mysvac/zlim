use crate::entity::{EntityId, Location};
use crate::ops::EntityMut;
use crate::table::TableId;
use crate::utils::DebugLocation;
use crate::utils::ForgetEntityOnPanic;
use crate::world::World;

impl World {
    /// Spawn a new entity with uninitialized component data.
    ///
    /// Although the spawned entity has a `ChildOf` relationship, it will not
    /// be automatically added to the parent entity's `Children` collection.
    ///
    /// You must manually add it after the entity's data has been fully initialized.
    ///
    /// # Safety
    /// - The spawned entity's component data is uninitialized.
    ///   Accessing component data before initialization is undefined behavior.
    ///
    /// - The input `TableId` must point to a intialized Table.
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
    pub(crate) unsafe fn spawn_uninit_with_caller(
        &mut self,
        table_id: TableId,
        caller: DebugLocation,
        child_of: Option<EntityId>,
    ) -> EntityMut<'_> {
        let entity = self.allocator.alloc_mut();

        if ::core::cfg!(debug_assertions) {
            self.entities.check_spawnable(entity).unwrap();
        }

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
