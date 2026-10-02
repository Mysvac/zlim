//! [`BundleScratch`] and [`BundleWriter`] — inserting a bundle whose
//! components are only known at runtime.
//!
//! Both types are re-exported from [`crate::bundle`], where they are the
//! dynamic counterpart of the [`Bundle`] trait.
//!
//! [`Bundle`]: crate::bundle::Bundle

use core::any::TypeId;
use core::ptr::NonNull;

use zlim_ptr::OwningPtr;
use zlim_utils::debug::DebugLocation;
use zlim_utils::mem::Bump;

use crate::bundle::{Bundle, BundleId};
use crate::component::{Component, ComponentDB, ComponentId};
use crate::component::{ComponentCollector, Components};
use crate::component::{ComponentWriter, HookContext, Required};
use crate::entity::{EntityError, Location};
use crate::ops::EntityOwned;
use crate::table::{Table, TableId};
use crate::utils::{DebugCheckedUnwrap, ForgetEntityOnPanic};

// -----------------------------------------------------------------------------

struct ComponentCell {
    id: ComponentId,
    type_id: TypeId,
    data: NonNull<u8>,
    required: Option<Required>,
}

// -----------------------------------------------------------------------------
// BundleScratch

/// A reusable scratch space that collects components one at a time, so that
/// they can be written to an entity together as a dynamic bundle.
///
/// A [`Bundle`] names its components at compile time, which is what lets the
/// typed insertion path write a bundle straight into storage.  A scratch space
/// does the opposite: components are pushed at runtime, through the
/// [`BundleWriter`] returned by [`writer`](Self::writer), and are committed to
/// an entity in one operation by [`BundleWriter::write`].
///
/// [`Bundle`]: crate::bundle::Bundle
///
/// # Cost
///
/// Every pushed component is copied into the scratch space, and the scratch
/// space is copied into the entity's row on write, so this path costs **one
/// extra copy** compared to [`EntityOwned::insert`], which writes a bundle value
/// straight into storage.  In exchange, the component set does not have to be
/// known when the code is written.
///
/// A scratch space cannot create an entity — it only inserts into one that
/// already exists.
///
/// # Reuse
///
/// [`BundleWriter::write`] empties the scratch space and resets its arena
/// without releasing the memory, so the scratch space is cheapest when it is
/// created once and reused across writes: after the largest write it has seen,
/// it stops allocating.
///
/// # Leaking
///
/// A component that is pushed but never written is never dropped, because the
/// scratch space only owns its bytes.  A scratch space that is dropped while it
/// still holds components therefore reports the leak through [`zlim_log`](zlim_log),
/// and the same leak happens silently when a write fails, or when
/// [`writer`](Self::writer) discards what a skipped write left behind.
///
/// [`EntityOwned::insert`]: crate::ops::EntityOwned::insert
///
/// # Example
///
/// ```rust
/// use zlim_core::bundle::BundleScratch;
/// use zlim_core::prelude::*;
///
/// #[derive(Component, Clone, PartialEq, Debug)]
/// struct Hp(u32);
///
/// #[derive(Component, Clone, PartialEq, Debug)]
/// struct Armor(u32);
///
/// let mut world = World::alloc();
/// let mut entity = world.spawn(Hp(100), None);
///
/// // Components are decided while the writer lives, not before it.
/// let mut scratch = BundleScratch::default();
/// let mut writer = scratch.writer();
///
/// writer.push(Armor(50), None);
/// writer.write(&mut entity).unwrap();
///
/// assert_eq!(entity.get::<Armor>(), Some(&Armor(50)));
///
/// // The scratch space is empty again, and can be reused.
/// assert!(scratch.is_empty());
/// ```
pub struct BundleScratch {
    // Safety: this cannot be exposed, otherwise `alloc.reset()` could be called in arbitrary places.
    alloc: Bump,
    // Correctness: this should never be made public.
    components: Vec<ComponentCell>,
}

// SAFETY: The `NonNull`s in component_ptrs are always a `Component`, which is Send
unsafe impl Send for BundleScratch where Bump: Send {}

