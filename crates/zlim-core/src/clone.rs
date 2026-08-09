use core::any::TypeId;
use core::ptr::NonNull;
use std::collections::VecDeque;

use crate::table::{Column, TableCol};
use crate::utils::{DebugCheckedUnwrap, DebugName};
use zlim_ptr::{OwningPtr, Ptr, PtrMut};
use zlim_utils::vec::SmallVec;

use crate::component::Component;
use crate::component::ComponentId;
use crate::entity::{EntityId, EntityMap, EntityMapper};
use crate::utils::{DebugLocation, ForgetEntityOnPanic};
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------
// CloneSource & CloneTarget & CloneValue
// -----------------------------------------------------------------------------

/// Type-erased read-only view of one source component value.
///
/// This is primarily used by custom component cloners.
pub struct CloneSource<'a> {
    ptr: Ptr<'a>,
    name: &'static str,
    type_id: TypeId,
}

/// Type-erased write target for one cloned component value.
///
/// The underlying slot may start as uninitialized memory and must be
/// initialized by the active cloner before returning.
pub struct CloneTarget<'a> {
    ptr: OwningPtr<'a>,
    name: &'static str,
    type_id: TypeId,
    initialized: &'a mut bool,
}

/// Type-erased mutable view used in deferred clone callbacks.
///
/// This is used after plain cloning, when source-target entity mapping is
/// fully available.
pub struct CloneValue<'a> {
    ptr: PtrMut<'a>,
    name: DebugName,
    type_id: TypeId,
}

// -----------------------------------------------------------------------------
// CloneSource & CloneTarget & CloneValue Methods
// -----------------------------------------------------------------------------

impl CloneSource<'_> {
    #[cold]
    #[inline(never)]
    fn invalid_type(actual: &str, read: DebugName) -> ! {
        panic!("CloneSource Error: try read value as `{read}`, but the actual type is `{actual}`.")
    }

    /// Verifies that this source value has type `C`.
    ///
    /// Panics if the type does not match.
    #[inline(always)]
    pub fn assert_type<C: 'static>(&self) {
        if self.type_id != TypeId::of::<C>() {
            Self::invalid_type(self.name, DebugName::type_name::<C>());
        }
    }

    /// Reads this source value as `C`.
    ///
    /// Panics if the requested type does not match the actual component type.
    #[inline(always)]
    pub fn read<C: Sized + 'static>(&self) -> &C {
        self.assert_type::<C>();
        unsafe { self.ptr.deref() }
    }
}

impl CloneTarget<'_> {
    #[cold]
    #[inline(never)]
    fn invalid_type(actual: &str, read: DebugName) -> ! {
        panic!("CloneTarget Error: try write value as `{read}`, but the actual type is `{actual}`.")
    }

    /// Verifies that this target slot has type `C`.
    ///
    /// Panics if the type does not match.
    #[inline(always)]
    pub fn assert_type<C: 'static>(&self) {
        if self.type_id != TypeId::of::<C>() {
            Self::invalid_type(self.name, DebugName::type_name::<C>());
        }
    }

    /// Returns whether this target slot is already initialized.
    #[inline(always)]
    pub fn is_initialized(&self) -> bool {
        *self.initialized
    }

    /// Marks this target slot as initialized without writing through [`Self::write`].
    ///
    /// # Safety
    /// Caller must guarantee that the target slot already contains a fully
    /// initialized, valid value for this component type.
    #[inline(always)]
    pub unsafe fn assume_initialized(&mut self) {
        *self.initialized = true;
    }

    /// Writes a cloned component value into this target slot.
    ///
    /// If the slot is already initialized, the previous value is dropped first.
    /// Panics if `C` does not match the target component type.
    #[inline(always)]
    pub fn write<C: Sized + 'static>(&mut self, value: C) {
        self.assert_type::<C>();

        unsafe {
            if *self.initialized {
                self.ptr.borrow_mut().promote().drop_as::<C>();
            }
            self.ptr.write(value);
        }

        *self.initialized = true;
    }
}

