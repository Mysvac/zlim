//! Component-clearing method implemented on `EntityOwned`.

use zlim_utils::debug::DebugLocation;

use crate::component::HookContext;
use crate::entity::{EntityError, Location};
use crate::ops::EntityOwned;
use crate::table::TableId;
use crate::utils::{DebugCheckedUnwrap, ForgetEntityOnPanic};

impl EntityOwned<'_> {
    /// Removes all components associated with the entity.
    ///
    /// The entity is moved to the empty archetype; its sub entities are
    /// left untouched.
    ///
    /// # Errors
    ///
    /// Returns [`EntityError`] if the entity is not spawned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// #[derive(TypePath, Component, Clone, PartialEq, Debug)]
    /// struct Foo;
    ///
    /// #[derive(TypePath, Component, Clone, PartialEq, Debug)]
    /// struct Bar(u32);
    ///
    /// let mut world = World::alloc();
    ///
    /// let mut entity = world.spawn((Foo, Bar(7)), None);
    /// assert!(entity.contains::<Foo>());
    /// assert!(entity.contains::<Bar>());
    ///
    /// entity.clear().unwrap();
    /// assert!(!entity.contains::<Foo>());
    /// assert!(!entity.contains::<Bar>());
    /// ```
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn clear(&mut self) -> Result<&mut Self, EntityError> {
        self.clear_with_caller(DebugLocation::caller())
    }

    #[inline(never)]
    pub(crate) fn clear_with_caller(
        &mut self,
        caller: DebugLocation,
    ) -> Result<&mut Self, EntityError> {
        self.validate()?;

        let entity = self.id;
        let world_cell = self.world;

        // Peek at the storage instead of taking it: the no-op (empty-table)
        // path must leave the cached storage intact, mirroring
        // `remove_dynamic_with_caller`.
        let location = unsafe { self.storage.as_ref().debug_checked_unwrap().1 };

        let old_table_id = location.table_id;
        let old_table_row = location.table_row;

        if old_table_id == TableId::EMPTY {
            return Ok(self);
        }

        // Only the move path consumes the cached storage; `relocate()`
        // restores it afterwards.
        unsafe {
            self.storage.take().debug_checked_unwrap();
        }

        let guard = ForgetEntityOnPanic {
            entity: self.id,
            world: self.world,
            caller,
        };

        let old_table = unsafe { world_cell.data_mut().tables.get_unchecked_mut(old_table_id) };
        let new_table = unsafe {
            world_cell
                .data_mut()
                .tables
                .get_unchecked_mut(TableId::EMPTY)
        };

        // --- trigger on_discard hooks ---
        {
            for &(id, hook) in old_table.on_discard_hooks() {
                let ctx = HookContext { id, entity, caller };
                let deferred = unsafe { world_cell.deferred() };
                hook(deferred, ctx);
            }
        }

        // --- trigger on_remove hooks ---
        {
            for &(id, hook) in old_table.on_remove_hooks() {
                let ctx = HookContext { id, entity, caller };
                let deferred = unsafe { world_cell.deferred() };
                hook(deferred, ctx);
            }
        }

        // --- move entity between tables ---
        let new_table_row = unsafe {
            // SAFETY: old_table_id and new_table_id are distinct.
            let (moved, new_table_row) = old_table.move_row::<true>(old_table_row, new_table);
            world_cell.full_mut().entities.update_row(moved).unwrap();
            new_table_row
        };

        unsafe {
            let location = &mut world_cell
                .full_mut()
                .entities
                .entities
                .get_unchecked_mut(entity.index() as usize)
                .location;
            *location = Some(Location {
                table_id: TableId::EMPTY,
                table_row: new_table_row,
            });
        }

        unsafe { world_cell.full_mut().flush() };

        self.relocate();

        core::mem::forget(guard);

        Ok(self)
    }
}