impl Default for BundleScratch {
    fn default() -> Self {
        Self::new()
    }
}

impl BundleScratch {
    /// Creates an empty [`BundleScratch`].
    ///
    /// Nothing is allocated up front: the first block of the arena is
    /// allocated when the first component is pushed, starting small (a few
    /// hundred bytes) and growing geometrically with each further block.
    ///
    /// [`BundleScratch::default`] is equivalent to this constructor.
    pub const fn new() -> Self {
        Self {
            // 400:
            // - First block is `512` (next_power_of_two)
            // - Third block is `1024` ((400 * 1.5).next_power_of_two)
            alloc: Bump::new(400),
            components: Vec::new(),
        }
    }

    /// Creates a new [`BundleWriter`] that pushes into this scratch space.
    ///
    /// Anything a previous writer pushed is discarded first.  That keeps the
    /// scratch space consistent when a write was skipped or unwound by a panic,
    /// but the discarded values are **not** dropped — they are leaked, exactly
    /// as if the scratch space itself had been dropped, without the report that
    /// [`Drop`] would have made.  A skipped [`BundleWriter::write`] is therefore
    /// an error path, not a normal one.
    pub fn writer(&mut self) -> BundleWriter<'_> {
        // This is necessary to ensure safety / correctness is maintained
        // in the context of catch_unwind or a skipped `write`.
        self.components.clear();
        BundleWriter(self)
    }

    /// Returns `true` if the scratch space currently holds no components.
    ///
    /// This is the case before the first push, and again after
    /// [`BundleWriter::write`] has consumed everything.
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    /// Drops the components the scratch space still holds, and resets its arena.
    ///
    /// A pushed component belongs to the scratch space: [`BundleWriter::write`]
    /// copies it into storage and gives it up, and a write that never happens
    /// leaves it here.  This is what a caller that gives up halfway calls, so
    /// that those values are dropped the way they would have been by the write —
    /// instead of leaving them for [`Drop`], which can only report the leak.
    ///
    /// `components` is the registry the components were pushed with, which is
    /// what knows how to drop a value of each type.  The scratch space is empty
    /// afterwards, and its arena is kept for the next writer.
    #[cold]
    pub fn manual_drop(&mut self, components: &Components) {
        for cell in self.components.drain(..) {
            let db = components
                .get_by_id(cell.id)
                .unwrap_or_else(|| ComponentDB::get_by_id(cell.id));

            if let Some(dropper) = db.dropper {
                // SAFETY: `cell.data` points to a value of this component that was
                // copied into the arena and never handed anywhere else.
                unsafe { dropper.call(OwningPtr::new(cell.data)) };
            }
        }

        self.alloc.reset();
    }
}

impl Drop for BundleScratch {
    /// Reports a scratch space that was dropped while it still held components.
    ///
    /// Those components were copied into the arena and are never dropped, so
    /// their allocation leaks.  Writing them is the only way to release them.
    #[track_caller]
    fn drop(&mut self) {
        // We cannot call `manual_drop` here.
        //
        // Drop may be triggered during a panic, and it cannot be guaranteed that
        // manual-drop will not result in double release (i.e. some data has been
        // consumed but not removed from the list).
        if !self.components.is_empty() {
            ::core::hint::cold_path();
            zlim_log::error!(
                "Unconsumed BundleScratch was found, which may cause memory leaks. {}",
                ::core::panic::Location::caller(),
            );
        }
    }
}

// -----------------------------------------------------------------------------
// BundleWriter

