//! Uninitialized-entity spawning method implemented on `World`.

use zlim_utils::debug::DebugLocation;

use crate::entity::{EntityId, Location};
use crate::ops::EntityMut;
use crate::table::TableId;
use crate::utils::ForgetEntityOnPanic;
use crate::world::World;

impl World {
    /// Spawn a new entity with uninitialized component data.
    ///
    /// Although a `parent` may be given, the parent/child links are only
    /// partially wired: the parent is recorded on the new entity, but the
    /// new entity is **not** added to the parent's children.  In other
    /// words, the input `parent` is just a placeholder — you must complete
    /// the hierarchy manually (e.g. with [`EntityOwned::modify_parent`])
    /// once the entity's data has been fully initialized.
    ///
    /// # Panics
    /// - Panics if `parent` is `Some` but the target entity is not spawned.
    ///
    /// # Safety
    /// - The spawned entity's component data is uninitialized.
    ///   Accessing component data before initialization is undefined behavior.
    ///
    /// - The input `TableId` must point to an initialized Table.
    ///
    /// [`EntityOwned::modify_parent`]: crate::ops::EntityOwned::modify_parent
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub unsafe fn spawn_uninit(
        &mut self,
        table_id: TableId,
        parent: Option<EntityId>,
    ) -> EntityMut<'_> {
        let caller = DebugLocation::caller();
        unsafe { self.spawn_uninit_with_caller(table_id, caller, parent) }
    }

    /// # Safety
    /// - The spawned entity's component data is uninitialized.
    ///   Accessing component data before initialization is undefined behavior.
    ///
    /// - The input `TableId` must point to an initialized Table.
    #[inline(never)]
    pub(crate) unsafe fn spawn_uninit_with_caller(
        &mut self,
        table_id: TableId,
        caller: DebugLocation,
        parent: Option<EntityId>,
    ) -> EntityMut<'_> {
        let entity = self.allocator.alloc_mut();

        #[cfg(debug_assertions)]
        if let Err(e) = self.entities.check_spawnable(entity) {
            ::core::hint::cold_path();
            panic!("entity {entity} cannot spawned: {e}.\n\t{caller}")
        }
        // No need to check `parent`, it's just a placeholder.

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
            .insert_uninit(entity, parent, location)
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