impl CloneValue<'_> {
    #[cold]
    #[inline(never)]
    fn invalid_type(actual: DebugName, read: DebugName) -> ! {
        panic!("CloneValue Error: try modify value as `{read}`, but the actual type is `{actual}`.")
    }

    /// Verifies that this value has type `C`.
    ///
    /// Panics if the type does not match.
    #[inline(always)]
    pub fn assert_type<C: 'static>(&self) {
        if self.type_id != TypeId::of::<C>() {
            Self::invalid_type(self.name, DebugName::type_name::<C>());
        }
    }

    /// Mutates this value as `C`.
    ///
    /// This is used in deferred callbacks after plain component cloning.
    #[inline(always)]
    pub fn mutate<C: Sized + 'static>(&mut self, fun: impl FnOnce(&mut C)) {
        self.assert_type::<C>();

        unsafe {
            fun(self.ptr.as_mut::<C>());
        }
    }
}

// -----------------------------------------------------------------------------
// CloneContext & CloneEntityMapper & Callback
// -----------------------------------------------------------------------------

pub type CloneEntityMapper = EntityMap<EntityId>;

struct Callback {
    func: Box<dyn FnOnce(CloneValue, &mut CloneEntityMapper)>,
    id: ComponentId,
    entity: EntityId,
    name: DebugName,
    type_id: TypeId,
}

/// Per-component context passed through a clone run.
///
/// This exposes source/target entities and provides deferred operations for
/// entity remapping and post-clone mutation.
pub struct CloneContext {
    name: DebugName,
    linked_clone: bool,
    id: ComponentId,
    type_id: TypeId,
    source: EntityId,
    target: EntityId,
    deferred: Vec<EntityId>,
    callback: Vec<Callback>,
}

// -----------------------------------------------------------------------------
// CloneContext Methods
// -----------------------------------------------------------------------------

impl CloneContext {
    #[cold]
    #[inline(never)]
    fn invalid_type(actual: DebugName, read: DebugName) -> ! {
        panic!(
            "CloneContext Error: try callback value as `{read}`, but the actual type is `{actual}`."
        )
    }

    pub(crate) fn new(linked_clone: bool) -> Self {
        Self {
            linked_clone,
            id: ComponentId::without_provenance(0),
            source: EntityId::PLACEHOLDER,
            target: EntityId::PLACEHOLDER,
            type_id: TypeId::of::<Self>(),
            name: DebugName::anonymous(),
            deferred: Vec::new(),
            callback: Vec::new(),
        }
    }

    /// Verifies that the current component type is `C`.
    ///
    /// Panics if the requested type does not match the current clone step.
    #[inline(always)]
    pub fn assert_type<C: 'static>(&self) {
        if self.type_id != TypeId::of::<C>() {
            Self::invalid_type(self.name, DebugName::type_name::<C>());
        }
    }

    /// Returns the currently cloned component id.
    pub fn id(&self) -> ComponentId {
        self.id
    }

    /// Returns whether this clone run is in linked mode.
    pub fn linked_clone(&self) -> bool {
        self.linked_clone
    }

    /// Returns the source entity of the current clone step.
    pub fn source_entity(&self) -> EntityId {
        self.source
    }

    /// Returns the target entity of the current clone step.
    pub fn target_entity(&self) -> EntityId {
        self.target
    }

    /// Schedules another entity to be cloned in the same run.
    ///
    /// This is typically used by relationship-target cloners in linked mode.
    pub fn defer_clone(&mut self, entity: EntityId) {
        self.deferred.push(entity);
    }

    /// Schedules deferred entity-remapping for component type `C`.
    ///
    /// This calls [`Component::map_entities`] for the cloned component.
    pub fn defer_map_entities<C: Component>(&mut self) {
        self.assert_type::<C>();

        let wrapper = move |mut value: CloneValue, mapper: &mut CloneEntityMapper| {
            value.mutate::<C>(|c| Component::map_entities(c, mapper))
        };

        self.callback.push(Callback {
            id: self.id,
            entity: self.target,
            func: Box::new(wrapper),
            name: self.name,
            type_id: self.type_id,
        });
    }

    /// Schedules a custom deferred mutation for component type `C`.
    ///
    /// This is useful when cloning needs source-target mapping that is only
    /// available after all target entities have been allocated.
    pub fn defer_mutate<C: Component>(
        &mut self,
        func: impl FnOnce(&mut C, &mut CloneEntityMapper) + Send + 'static,
    ) {
        self.assert_type::<C>();

        let wrapper = move |mut value: CloneValue, mapper: &mut CloneEntityMapper| {
            value.mutate::<C>(|c| func(c, mapper))
        };

        self.callback.push(Callback {
            id: self.id,
            entity: self.target,
            func: Box::new(wrapper),
            name: self.name,
            type_id: self.type_id,
        });
    }
}

