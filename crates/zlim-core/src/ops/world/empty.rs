use zlim_utils::debug::DebugLocation;

use crate::entity::{EntityId, Location};
use crate::ops::EntityOwned;
use crate::table::TableId;
use crate::utils::ForgetEntityOnPanic;
use crate::world::World;

impl World {
    /// Spawns a new empty entity and returns an owned handle to it.
    ///
    /// This function is faster than `spawn((), child_of)`.
    ///
    /// # Panic
    /// - Panics if `child_of` is `Some` but the target entity is not spawned.
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn spawn_empty(&mut self, child_of: Option<EntityId>) -> EntityOwned<'_> {
        let caller = DebugLocation::caller();
        self.spawn_empty_with_caller(child_of, caller)
    }

    /// Spawns a new empty entity at given `id` and returns an owned handle to it.
    ///
    /// This function is faster than `spawn_at((), id, child_of)`.
    ///
    /// # Panic
    /// - Panics if given `id` cannot spawn (e.g. already spawned).
    /// - Panics if `child_of` is `Some` but the target entity is not spawned.
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn spawn_empty_at(&mut self, id: EntityId, child_of: Option<EntityId>) -> EntityOwned<'_> {
        let caller = DebugLocation::caller();
        self.spawn_empty_at_with_caller(id, child_of, caller)
    }

    #[inline(never)]
    pub(crate) fn spawn_empty_with_caller(
        &mut self,
        child_of: Option<EntityId>,
        caller: DebugLocation,
    ) -> EntityOwned<'_> {
        let entity = self.allocator.alloc_mut();

        #[cfg(debug_assertions)]
        if let Err(e) = self.entities.check_spawnable(entity) {
            ::core::hint::cold_path();
            panic!("entity {entity} cannot spawned: {e}.\n\t{caller}")
        }

        let cell = self.cell();

        let guard = ForgetEntityOnPanic {
            entity,
            world: cell,
            caller,
        };

        let world = unsafe { cell.full_mut() };

        let table = unsafe { world.tables.get_unchecked_mut(TableId::EMPTY) };

        let table_row = unsafe { table.alloc_row(entity) };

        let location = Location {
            table_id: TableId::EMPTY,
            table_row,
        };

        if let Err(e) = world.entities.insert_one(entity, child_of, location) {
            ::core::hint::cold_path();
            panic!("child_of `{child_of:?}` is invalid: {e}.\n\t{caller}");
        }

        ::core::mem::drop(guard);

        let storage = Some((table, location));

        EntityOwned {
            world: cell,
            id: entity,
            storage,
        }
    }

    #[inline(never)]
    pub(crate) fn spawn_empty_at_with_caller(
        &mut self,
        id: EntityId,
        child_of: Option<EntityId>,
        caller: DebugLocation,
    ) -> EntityOwned<'_> {
        if let Err(e) = self.entities.check_spawnable(id) {
            ::core::hint::cold_path();
            panic!("entity {id} cannot spawned: {e}.\n\t{caller}")
        }
        if let Some(c) = child_of
            && !self.entities.contains(c)
        {
            ::core::hint::cold_path();
            panic!("child_of `{c}` is unspawned.\n\t{caller}");
        }

        let cell = self.cell();

        let guard = ForgetEntityOnPanic {
            entity: id,
            world: cell,
            caller,
        };

        let world = unsafe { cell.full_mut() };

        let table = unsafe { world.tables.get_unchecked_mut(TableId::EMPTY) };

        let table_row = unsafe { table.alloc_row(id) };

        let location = Location {
            table_id: TableId::EMPTY,
            table_row,
        };

        world.entities.insert_one(id, child_of, location).unwrap();

        ::core::mem::drop(guard);

        let storage = Some((table, location));

        EntityOwned {
            id,
            world: cell,
            storage,
        }
    }
}