/// Pushes components into a [`BundleScratch`], which can then be written to an
/// existing entity as a dynamic bundle.
///
/// A writer is created by [`BundleScratch::writer`] and consumed by
/// [`write`](Self::write); every component pushed in between becomes part of
/// the same bundle:
///
/// ```rust
/// use zlim_core::bundle::BundleScratch;
/// use zlim_core::prelude::*;
///
/// #[derive(Component, Clone, PartialEq, Debug)]
/// struct Hp(u32);
///
/// #[derive(Component, Clone, PartialEq, Debug)]
/// struct Armor(u32);
///
/// #[derive(Component, Clone, PartialEq, Debug)]
/// struct Name(String);
///
/// let mut world = World::alloc();
/// let mut entity = world.spawn_empty(None);
///
/// let mut scratch = BundleScratch::default();
/// let mut writer = scratch.writer();
///
/// // The three components are handed over one by one...
/// writer.push(Hp(100), None);
/// writer.push(Armor(50), None);
/// writer.push(Name("knight".into()), None);
///
/// // ...and reach storage in a single write.
/// writer.write(&mut entity).unwrap();
///
/// assert_eq!(entity.get::<Hp>(), Some(&Hp(100)));
/// assert_eq!(entity.get::<Armor>(), Some(&Armor(50)));
/// assert_eq!(entity.get::<Name>(), Some(&Name("knight".into())));
/// # assert!(scratch.is_empty());
/// ```
///
/// # Limits
///
/// - It inserts into an existing entity; it never creates one.
/// - Pushed components are copied into the scratch space before they reach
///   storage, so this path costs one more copy than [`EntityOwned::insert`].
/// - A dynamic bundle carries data only: nothing runs once it is written.
/// - The bundle is written as one set: there is no way to push a component to
///   storage without committing the components pushed before it.
///
/// [`EntityOwned::insert`]: crate::ops::EntityOwned::insert
#[must_use]
pub struct BundleWriter<'a>(&'a mut BundleScratch);

impl BundleWriter<'_> {
    /// Returns `true` if nothing has been pushed since the writer was created.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.components.is_empty()
    }
}

impl<'a> BundleWriter<'a> {
    /// Pushes a component from it's pointer and data base.
    ///
    /// # Safety
    ///
    /// - `component` must point to a valid, initialised value of that type.
    /// - `db` must be correct component info for given pointer,
    #[inline(never)]
    unsafe fn push_with_db(&mut self, component: OwningPtr<'_>, db: &'static ComponentDB) {
        let id = db.id;
        let layout = db.layout;
        let type_id = db.type_id;
        let required = db.required;

        let data = self.0.alloc.alloc(layout);

        let src = component.as_ptr();
        let dst = data.as_ptr();
        let count = layout.size();

        // SAFETY:
        // - `component` points to a valid value with this layout per precondition
        // - `ptr` was just allocated (so cannot overlap) and has the same layout
        unsafe { core::ptr::copy_nonoverlapping::<u8>(src, dst, count) };

        let cell = ComponentCell {
            id,
            type_id,
            data,
            required,
        };

        self.0.components.push(cell);
    }

    /// # Safety
    ///
    /// - `ptr` must point to a valid, initialised component value.
    /// - `C` must be correct component type for given pointer,
    #[inline(never)]
    pub(super) fn push_owning<C: Component>(
        &mut self,
        ptr: OwningPtr<'_>,
        infos: Option<&Components>,
    ) {
        let db = match infos {
            Some(infos) => infos.get::<C>(),
            None => ComponentDB::of::<C>(),
        };

        unsafe { self.push_with_db(ptr, db) };
    }

    /// Pushes the given bundle to the back of the current bundle scratch space.
    ///
    /// The components are copied into the scratch space and is not dropped there:
    /// it is written to the entity by [`write`](Self::write), and is leaked if
    /// that never happens.
    ///
    /// `infos` is the registry the component type is looked up in — usually
    /// the target world's, as returned by [`World::components`].  Passing `None`
    /// uses the process-global registry instead; both resolve to the same
    /// [`ComponentDB`], and neither is required for the write to succeed.
    ///
    /// Pushing the same component type more than once is allowed.  On write the
    /// later value replaces the earlier one, which is dropped like any other
    /// overwritten value.
    ///
    /// [`World::components`]: crate::world::World::components
    #[inline]
    pub fn push<B: Bundle>(&mut self, bundle: B, infos: Option<&Components>) {
        zlim_ptr::into_owning!(bundle);
        unsafe { B::push_to(bundle, self, infos) };
    }
}

// -----------------------------------------------------------------------------