// -----------------------------------------------------------------------------
// ComponentCloner
// -----------------------------------------------------------------------------

/// Strategy object describing how a single component type is cloned.
///
/// Most component types should use [`Self::copyable`] or [`Self::clonable`].
/// Relationship-aware types should use [`Self::relationship`] or
/// [`Self::relationship_target`].
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct ComponentCloner {
    func: fn(src: CloneSource<'_>, dst: CloneTarget<'_>, ctx: &mut CloneContext),
}

impl ComponentCloner {
    /// Creates a cloner that performs a byte-for-byte copy for `Copy` components.
    ///
    /// If `C::NO_ENTITY` is `false`, deferred entity remapping is queued.
    #[inline(always)]
    pub const fn copyable<C: Copy + Component>() -> Self {
        Self {
            func: |src, mut dst, ctx| {
                #[cfg(debug_assertions)]
                {
                    assert!(!dst.is_initialized());
                    src.assert_type::<C>();
                    dst.assert_type::<C>();
                    ctx.assert_type::<C>();
                    src.ptr.debug_assert_aligned::<C>();
                    dst.ptr.debug_assert_aligned::<C>();
                }

                unsafe {
                    dst.assume_initialized();
                    let src = src.ptr.as_ptr() as *const C;
                    let dst = dst.ptr.as_ptr() as *mut C;
                    core::ptr::copy_nonoverlapping::<C>(src, dst, 1);
                }

                if !C::NO_ENTITY {
                    ctx.defer_map_entities::<C>();
                }
            },
        }
    }

    /// Creates a cloner that calls [`Clone::clone`] for components.
    ///
    /// If `C::NO_ENTITY` is `false`, deferred entity remapping is queued.
    #[inline(always)]
    pub const fn clonable<C: Clone + Component>() -> Self {
        Self {
            func: |src, mut dst, ctx| {
                #[cfg(debug_assertions)]
                {
                    assert!(!dst.is_initialized());
                    src.assert_type::<C>();
                    dst.assert_type::<C>();
                    ctx.assert_type::<C>();
                    src.ptr.debug_assert_aligned::<C>();
                    dst.ptr.debug_assert_aligned::<C>();
                }

                unsafe {
                    dst.ptr.write::<C>(Clone::clone(src.ptr.deref::<C>()));
                    dst.assume_initialized();
                }

                if !C::NO_ENTITY {
                    ctx.defer_map_entities::<C>();
                }
            },
        }
    }

    /// Creates a fully custom cloner.
    ///
    /// Most users should prefer [`Self::copyable`] or [`Self::clonable`].
    ///
    /// A custom cloner should always initialize `dst` and should queue remap
    /// when the component contains embedded entities.
    #[inline(always)]
    pub const fn custom(func: fn(CloneSource, CloneTarget, &mut CloneContext)) -> Self {
        Self { func }
    }

    /// Invokes this cloner.
    #[inline(always)]
    pub fn call(
        self,
        source: CloneSource<'_>,
        target: CloneTarget<'_>,
        context: &mut CloneContext,
    ) {
        (self.func)(source, target, context)
    }
}

// -----------------------------------------------------------------------------
// EntityCloner Methods
// -----------------------------------------------------------------------------

/// High-level entity cloning entry point.
///
/// Create this via [`World::entity_cloner`], then call
/// [`Self::spawn_clone`] or [`Self::spawn_clone_batch`].
pub struct EntityCloner<'w> {
    world: WorldCell<'w>,
    mapper: CloneEntityMapper,
    cloned: Vec<EntityId>,
    wait: VecDeque<EntityId>,
}

