//! Empty-entity spawning methods implemented on `World`.

use zlim_utils::debug::DebugLocation;

use crate::entity::{EntityId, Location};
use crate::ops::EntityOwned;
use crate::table::TableId;
use crate::utils::ForgetEntityOnPanic;
use crate::world::World;

impl World {
    /// Spawns a new empty entity and returns an owned handle to it.
    ///
    /// This function is faster than `spawn((), parent)`.
    ///
    /// # Panics
    /// - Panics if `parent` is `Some` but the target entity is not spawned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    ///
    /// // The entity starts with no components at all.
    /// let entity = world.spawn_empty(None);
    /// assert!(entity.components().is_some_and(|c| c.is_empty()));
    /// ```
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn spawn_empty(&mut self, parent: Option<EntityId>) -> EntityOwned<'_> {
        let caller = DebugLocation::caller();
        self.spawn_empty_with_caller(parent, caller)
    }

    /// Spawns a new empty entity at given `id` and returns an owned handle to it.
    ///
    /// This function is faster than `spawn_at((), id, parent)`.
    ///
    /// # Panics
    /// - Panics if given `id` cannot spawn (e.g. already spawned).
    /// - Panics if `parent` is `Some` but the target entity is not spawned.
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn spawn_empty_at(&mut self, id: EntityId, parent: Option<EntityId>) -> EntityOwned<'_> {
        let caller = DebugLocation::caller();
        self.spawn_empty_at_with_caller(id, parent, caller)
    }

    #[inline(never)]
    pub(crate) fn spawn_empty_with_caller(
        &mut self,
        parent: Option<EntityId>,
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

        if let Err(e) = world.entities.insert_one(entity, parent, location) {
            ::core::hint::cold_path();
            panic!("parent `{parent:?}` is invalid: {e}.\n\t{caller}");
        }

        ::core::mem::forget(guard);

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
        parent: Option<EntityId>,
        caller: DebugLocation,
    ) -> EntityOwned<'_> {
        if let Err(e) = self.entities.check_spawnable(id) {
            ::core::hint::cold_path();
            panic!("entity {id} cannot spawned: {e}.\n\t{caller}")
        }
        if let Some(c) = parent
            && !self.entities.contains(c)
        {
            ::core::hint::cold_path();
            panic!("parent `{c}` is unspawned.\n\t{caller}");
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

        world.entities.insert_one(id, parent, location).unwrap();

        ::core::mem::forget(guard);

        let storage = Some((table, location));

        EntityOwned {
            id,
            world: cell,
            storage,
        }
    }
}
