//! Component-removal methods implemented on `EntityOwned`.

use zlim_utils::debug::DebugLocation;

use crate::bundle::{BundleId, DataBundle};
use crate::component::HookContext;
use crate::entity::{EntityError, Location};
use crate::ops::entity::EntityOwned;
use crate::table::TableId;
use crate::utils::DebugCheckedUnwrap;
use crate::utils::ForgetEntityOnPanic;

// -----------------------------------------------------------------------------
// remove
// -----------------------------------------------------------------------------

impl EntityOwned<'_> {
    /// Removes all components explicitly included in the `Bundle` from this
    /// entity.
    ///
    /// A alias function of [`EntityOwned::remove_explicit`].
    ///
    /// Components that do not exist on the entity are silently ignored.
    /// If the removal changes the entity's component set, the entity moves
    /// to a different table.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::derive::Component;
    ///
    /// #[derive(TypePath, Component, Clone, PartialEq, Debug)]
    /// struct Hp(u32);
    ///
    /// #[derive(TypePath, Component, Clone, PartialEq, Debug)]
    /// struct Mana(u32);
    ///
    /// let mut world = World::alloc();
    /// let mut entity = world.spawn((Hp(100), Mana(50)), None);
    ///
    /// entity.remove::<Mana>().unwrap();
    /// assert!(!entity.contains::<Mana>());
    /// assert_eq!(entity.get::<Hp>(), Some(&Hp(100)));
    /// ```
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn remove<B: DataBundle>(&mut self) -> Result<&mut Self, EntityError> {
        self.remove_explicit_with_caller::<B>(DebugLocation::caller())
    }

    /// Removes all components included in the `Bundle` from this entity.
    ///
    /// The required components included in the bundle will also be removed
    /// (if they are not dependent on other components).
    ///
    /// Components that do not exist on the entity are silently ignored.
    /// If the removal changes the entity's component set, the entity moves
    /// to a different table.
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn remove_required<B: DataBundle>(&mut self) -> Result<&mut Self, EntityError> {
        self.remove_required_with_caller::<B>(DebugLocation::caller())
    }

    /// Removes all components explicitly included in the `Bundle` from this
    /// entity.
    ///
    /// Components that do not exist on the entity are silently ignored.
    /// If the removal changes the entity's component set, the entity moves
    /// to a different table.
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn remove_explicit<B: DataBundle>(&mut self) -> Result<&mut Self, EntityError> {
        self.remove_explicit_with_caller::<B>(DebugLocation::caller())
    }

    /// Removes the components described by the given [`BundleId`] from
    /// this entity.
    ///
    /// Components that do not exist on the entity are silently ignored.
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn remove_dynamic(&mut self, bundle_id: BundleId) -> Result<&mut Self, EntityError> {
        self.remove_dynamic_with_caller(bundle_id, DebugLocation::caller())
    }
}

// -----------------------------------------------------------------------------
// remove_with_caller / remove_dynamic_with_caller
// -----------------------------------------------------------------------------

impl EntityOwned<'_> {
    #[inline]
    pub(crate) fn remove_explicit_with_caller<B: DataBundle>(
        &mut self,
        caller: DebugLocation,
    ) -> Result<&mut Self, EntityError> {
        let bundle_id = unsafe { self.world.full_mut().register_explicit_bundle::<B>() };

        self.remove_dynamic_with_caller(bundle_id, caller)
    }

    #[inline]
    pub(crate) fn remove_required_with_caller<B: DataBundle>(
        &mut self,
        caller: DebugLocation,
    ) -> Result<&mut Self, EntityError> {
        let bundle_id = unsafe { self.world.full_mut().register_required_bundle::<B>() };

        self.remove_dynamic_with_caller(bundle_id, caller)
    }

    /// Internal implementation of component removal.
    ///
    /// If the target table differs from the current table, the entity moves.
    /// Otherwise, the bundle components do not exist on this entity and the
    /// call is a no-op.
    #[inline(never)]
    pub(crate) fn remove_dynamic_with_caller(
        &mut self,
        bundle_id: BundleId,
        caller: DebugLocation,
    ) -> Result<&mut Self, EntityError> {
        self.validate()?;

        let world_cell = self.world;
        let world = unsafe { world_cell.data_mut() };

        // Peek at the storage instead of taking it: `remove_moved` below is
        // the only consumer allowed to take the storage, and the no-op
        // (same-table) path must leave it intact.
        let location = unsafe { self.storage.as_ref().debug_checked_unwrap().1 };
        let current_table_id = location.table_id;

        let new_table_id = world.tables.table_after_remove(
            current_table_id,
            bundle_id,
            &world.bundles,
            &world.components,
        );

        if current_table_id == new_table_id {
            return Ok(self);
        }

        let guard = ForgetEntityOnPanic {
            entity: self.id,
            world: self.world,
            caller,
        };

        remove_moved(self, new_table_id, caller);

        core::mem::forget(guard);
        Ok(self)
    }
}

// -----------------------------------------------------------------------------
// remove_moved
// -----------------------------------------------------------------------------

fn remove_moved(this: &mut EntityOwned, new_table_id: TableId, caller: DebugLocation) {
    let entity = this.id;
    let world_cell = this.world;

    // old table reference may be invalid after new table created.
    let (_, location) = unsafe { this.storage.take().debug_checked_unwrap() };
    let old_table_id = location.table_id;
    let old_table_row = location.table_row;

    let old_table = unsafe { world_cell.data_mut().tables.get_unchecked_mut(old_table_id) };
    let new_table = unsafe { world_cell.data_mut().tables.get_unchecked_mut(new_table_id) };

    // --- trigger on_discard hooks ---
    {
        for &(id, hook) in old_table.on_discard_hooks() {
            if !new_table.contains_component(id) {
                let ctx = HookContext { id, entity, caller };
                let deferred = unsafe { world_cell.deferred() };
                hook(deferred, ctx);
            }
        }
    }

    // --- trigger on_remove hooks ---
    {
        for &(id, hook) in old_table.on_remove_hooks() {
            if !new_table.contains_component(id) {
                let ctx = HookContext { id, entity, caller };
                let deferred = unsafe { world_cell.deferred() };
                hook(deferred, ctx);
            }
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
            table_id: new_table_id,
            table_row: new_table_row,
        });
    }

    unsafe { world_cell.full_mut().flush() };

    this.relocate();
}

// -----------------------------------------------------------------------------