impl<'w> EntityCloner<'w> {
    /// Creates an entity cloner bound to the given world.
    pub fn new(world: &mut World) -> EntityCloner<'_> {
        EntityCloner {
            world: world.cell(),
            mapper: EntityMap::new(),
            cloned: Vec::new(),
            wait: VecDeque::new(),
        }
    }

    /// Clones a batch of entities.
    ///
    /// The returned vector preserves input order and contains cloned target
    /// entities for each input source entity.
    ///
    /// If relationship cloning is enabled, the number of returned entities may
    /// exceed the number of input entities. However, it is guaranteed that the
    /// indices of the cloned targets correspond one-to-one with the inputs:
    /// the first N elements of the returned vector are the direct clones of the
    /// N input entities, and any remaining elements are products of recursive
    /// cloning (e.g., children or related entities).
    ///
    /// In theory, the order of input elements does not negatively affect the result.
    /// Hierarchical relationships are established after all prototypes have been
    /// cloned.
    ///
    /// If `LINKED` is `true`, children entities will be recursively cloned.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn spawn_clone_batch(
        &mut self,
        entities: &[EntityId],
        linked_clone: bool,
    ) -> Vec<EntityId> {
        let caller = DebugLocation::caller();
        self.wait.extend(entities);
        self.run(linked_clone, caller).into_vec()
    }

    /// Clones one entity and returns the cloned target entity id.
    ///
    /// If `LINKED` is `true`, children entities will be recursively cloned.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn spawn_clone(&mut self, entity: EntityId, linked_clone: bool) -> EntityId {
        let caller = DebugLocation::caller();
        self.wait.push_back(entity);
        self.run(linked_clone, caller)[0]
    }
}

// -----------------------------------------------------------------------------
// EntityCloner Implementation
// -----------------------------------------------------------------------------

impl<'w> EntityCloner<'w> {
    /// Clones one entity and returns the cloned target entity id.
    ///
    /// If `LINKED` is `true`, the children entities will be recursively cloned.
    #[inline(never)]
    fn run(&mut self, linked_clone: bool, caller: DebugLocation) -> SmallVec<EntityId, 2> {
        let mut context = CloneContext::new(linked_clone);

        // Store entities that are explicitly cloned.
        let mut output: SmallVec<EntityId, 2> = SmallVec::with_capacity(self.wait.len());
        self.wait
            .iter()
            .for_each(|&e| unsafe { output.push_unchecked(e) });

        // -------------------------------------------------------------------
        // Forget Guard
        // -------------------------------------------------------------------

        // If the program panic, we need to forget all cloned entities.
        struct ForgetGuard<'a> {
            world: WorldCell<'a>,
            cloned: NonNull<Vec<EntityId>>,
            caller: DebugLocation,
        }

