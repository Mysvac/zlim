//! Component-insertion methods implemented on `EntityOwned`.

use zlim_utils::debug::DebugLocation;

use crate::bundle::{Bundle, BundleId, DataBundle};
use crate::component::{ComponentWriter, HookContext};
use crate::entity::{EntityError, Location};
use crate::ops::entity::EntityOwned;
use crate::table::{Table, TableId};
use crate::utils::DebugCheckedUnwrap;
use crate::utils::ForgetEntityOnPanic;

// -----------------------------------------------------------------------------
// insert
// -----------------------------------------------------------------------------

impl EntityOwned<'_> {
    /// Inserts all components from the given `Bundle` into this entity.
    ///
    /// Components that already exist on the entity are **overwritten**.
    /// If the bundle introduces new component types, the entity moves to
    /// a different table.
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
    /// struct Armor(u32);
    ///
    /// let mut world = World::alloc();
    /// let mut entity = world.spawn(Hp(100), None);
    ///
    /// // Adding a new component type moves the entity to a new table.
    /// entity.insert(Armor(50)).unwrap();
    /// assert_eq!(entity.get::<Armor>(), Some(&Armor(50)));
    ///
    /// // Re-inserting an existing component overwrites its value.
    /// entity.insert(Hp(75)).unwrap();
    /// assert_eq!(entity.get::<Hp>(), Some(&Hp(75)));
    /// ```
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn insert<B: Bundle>(&mut self, bundle: B) -> Result<&mut Self, EntityError> {
        self.insert_with_caller(bundle, DebugLocation::caller())
    }

    /// Inserts a new bundle if the entity is missing **any** of its
    /// components.
    ///
    /// If all components already exist, nothing happens.  Otherwise the
    /// provided closure is called to produce the bundle value and all of
    /// its components are inserted (overwriting any existing ones).
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
    /// struct Speed(f32);
    ///
    /// let mut world = World::alloc();
    /// let mut entity = world.spawn(Hp(100), None);
    ///
    /// // `Hp` already exists, so the closure is never invoked.
    /// entity.insert_if_new(|| Hp(1)).unwrap();
    /// assert_eq!(entity.get::<Hp>(), Some(&Hp(100)));
    ///
    /// // `Speed` is missing, so it is inserted.
    /// entity.insert_if_new(|| Speed(3.0)).unwrap();
    /// assert_eq!(entity.get::<Speed>(), Some(&Speed(3.0)));
    /// ```
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn insert_if_new<B: DataBundle>(
        &mut self,
        f: impl FnOnce() -> B,
    ) -> Result<&mut Self, EntityError> {
        self.insert_if_new_with_caller(f, DebugLocation::caller())
    }
}

// -----------------------------------------------------------------------------
// insert_with_caller
// -----------------------------------------------------------------------------

impl EntityOwned<'_> {
    #[inline]
    pub(crate) fn insert_with_caller<B: Bundle>(
        &mut self,
        bundle: B,
        caller: DebugLocation,
    ) -> Result<&mut Self, EntityError> {
        self.validate()?;

        let world_cell = self.world;
        let world = unsafe { world_cell.data_mut() };
        let bundle_id = world.register_required_bundle::<B>();
        let current_table_id = unsafe { self.storage.as_ref().debug_checked_unwrap().1.table_id };

        let new_table_id = world.tables.table_after_insert(
            current_table_id,
            bundle_id,
            &world.bundles,
            &world.components,
        );

        let guard = ForgetEntityOnPanic {
            entity: self.id,
            world: self.world,
            caller,
        };

        zlim_ptr::into_owning!(bundle);
        let data = bundle;

        if current_table_id == new_table_id {
            insert_local(
                self,
                data,
                bundle_id,
                B::write_explicit,
                B::write_required,
                caller,
            );
        } else {
            insert_moved(
                self,
                data,
                bundle_id,
                new_table_id,
                B::write_explicit,
                B::write_required,
                caller,
            );
        }

        core::mem::forget(guard);
        Ok(self)
    }

    pub(crate) fn insert_if_new_with_caller<B: DataBundle>(
        &mut self,
        f: impl FnOnce() -> B,
        caller: DebugLocation,
    ) -> Result<&mut Self, EntityError> {
        self.validate()?;

        // Check whether all bundle components already exist.  Use a
        // nested block so that the `world` borrow ends before we call
        // into `insert_with_caller`.
        unsafe {
            let world = self.world.full_mut();
            let bundle_id = world.register_required_bundle::<B>();
            let info = world.bundles.get_unchecked(bundle_id);
            let table = &self.storage.as_ref().debug_checked_unwrap().0;
            if info
                .components()
                .iter()
                .all(|&id| table.contains_component(id))
            {
                return Ok(self);
            }
        }

        self.insert_with_caller(f(), caller)
    }
}