impl<'a> BundleWriter<'a> {
    /// Writes every pushed component into `entity` at once, then empties the
    /// scratch space.
    ///
    /// The components are inserted the same way [`EntityOwned::insert`] inserts
    /// a bundle: a component the entity already has is overwritten — its old
    /// value is dropped and its `changed` tick is bumped — a component it does
    /// not have yet is added, and the entity is moved to another table whenever
    /// the set of component types changes.  Components are written in the order
    /// they were pushed.  Required components of the pushed components are
    /// collected into the same bundle, and the ones the entity does not already
    /// have are filled in from their `Default`, so the target table holds a
    /// complete row.
    ///
    /// On the way out the scratch space is left empty and its arena is reset,
    /// keeping its memory for the next writer.
    ///
    /// [`EntityOwned::insert`]: crate::ops::EntityOwned::insert
    ///
    /// # Errors
    ///
    /// Returns [`EntityError::NotSpawned`] if `entity` is not currently
    /// spawned — for example a stale handle, or one that was despawned through
    /// [`EntityOwned::world_scope`].  Nothing is written in that case, and the
    /// pushed components will be cleared.
    ///
    /// [`EntityOwned::world_scope`]: crate::ops::EntityOwned::world_scope
    ///
    /// # Panics
    ///
    /// Panics if a component hook run by this write panics, or if a pushed
    /// component's id is not known to the entity's world (that one is checked in
    /// debug builds only).
    #[inline(never)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn write(self, entity: &mut EntityOwned) -> Result<(), EntityError> {
        // modified from `EntityOwned::insert`

        if let Err(e) = entity.validate() {
            ::core::hint::cold_path();
            self.0.components.clear();
            self.0.alloc.reset();
            return Err(e);
        }

        let components = {
            let infos = Some(&entity.world().components);
            let mut collector = ComponentCollector::new(infos);
            for cell in &self.0.components {
                collector.insert(cell.id);
                if let Some(required) = cell.required {
                    required.collect(&mut collector);
                }
            }
            collector.finish()
        };

        let world_cell = entity.world;
        let world = unsafe { world_cell.data_mut() };

        let bundle_id = world.bundles.register_dynamic(components);

        let current_table_id = unsafe { entity.storage.as_ref().debug_checked_unwrap().1.table_id };

        let new_table_id = world.tables.table_after_insert(
            current_table_id,
            bundle_id,
            &world.bundles,
            &world.components,
        );

        let caller = DebugLocation::caller();

        let guard = ForgetEntityOnPanic {
            entity: entity.id,
            world: entity.world,
            caller,
        };

        if current_table_id == new_table_id {
            insert_local(entity, self.0, bundle_id, caller);
        } else {
            insert_moved(entity, self.0, bundle_id, new_table_id, caller);
        }

        core::mem::forget(guard);

        self.0.components.clear();
        self.0.alloc.reset();

        Ok(())
    }
}

// modified from `EntityOwned::insert`
fn insert_local(
    this: &mut EntityOwned,
    data: &mut BundleScratch,
    bundle_id: BundleId,
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
        // NOTE: hook-queued commands must NOT be flushed here — `flush()`
        // can create new tables (reallocating the `Table` vector, dangling
        // `table` above) or move/despawn the entity (stale `table_row`).
        // The single flush at the end of this function applies them safely.
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

            for cell in &data.components {
                writer.write_raw(cell.type_id, OwningPtr::new(cell.data));
            }
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

// modified from `EntityOwned::insert`
fn insert_moved(
    this: &mut EntityOwned,
    data: &mut BundleScratch,
    bundle_id: BundleId,
    new_table_id: TableId,
    caller: DebugLocation,
) {
    let entity = this.id;
    let world_cell = this.world;
    let components = unsafe { &world_cell.read_only().components };

    let get_required = |id: ComponentId| {
        components
            .get_by_id(id)
            .unwrap_or_else(|| ComponentDB::get_by_id(id))
            .required
    };

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
            for cell in &data.components {
                writer.write_raw(cell.type_id, OwningPtr::new(cell.data));
            }
            for cell in &data.components {
                if let Some(required) = get_required(cell.id) {
                    required.write(&mut writer);
                }
            }
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