        impl Drop for ForgetGuard<'_> {
            #[cold]
            #[inline(never)]
            fn drop(&mut self) {
                unsafe {
                    let world = self.world.full_mut();
                    let entities = self.cloned.as_mut().as_slice();
                    for &entity in entities {
                        world.forget_with_caller(entity, self.caller);
                    }
                }
            }
        }

        let forget_guard = ForgetGuard {
            world: self.world,
            cloned: NonNull::from(&self.cloned),
            caller,
        };

        // -------------------------------------------------------------------
        // Plain Clone
        // -------------------------------------------------------------------

        // Clone all waiting entities.
        while let Some(source) = self.wait.pop_front() {
            let world1 = unsafe { self.world.full_mut() };
            // Obtain the Archetype Info of the source entity.
            let node = match world1.entities.get(source) {
                Ok(location) => location,
                Err(e) => {
                    core::hint::cold_path();
                    log::warn!("Try Clone Entity `{source}` but it is not spawned. {e}. {caller}");
                    continue;
                }
            };

            // We will map it after everything is completed.
            let child_of = node.child_of;

            // SAFETY: `EntityTree::get` return `Err` if `location` is `None`.
            let location = unsafe { node.location.debug_checked_unwrap() };

            let table_id = location.table_id;
            let src_row = location.table_row;

            // Spawn a uninitialized entity from given Archetype.
            let uninit_entity =
                unsafe { world1.spawn_uninit_with_caller(table_id, caller, child_of) };

            // `ForgetGuard` can not forget this cloning entity.
            // We need handle it manually.
            let item_guard = ForgetEntityOnPanic {
                entity: uninit_entity.id,
                world: self.world,
                caller,
            };

            context.source = source;
            context.target = uninit_entity.id;

            let dst_id = uninit_entity.id;
            let dst_row = uninit_entity.location.table_row;
            let this_run = uninit_entity.this_run;
            let table = uninit_entity.table;

            debug_assert_eq!(table_id, uninit_entity.location.table_id);
            debug_assert_eq!(
                table.entities().get(src_row.0 as usize).copied(),
                Some(source)
            );

            let components = table.components();

            // Clone all component data
            for (index, &id) in components.iter().enumerate() {
                let table_col = TableCol(index as u32);
                debug_assert_eq!(Some(table_col), table.get_table_col(id));

                let column = unsafe { table.get_column_mut(table_col) };

                let info = unsafe {
                    let components = &self.world.read_only().components;
                    components.get_by_id(id).debug_checked_unwrap()
                };

                let name = info.typa_name;
                let type_id = info.type_id;
                let cloner = info.cloner;

                let src_index = src_row.0 as usize;
                let dst_index = dst_row.0 as usize;

                // set added and changed
                unsafe {
                    *column.get_added_mut(dst_index) = this_run;
                    *column.get_changed_mut(dst_index) = this_run;
                }

                let column_p = column as *mut Column;
                let src_ptr = unsafe { (*column_p).get_data(src_index) };
                let dst_ptr = unsafe { (*column_p).get_data_mut(dst_index).promote() };

                let src = CloneSource {
                    ptr: src_ptr,
                    name,
                    type_id,
                };

                let mut initialized = false;
                let dst = CloneTarget {
                    ptr: dst_ptr,
                    name,
                    type_id,
                    initialized: &mut initialized,
                };

                cloner.call(src, dst, &mut context);

                // change to debug_assert ?
                assert!(
                    initialized,
                    "The ComponentCloner of `{name}` did not write data. {caller}"
                );
            }

            self.mapper.set_mapped(source, dst_id);
            self.cloned.push(dst_id);

            ::core::mem::forget(item_guard);

            // Collect all entities that should be linked clone.
            // Note that the input `linked_clone` is non mandatory.
            context.deferred.drain(..).for_each(|entity| {
                use crate::utils::contains_entity;
                let (x, y) = self.wait.as_slices();
                let c1 = !self.mapper.contains(entity);
                let c2 = !contains_entity(entity, x);
                let c3 = !contains_entity(entity, y);
                // let c4 = !contains_entity(entity, &self.cloned); // c4 == c1
                if c1 && c2 && c3
                /* && c4 */
                {
                    self.wait.push_back(entity);
                }
            });
        }

        // -------------------------------------------------------------------
        // Callbacks
        // -------------------------------------------------------------------

        // Run callbacks
        let callbacks = context.callback;
        let world = unsafe { self.world.full_mut() };
        for callback in callbacks {
            let Callback {
                func,
                id,
                entity,
                name,
                type_id,
            } = callback;

            // The cloning operation has not yet called the lifecycle hooks.
            // The target entity should exist.
            let mut entity_mut = world.get_entity_mut(entity).unwrap();
            // In theory, when there are fewer components, binary search (ComponentId)
            // is faster than hash (TypeId). Require tests.
            let untyped = entity_mut.get_mut_by_id(id).expect("should exist");
            let ptr = untyped.value;
            let clone_value = CloneValue { ptr, name, type_id };
            func(clone_value, &mut self.mapper);
        }

        // -------------------------------------------------------------------
        // Complete Hierarchy
        // -------------------------------------------------------------------

        let world = unsafe { self.world.full_mut() };
        for &id in self.cloned.as_slice() {
            let tree = &mut world.entities;
            let node = unsafe { tree.entities.get_unchecked_mut(id.index() as usize) };
            let child_of = node.child_of.map(|x| self.mapper.get_mapped(x));

            node.child_of = child_of;

            if let Some(p) = child_of {
                let slot = unsafe { tree.entities.get_unchecked_mut(p.index() as usize) };
                slot.children.insert(id);
            } else {
                tree.root.insert(id);
            }
        }

        // -------------------------------------------------------------------
        // Component Hooks
        // -------------------------------------------------------------------

        // Run Lifetime Hooks
        let world = unsafe { self.world.full_mut() };
        for &entity in self.cloned.as_slice() {
            if let Ok(location) = world.entities.locate(entity) {
                let table_id = location.table_id;
                let table = unsafe { world.tables.get_unchecked(table_id) };
                let mut deferred = unsafe { self.world.deferred() };

                table.trigger_on_clone(entity, deferred.reborrow(), caller);
                table.trigger_on_add(entity, deferred.reborrow(), caller);
                table.trigger_on_insert(entity, deferred.reborrow(), caller);

                todo!("World::flush")
            }
        }

        ::core::mem::forget(forget_guard);

        // -------------------------------------------------------------------
        // Return & Clear
        // -------------------------------------------------------------------

        // Map output
        for item in output.iter_mut() {
            *item = self.mapper.get_mapped(*item);
        }

        self.mapper.clear();
        self.cloned.clear();
        self.wait.clear();

        output
    }
}