// -----------------------------------------------------------------------------
// insert_with_caller
// -----------------------------------------------------------------------------

#[inline(never)]
fn insert_local(
    this: &mut EntityOwned,
    data: zlim_ptr::OwningPtr<'_>,
    bundle_id: BundleId,
    write_fn: unsafe fn(zlim_ptr::OwningPtr<'_>, &mut ComponentWriter),
    write_required_fn: unsafe fn(&mut ComponentWriter),
    caller: DebugLocation,
) {
    let entity = this.id;
    let world_cell = this.world;

    let (_, location) = unsafe { this.storage.take().debug_checked_unwrap() };
    let table_id = location.table_id;
    let table_row = location.table_row;

    let table = unsafe { world_cell.data_mut().tables.get_unchecked_mut(table_id) };

    // --- trigger on_discard hooks for overwritten components ---
    {
        let world = unsafe { world_cell.data_mut() };
        let info = unsafe { world.bundles.get_unchecked(bundle_id) };

        for &(id, hook) in table.on_discard_hooks() {
            if info.contains_component(id) {
                let ctx = HookContext { id, entity, caller };
                let deferred = unsafe { world_cell.deferred() };
                hook(deferred, ctx);
            }
        }
        world.flush();
    }

    // --- write data into the current row ---
    {
        let world = unsafe { world_cell.data_mut() };
        let tick = world.this_run_fast();
        let table_ptr = table as *mut Table;

        unsafe {
            let mut writer = ComponentWriter::from_table(&mut *table_ptr, table_row, tick);
            // SAFETY: assume_init does not access `Table`.
            (*table_ptr).types().for_each(|ty| writer.assume_init(ty));
            write_fn(data, &mut writer);
            write_required_fn(&mut writer);
        }
    }

    // --- trigger on_insert hooks ---
    {
        let world = unsafe { world_cell.data_mut() };
        let info = unsafe { world.bundles.get_unchecked(bundle_id) };

        for &(id, hook) in table.on_insert_hooks() {
            if info.contains_component(id) {
                let ctx = HookContext { id, entity, caller };
                let deferred = unsafe { world_cell.deferred() };
                hook(deferred, ctx);
            }
        }
    }

    unsafe { world_cell.full_mut().flush() };

    this.relocate();
}

// -----------------------------------------------------------------------------
// insert_moved — move entity to a different table
// -----------------------------------------------------------------------------

#[inline(never)]
fn insert_moved(
    this: &mut EntityOwned,
    data: zlim_ptr::OwningPtr<'_>,
    bundle_id: BundleId,
    new_table_id: TableId,
    write_fn: unsafe fn(zlim_ptr::OwningPtr<'_>, &mut ComponentWriter),
    write_required_fn: unsafe fn(&mut ComponentWriter),
    caller: DebugLocation,
) {
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
        let info = unsafe { world_cell.data_mut().bundles.get_unchecked(bundle_id) };

        for &(id, hook) in old_table.on_discard_hooks() {
            if info.contains_component(id) {
                let ctx = HookContext { id, entity, caller };
                let deferred = unsafe { world_cell.deferred() };
                hook(deferred, ctx);
            }
        }
    }

    // --- move entity between tables ---
    let new_table_row = unsafe {
        // SAFETY: old_table_id and new_table_id are distinct.
        let (moved, new_table_row) = old_table.move_row::<false>(old_table_row, new_table);
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

    // --- write data into the current row ---
    {
        let world = unsafe { world_cell.data_mut() };
        let tick = world.this_run_fast();
        unsafe {
            let mut writer = ComponentWriter::from_table(new_table, new_table_row, tick);
            old_table.types().for_each(|ty| writer.assume_init(ty));
            write_fn(data, &mut writer);
            write_required_fn(&mut writer);
        }
    }

    // --- trigger on_add hooks for newly added components ---
    {
        for &(id, hook) in new_table.on_add_hooks() {
            if !old_table.contains_component(id) {
                let ctx = HookContext { id, entity, caller };
                let deferred = unsafe { world_cell.deferred() };
                hook(deferred, ctx);
            }
        }
    }

    // --- trigger on_insert hooks ---
    {
        let info = unsafe { world_cell.data_mut().bundles.get_unchecked(bundle_id) };

        for &(id, hook) in new_table.on_insert_hooks() {
            if info.contains_component(id) {
                let ctx = HookContext { id, entity, caller };
                let deferred = unsafe { world_cell.deferred() };
                hook(deferred, ctx);
            }
        }
    }

    unsafe { world_cell.full_mut().flush() };

    this.relocate();
}

// -----------------------------------------------------------------------------
