//! Entity cloning.
//!
//! This module implements the machinery for cloning entities:
//!
//! - [`ComponentCloner`] — per-component-type clone strategy (byte copy,
//!   [`Clone`], or fully custom).
//! - [`CloneSource`] / [`CloneTarget`] / [`CloneValue`] — type-erased views
//!   handed to cloners for reading, writing, and deferred mutation.
//! - [`CloneContext`] — per-step state plus deferred remap/mutate callbacks.
//! - [`EntityCloner`] — the high-level entry point driving a full clone run.

use core::any::TypeId;
use core::ptr::NonNull;
use std::collections::VecDeque;

use zlim_ptr::{OwningPtr, Ptr, PtrMut};
use zlim_utils::debug::{DebugLocation, DebugName};
use zlim_utils::vec::SmallVec;

use crate::component::Component;
use crate::component::ComponentId;
use crate::entity::{EntityId, EntityMap, EntityMapper};
use crate::table::{Column, TableCol};
use crate::utils::DebugCheckedUnwrap;
use crate::utils::ForgetEntityOnPanic;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------
// CloneSource & CloneTarget & CloneValue
// -----------------------------------------------------------------------------

/// Type-erased read-only view of one source component value.
///
/// Wraps a shared pointer to the source component together with its
/// [`TypeId`], and is primarily used by custom component cloners to read the
/// value being cloned via [`Self::read`].
///
/// [`TypeId`]: core::any::TypeId
pub struct CloneSource<'a> {
    ptr: Ptr<'a>,
    type_id: TypeId,
    #[cfg(any(debug_assertions, feature = "debug"))]
    name: &'static str,
}

/// Type-erased write target for one cloned component value.
///
/// Wraps an owning pointer to the destination slot together with its
/// [`TypeId`]. The underlying slot may start as uninitialized memory and must
/// be initialized by the active cloner before returning, either via
/// [`Self::write`] or — when writing through the raw pointer directly — via
/// [`Self::assume_initialized`].
///
/// [`TypeId`]: core::any::TypeId
pub struct CloneTarget<'a> {
    ptr: OwningPtr<'a>,
    type_id: TypeId,
    initialized: &'a mut bool,
    #[cfg(any(debug_assertions, feature = "debug"))]
    name: &'static str,
}

/// Type-erased mutable view used in deferred clone callbacks.
///
/// This is used after plain cloning, when the source-to-target
/// [`EntityMap`] is fully available, to remap embedded entity references or
/// apply custom post-processing via [`Self::mutate`].
///
/// [`EntityMap`]: crate::entity::EntityMap
pub struct CloneValue<'a> {
    ptr: PtrMut<'a>,
    type_id: TypeId,
    #[cfg(any(debug_assertions, feature = "debug"))]
    name: &'static str,
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
            #[cfg(any(debug_assertions, feature = "debug"))]
            Self::invalid_type(self.name, DebugName::type_name::<C>());
            #[cfg(not(any(debug_assertions, feature = "debug")))]
            Self::invalid_type("__unknown__", DebugName::type_name::<C>());
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
            #[cfg(any(debug_assertions, feature = "debug"))]
            Self::invalid_type(self.name, DebugName::type_name::<C>());
            #[cfg(not(any(debug_assertions, feature = "debug")))]
            Self::invalid_type("__unknown__", DebugName::type_name::<C>());
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
    fn invalid_type(actual: &'static str, read: DebugName) -> ! {
        panic!("CloneValue Error: try modify value as `{read}`, but the actual type is `{actual}`.")
    }

    /// Verifies that this value has type `C`.
    ///
    /// Panics if the type does not match.
    #[inline(always)]
    pub fn assert_type<C: 'static>(&self) {
        if self.type_id != TypeId::of::<C>() {
            #[cfg(any(debug_assertions, feature = "debug"))]
            Self::invalid_type(self.name, DebugName::type_name::<C>());
            #[cfg(not(any(debug_assertions, feature = "debug")))]
            Self::invalid_type("__unknown__", DebugName::type_name::<C>());
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

/// Type alias for the entity-source-to-target mapping used during cloning.
///
/// Maps each source [`EntityId`] to its cloned counterpart. The mapping is
/// built incrementally as entities are cloned and is consumed by deferred
/// callbacks (e.g., [`Component::map_entities`]) to remap embedded entity
/// references.
///
/// [`Component::map_entities`]: crate::component::Component::map_entities
pub type CloneEntityMapper = EntityMap<EntityId>;

struct Callback {
    func: Box<dyn FnOnce(CloneValue, &mut CloneEntityMapper)>,
    id: ComponentId,
    entity: EntityId,
    type_id: TypeId,
    #[cfg(any(debug_assertions, feature = "debug"))]
    name: &'static str,
}

/// Per-component context passed through a clone run.
///
/// Carries the [`ComponentId`] and the source/target entities of the current
/// clone step, and collects deferred operations for entity remapping and
/// post-clone mutation. A single context is reused across every component of
/// every entity in one clone run.
///
/// [`ComponentId`]: crate::component::ComponentId
pub struct CloneContext {
    recursive: bool,
    id: ComponentId,
    type_id: TypeId,
    source: EntityId,
    target: EntityId,
    deferred: Vec<EntityId>,
    callback: Vec<Callback>,
    #[cfg(any(debug_assertions, feature = "debug"))]
    name: &'static str,
}

// -----------------------------------------------------------------------------
// CloneContext Methods
// -----------------------------------------------------------------------------

impl CloneContext {
    #[cold]
    #[inline(never)]
    fn invalid_type(actual: &'static str, read: DebugName) -> ! {
        panic!(
            "CloneContext Error: try callback value as `{read}`, but the actual type is `{actual}`."
        )
    }

    /// Creates a new clone context.
    ///
    /// Pass `recursive = true` when children entities should be recursively
    /// cloned as part of the operation. The flag is queried by relationship
    /// cloners through [`Self::recursive`].
    pub(crate) fn new(recursive: bool) -> Self {
        Self {
            recursive,
            id: ComponentId::without_provenance(0),
            source: EntityId::PLACEHOLDER,
            target: EntityId::PLACEHOLDER,
            type_id: TypeId::of::<Self>(),
            deferred: Vec::new(),
            callback: Vec::new(),
            #[cfg(any(debug_assertions, feature = "debug"))]
            name: "__unknown__",
        }
    }

    /// Verifies that the current component type is `C`.
    ///
    /// Panics if the requested type does not match the current clone step.
    #[inline(always)]
    pub fn assert_type<C: 'static>(&self) {
        if self.type_id != TypeId::of::<C>() {
            #[cfg(any(debug_assertions, feature = "debug"))]
            Self::invalid_type(self.name, DebugName::type_name::<C>());
            #[cfg(not(any(debug_assertions, feature = "debug")))]
            Self::invalid_type("__unknown__", DebugName::type_name::<C>());
        }
    }

    /// Returns the currently cloned component id.
    pub fn id(&self) -> ComponentId {
        self.id
    }

    /// Returns whether this clone run recursively clones children entities.
    pub fn recursive(&self) -> bool {
        self.recursive
    }

    /// Returns the source entity of the current clone step.
    pub fn source_entity(&self) -> EntityId {
        self.source
    }

    /// Returns the target entity of the current clone step.
    pub fn target_entity(&self) -> EntityId {
        self.target
    }

    /// Schedules another entity to be cloned later in the same run.
    ///
    /// This is typically used by relationship-target cloners in linked mode:
    /// the target entity is enqueued and cloned once the current step
    /// finishes. Entities that are already cloned or already waiting are
    /// skipped.
    pub fn defer_clone(&mut self, entity: EntityId) {
        self.deferred.push(entity);
    }

    /// Schedules deferred entity-remapping for component type `C`.
    ///
    /// After every target entity has been allocated, the cloned component is
    /// visited with [`Component::map_entities`] so embedded [`EntityId`]
    /// references are rewritten from source to target ids. Only call this
    /// while the context is positioned on component `C` (asserted in debug
    /// builds).
    ///
    /// [`Component::map_entities`]: crate::component::Component::map_entities
    /// [`EntityId`]: crate::entity::EntityId
    pub fn defer_map_entities<C: Component>(&mut self) {
        #[cfg(debug_assertions)]
        self.assert_type::<C>();

        let wrapper = move |mut value: CloneValue, mapper: &mut CloneEntityMapper| {
            value.mutate::<C>(|c| Component::map_entities(c, mapper))
        };

        self.callback.push(Callback {
            id: self.id,
            entity: self.target,
            func: Box::new(wrapper),
            type_id: self.type_id,
            #[cfg(any(debug_assertions, feature = "debug"))]
            name: self.name,
        });
    }

    /// Schedules a custom deferred mutation for component type `C`.
    ///
    /// This is useful when cloning needs the source-to-target mapping that is
    /// only available after all target entities have been allocated.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::clone::{CloneContext, CloneSource, CloneTarget};
    /// use zlim_core::clone::ComponentCloner;
    /// use zlim_reflect::derive::TypePath;
    ///
    /// #[derive(TypePath, Component, Clone)]
    /// #[component(cloner = clone_link)]
    /// struct Link { target: EntityId }
    ///
    /// // Call inside a custom cloner, after the target slot is initialized.
    /// fn clone_link(src: CloneSource, mut dst: CloneTarget, ctx: &mut CloneContext) {
    ///     let value: &Link = src.read::<Link>();
    ///     dst.write::<Link>(value.clone());
    ///
    ///     // Instead of the default remapping pass, schedule a deferred
    ///     // mutation that rewrites the entity reference once the
    ///     // source-to-target map is complete.
    ///     ctx.defer_mutate::<Link>(|link, mapper| {
    ///         link.target = mapper.get_mapped(link.target);
    ///     });
    /// }
    ///
    /// let _ = ComponentCloner::custom(clone_link);
    ///
    /// let mut world = World::alloc();
    /// let target = world.spawn((), None).id();
    /// let src = world.spawn((Link { target },), None).id();
    ///
    /// // Cloning both entities remaps `target` to its cloned counterpart.
    /// let clones = world.entity_cloner().spawn_clone_batch(&[src, target], false);
    /// assert_eq!(
    ///     world.entity_ref(clones[0]).get::<Link>().unwrap().target,
    ///     clones[1],
    /// );
    /// ```
    pub fn defer_mutate<C: Component>(
        &mut self,
        func: impl FnOnce(&mut C, &mut CloneEntityMapper) + Send + 'static,
    ) {
        #[cfg(debug_assertions)]
        self.assert_type::<C>();

        let wrapper = move |mut value: CloneValue, mapper: &mut CloneEntityMapper| {
            value.mutate::<C>(|c| func(c, mapper))
        };

        self.callback.push(Callback {
            id: self.id,
            entity: self.target,
            func: Box::new(wrapper),
            type_id: self.type_id,
            #[cfg(any(debug_assertions, feature = "debug"))]
            name: self.name,
        });
    }
}

// -----------------------------------------------------------------------------
// ComponentCloner
// -----------------------------------------------------------------------------

/// Strategy object describing how a single component type is cloned.
///
/// Most component types should use [`Self::copyable`] or [`Self::clonable`];
/// the `#[derive(Component)]` macro picks one automatically from the
/// component's traits. Components with custom copy semantics can supply a
/// manual strategy through [`Self::custom`].
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::clone::ComponentCloner;
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Component, Clone, Copy, PartialEq, Debug)]
/// #[component(copy)]
/// struct Position { x: f32, y: f32 }
///
/// // `Copy` components clone with a byte-for-byte copy.
/// let _ = ComponentCloner::copyable::<Position>();
///
/// // The strategy is selected per component; clone an entity to see the
/// // copy in action.
/// let mut world = World::alloc();
/// let src = world.spawn((Position { x: 1.0, y: 2.0 },), None).id();
/// let dst = world.entity_cloner().spawn_clone(src, false);
/// assert_eq!(
///     world.entity_ref(dst).get::<Position>(),
///     Some(&Position { x: 1.0, y: 2.0 }),
/// );
/// ```
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct ComponentCloner {
    func: fn(src: CloneSource<'_>, dst: CloneTarget<'_>, ctx: &mut CloneContext),
}

impl ComponentCloner {
    /// Creates a cloner that performs a byte-for-byte copy for `Copy` components.
    ///
    /// The source value is copied with `ptr::copy_nonoverlapping`, which is
    /// only sound for types that are [`Copy`] (i.e. trivially duplicable).
    ///
    /// If `C::NO_ENTITY` is `false`, deferred entity remapping is queued so
    /// embedded entity references are rewritten after cloning.
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
    /// If `C::NO_ENTITY` is `false`, deferred entity remapping is queued so
    /// embedded entity references are rewritten after cloning.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::clone::ComponentCloner;
    ///
    /// #[derive(TypePath, Component, Clone, PartialEq, Debug)]
    /// struct Name(String);
    ///
    /// // Non-`Copy` components clone by calling `Clone::clone` — the
    /// // strategy `#[derive(Component)]` selects by default.
    /// let _ = ComponentCloner::clonable::<Name>();
    ///
    /// let mut world = World::alloc();
    /// let src = world.spawn((Name("Ada".into()),), None).id();
    /// let dst = world.entity_cloner().spawn_clone(src, false);
    /// assert_eq!(
    ///     world.entity_ref(dst).get::<Name>(),
    ///     Some(&Name("Ada".into())),
    /// );
    /// ```
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
    /// A custom cloner must always initialize `dst` (via
    /// [`CloneTarget::write`] or [`CloneTarget::assume_initialized`]) and
    /// should queue remapping when the component contains embedded entity
    /// references.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::clone::{CloneContext, CloneSource, CloneTarget};
    /// use zlim_core::clone::ComponentCloner;
    ///
    /// #[derive(TypePath, Component, Clone)]
    /// #[component(cloner = clone_link)]
    /// struct Link {
    ///     #[entities]
    ///     target: EntityId,
    /// }
    ///
    /// // A custom cloner must always initialize `dst` (via
    /// // `CloneTarget::write`) and should queue remapping when the
    /// // component stores entity references.
    /// fn clone_link(src: CloneSource, mut dst: CloneTarget, ctx: &mut CloneContext) {
    ///     // Read the source value and initialize the target slot.
    ///     let value: &Link = src.read::<Link>();
    ///     dst.write::<Link>(value.clone());
    ///
    ///     // `Link` stores an entity reference, so queue remapping for the
    ///     // post-clone pass.
    ///     ctx.defer_map_entities::<Link>();
    /// }
    ///
    /// // `ComponentCloner::custom` builds the strategy object; the derive
    /// // wires it into the component through `#[component(cloner = …)]`.
    /// let _ = ComponentCloner::custom(clone_link);
    ///
    /// let mut world = World::alloc();
    /// let target = world.spawn((), None).id();
    /// let src = world.spawn((Link { target },), None).id();
    ///
    /// // Cloning both entities in one run remaps `target` to its twin.
    /// let clones = world.entity_cloner().spawn_clone_batch(&[src, target], false);
    /// assert_eq!(
    ///     world.entity_ref(clones[0]).get::<Link>().unwrap().target,
    ///     clones[1],
    /// );
    /// ```
    #[inline(always)]
    pub const fn custom(func: fn(CloneSource, CloneTarget, &mut CloneContext)) -> Self {
        Self { func }
    }

    /// Invokes this cloner on one source/target pair.
    ///
    /// This is the entry point used by the engine during a clone run; custom
    /// cloners do not normally call it themselves.
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
/// Clones entities into the same world, producing fresh entities with
/// duplicated component data. Deferred entity remapping is applied after all
/// target entities have been allocated, so components holding [`EntityId`]
/// references stay consistent, and hierarchical relationships are rebuilt at
/// the end of the run.
///
/// Create this via [`World::entity_cloner`], then call
/// [`Self::spawn_clone`] or [`Self::spawn_clone_batch`].
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone, Copy, PartialEq, Debug)]
/// struct Position { x: f32, y: f32 }
///
/// let mut world = World::alloc();
/// let src = world.spawn((Position { x: 1.0, y: 2.0 },), None).id();
///
/// let mut cloner = world.entity_cloner();
/// let dst = cloner.spawn_clone(src, false);
///
/// assert_eq!(
///     world.entity_ref(dst).get::<Position>(),
///     Some(&Position { x: 1.0, y: 2.0 }),
/// );
/// ```
///
/// [`World::entity_cloner`]: crate::world::World::entity_cloner
/// [`EntityId`]: crate::entity::EntityId
pub struct EntityCloner<'w> {
    world: WorldCell<'w>,
    mapper: CloneEntityMapper,
    cloned: Vec<EntityId>,
    wait: VecDeque<EntityId>,
}

impl<'w> EntityCloner<'w> {
    /// Creates an entity cloner bound to the given world.
    ///
    /// Prefer [`World::entity_cloner`] as the usual entry point.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let src = world.spawn((), None).id();
    ///
    /// let mut cloner = EntityCloner::new(&mut world);
    /// let dst = cloner.spawn_clone(src, false);
    ///
    /// assert_ne!(src, dst);
    /// ```
    ///
    /// [`World::entity_cloner`]: crate::world::World::entity_cloner
    #[inline(always)]
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
    /// The returned vector preserves input order: each input source entity
    /// corresponds one-to-one to the cloned target entity at the same index.
    /// The order of input elements does not affect the result; hierarchical
    /// relationships are established only after all entities have been
    /// cloned.
    ///
    /// If `recursive` is `true`, children entities are recursively cloned as
    /// part of the same run.
    ///
    /// # Panics
    ///
    /// - Panics if any given entity is not spawned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// #[derive(TypePath, Component, Clone, Copy, PartialEq, Debug)]
    /// struct Position { x: f32, y: f32 }
    ///
    /// let mut world = World::alloc();
    /// let ids: Vec<EntityId> = (0..3)
    ///     .map(|i| world.spawn((Position { x: i as f32, y: 0.0 },), None).id())
    ///     .collect();
    ///
    /// let mut cloner = world.entity_cloner();
    /// let clones = cloner.spawn_clone_batch(&ids, false);
    ///
    /// assert_eq!(clones.len(), ids.len());
    /// for (i, &dst) in clones.iter().enumerate() {
    ///     assert_eq!(
    ///         world.entity_ref(dst).get::<Position>(),
    ///         Some(&Position { x: i as f32, y: 0.0 }),
    ///     );
    /// }
    /// ```
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn spawn_clone_batch(&mut self, entities: &[EntityId], recursive: bool) -> Vec<EntityId> {
        let caller = DebugLocation::caller();
        self.wait.extend(entities);
        self.run(recursive, caller).into_vec()
    }

    /// Clones one entity and returns the cloned target entity id.
    ///
    /// If `recursive` is `true`, children entities are recursively cloned as
    /// part of the same run.
    ///
    /// # Panics
    ///
    /// - Panics if the given entity is not spawned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// #[derive(TypePath, Component, Clone, Copy, PartialEq, Debug)]
    /// struct Health { value: u32 }
    ///
    /// let mut world = World::alloc();
    /// let src = world.spawn((Health { value: 100 },), None).id();
    ///
    /// let mut cloner = world.entity_cloner();
    /// let dst = cloner.spawn_clone(src, false);
    ///
    /// assert_ne!(src, dst);
    /// assert_eq!(
    ///     world.entity_ref(dst).get::<Health>(),
    ///     Some(&Health { value: 100 }),
    /// );
    /// ```
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn spawn_clone(&mut self, entity: EntityId, recursive: bool) -> EntityId {
        let caller = DebugLocation::caller();
        self.wait.push_back(entity);
        self.run(recursive, caller)[0]
    }

    #[inline]
    pub(crate) fn spawn_clone_with_caller(
        &mut self,
        entity: EntityId,
        recursive: bool,
        caller: DebugLocation,
    ) -> EntityId {
        self.wait.push_back(entity);
        self.run(recursive, caller)[0]
    }
}

// -----------------------------------------------------------------------------
// EntityCloner Implementation
// -----------------------------------------------------------------------------

impl<'w> EntityCloner<'w> {
    /// Runs one clone pass over the whole waiting queue.
    ///
    /// Returns the cloned target entities in the same order as the queued
    /// inputs. If `recursive` is `true`, the children entities are
    /// recursively cloned.
    #[inline(never)]
    fn run(&mut self, recursive: bool, caller: DebugLocation) -> SmallVec<EntityId, 2> {
        // -------------------------------------------------------------------
        // Read waits
        // -------------------------------------------------------------------

        // Store entities that are explicitly cloned.
        let mut output: SmallVec<EntityId, 2> = SmallVec::with_capacity(self.wait.len());
        let (x, y) = self.wait.as_slices();
        // ↓ Faster than `extend_from_slice` and `iter + push_unchecked`.
        unsafe {
            debug_assert_eq!(x.len() + y.len(), self.wait.len());
            let ptr_x: *mut EntityId = output.as_mut_ptr();
            let ptr_y: *mut EntityId = ptr_x.add(x.len());
            core::ptr::copy_nonoverlapping::<EntityId>(x.as_ptr(), ptr_x, x.len());
            core::ptr::copy_nonoverlapping::<EntityId>(y.as_ptr(), ptr_y, y.len());
            output.set_len(self.wait.len());
        }

        self.cloned.reserve(self.wait.len());

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
        // Start Clone
        // -------------------------------------------------------------------

        // Reusable context, reduces memory allocation.
        let mut context = CloneContext::new(recursive);

        // -------------------------------------------------------------------
        // Step-1: Plain Clone (alloc entity + clone component data)
        // -------------------------------------------------------------------

        // Clone all waiting entities.
        while let Some(source) = self.wait.pop_front() {
            let world1 = unsafe { self.world.full_mut() };
            // Obtain the Archetype Info of the source entity.
            let node = match world1.entities.get(source) {
                Ok(location) => location,
                Err(e) => {
                    core::hint::cold_path();
                    panic!("Try Clone Entity `{source}` but it is not spawned. {e}. {caller}")
                }
            };

            // We will map it after everything is completed, no need handle it here.
            let parent = node.parent;

            // SAFETY: `Entities::get` return `Err` if `location` is `None`.
            let location = unsafe { node.location.debug_checked_unwrap() };

            let table_id = location.table_id;
            let src_row = location.table_row;

            // Spawn a uninitialized entity from given Table (Archetype).
            // The component data and hierarchy relationship is uninitialized.
            let uninit_entity =
                unsafe { world1.spawn_uninit_with_caller(table_id, caller, parent) };

            // `ForgetGuard` can not forget this cloning entity. We need handle it manually.
            let item_guard = ForgetEntityOnPanic {
                entity: uninit_entity.id,
                world: self.world,
                caller,
            };

            context.source = source;
            context.target = uninit_entity.id;

            let dst_id = uninit_entity.id;
            let dst_row = uninit_entity.location.table_row;
            let table = uninit_entity.table;
            let this_run = uninit_entity.this_run;

            debug_assert_eq!(table_id, uninit_entity.location.table_id);
            debug_assert_eq!(table.entities().get(src_row.0 as usize), Some(&source));

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

                let type_id = info.type_id;
                let cloner = info.cloner;

                context.id = id;
                context.type_id = type_id;

                #[cfg(any(debug_assertions, feature = "debug"))]
                let name = {
                    context.name = info.type_name;
                    info.type_name
                };

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

                let mut init = false;
                let initialized: &mut bool = &mut init;

                #[rustfmt::skip]
                #[cfg(any(debug_assertions, feature = "debug"))]
                let (src, dst) = (
                    CloneSource { ptr: src_ptr, type_id, name },
                    CloneTarget { ptr: dst_ptr, type_id, initialized, name },
                );

                #[rustfmt::skip]
                #[cfg(not(any(debug_assertions, feature = "debug")))]
                let (src, dst) = (
                    CloneSource { ptr: src_ptr, type_id },
                    CloneTarget { ptr: dst_ptr, type_id, initialized },
                );

                cloner.call(src, dst, &mut context);

                assert!(
                    init,
                    "The Cloner of `{}` did not write data.\n{}",
                    info.type_name, caller
                );
            }

            self.mapper.set_mapped(source, dst_id);
            self.cloned.push(dst_id);

            ::core::mem::forget(item_guard);

            // Collect all entities that should be linked clone.
            //
            // The input recursive is already stored in the CloneContext;
            // however, whether additional entities are cloned is determined
            // by the component cloner, not by recursive itself.
            //
            // Therefore, we must always collect all deferred entities.
            context.deferred.drain(..).for_each(|entity| {
                use crate::utils::contains_entity;
                // Skip if the entity is already cloned or is in the waiting queue.
                let (x, y) = self.wait.as_slices();
                let c1 = !self.mapper.contains(entity); // c1: not already cloned
                let c2 = !contains_entity(entity, x);
                let c3 = !contains_entity(entity, y);
                // let c4 = !contains_entity(entity, &self.cloned); // c4 == c1
                if c1 && c2 && c3 {
                    self.wait.push_back(entity);
                }
            });
        }

        // -------------------------------------------------------------------
        // Step-2: Callbacks (map entities + custom operation)
        // -------------------------------------------------------------------

        let world = unsafe { self.world.full_mut() };

        for callback in context.callback {
            let Callback {
                func,
                id,
                entity,
                type_id,
                #[cfg(any(debug_assertions, feature = "debug"))]
                name,
            } = callback;

            // The cloning operation has not yet called the lifecycle hooks.
            // The target entity should exist.
            let mut entity_mut = world.get_entity_mut(entity).unwrap();
            // In theory, when there are fewer components, binary search (ComponentId)
            // is faster than hash (TypeId). Require tests.
            let untyped = entity_mut.get_mut_by_id(id).expect("should exist");
            let ptr = untyped.value;

            #[cfg(any(debug_assertions, feature = "debug"))]
            let clone_value = CloneValue { ptr, type_id, name };
            #[cfg(not(any(debug_assertions, feature = "debug")))]
            let clone_value = CloneValue { ptr, type_id };

            func(clone_value, &mut self.mapper);
        }

        // -------------------------------------------------------------------
        // Step-3: Complete Hierarchy Relationship
        // -------------------------------------------------------------------

        let world = unsafe { self.world.full_mut() };
        // Iterate `&[EntityId]` is faster than `Vec<EntityId>`.
        for &id in self.cloned.as_slice() {
            let index = id.index() as usize;
            let tree = &mut world.entities;
            let node = unsafe { tree.entities.get_unchecked_mut(index) };
            let parent = node.parent.map(|x| self.mapper.get_mapped(x));

            node.parent = parent;

            if let Some(p) = parent {
                let p_index = p.index() as usize;
                let slot = unsafe { tree.entities.get_unchecked_mut(p_index) };
                debug_assert!(!slot.children.contains(&id));
                slot.children.push(id);
            } else {
                tree.root.insert(id);
            }
        }

        // -------------------------------------------------------------------
        // Step-4: Run Component Hooks
        // -------------------------------------------------------------------

        let world = unsafe { self.world.full_mut() };
        // Iterate `&[EntityId]` is faster than `Vec<EntityId>`.
        for &entity in self.cloned.as_slice() {
            // The entity may be removed by other's component hook.
            // Therefore, the locating failure is acceptable.
            if let Ok(location) = world.entities.locate(entity) {
                let table_id = location.table_id;
                let table = unsafe { world.tables.get_unchecked(table_id) };
                let mut deferred = unsafe { self.world.deferred() };

                table.trigger_on_clone(entity, deferred.reborrow(), caller);
                table.trigger_on_add(entity, deferred.reborrow(), caller);
                table.trigger_on_insert(entity, deferred.reborrow(), caller);
            }
        }

        // Apply commands after component hooks.
        world.flush();

        // -------------------------------------------------------------------
        // Finish, forget guard
        // -------------------------------------------------------------------

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

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use serde::{Deserialize, Serialize};
    use zlim_reflect::TypePath;

    use crate::world::World;
    use crate::{derive::Component, entity::EntityId};

    // -------------------------------------------------------------------------
    // Test helpers
    // -------------------------------------------------------------------------

    macro_rules! define_tracker {
        ($n:ident, $t:ident) => {
            static $n: AtomicUsize = AtomicUsize::new(0);

            #[derive(Debug, TypePath, Component, Clone, Serialize, Deserialize)]
            struct $t;

            impl Drop for $t {
                fn drop(&mut self) {
                    $n.fetch_add(1, Ordering::SeqCst);
                }
            }
        };
    }

    // -------------------------------------------------------------------------
    // Test components
    // -------------------------------------------------------------------------

    #[derive(Debug, TypePath, Component, Clone, Copy)]
    #[derive(PartialEq, Eq, Serialize, Deserialize)]
    struct Pos {
        x: i32,
        y: i32,
    }

    #[derive(Debug, TypePath, Component, Clone)]
    #[derive(PartialEq, Eq, Serialize, Deserialize)]
    struct Name(String);

    #[test]
    fn clone_batch_empty_returns_empty() {
        let mut world = World::alloc();
        let outputs = world.entity_cloner().spawn_clone_batch(&[], false);
        assert!(outputs.is_empty());
    }

    #[test]
    fn clone_empty_entity() {
        let mut world = World::alloc();
        let src_id = world.spawn((), None).id();
        let dst = world.entity_cloner().spawn_clone(src_id, false);
        assert_ne!(src_id, dst);
    }

    #[test]
    fn clone_returns_different_entity() {
        let mut world = World::alloc();
        let src_id = world.spawn((Pos { x: 0, y: 0 },), None).id();
        let dst = world.entity_cloner().spawn_clone(src_id, false);
        assert_ne!(src_id, dst);
    }

    #[test]
    fn clone_copies_both_components() {
        let mut world = World::alloc();

        let alice = Name("Alice".into());
        let mut src = world.spawn((Pos { x: 1, y: 2 }, alice.clone()), None);

        let dst_id = src.clone(false).unwrap();
        let dst = world.entity_ref(dst_id);

        assert_eq!(dst.get::<Pos>(), Some(&Pos { x: 1, y: 2 }));
        assert_eq!(dst.get::<Name>(), Some(&alice));
    }

    #[test]
    fn spawn_clone_batch_preserves_order() {
        let mut world = World::alloc();
        let ids: Vec<EntityId> = (0..5)
            .map(|i| world.spawn((Pos { x: i, y: i },), None).id())
            .collect();

        let outputs = world.entity_cloner().spawn_clone_batch(&ids, false);
        assert_eq!(outputs.len(), 5);

        for (i, &dst_id) in outputs.iter().enumerate() {
            let entity = world.entity_ref(dst_id);
            assert_eq!(
                entity.get::<Pos>(),
                Some(&Pos {
                    x: i as i32,
                    y: i as i32
                })
            );
        }
    }

    #[test]
    fn dropping_world_drops_cloned_entities() {
        define_tracker!(CLONE_DROP, TrackedDrop);

        let mut world = World::alloc();

        CLONE_DROP.store(0, Ordering::SeqCst);
        let mut src = world.spawn(TrackedDrop, None);
        let _dst = src.clone(false).unwrap();
        assert_eq!(CLONE_DROP.load(Ordering::SeqCst), 0usize);
        ::core::mem::drop(world);
        assert_eq!(CLONE_DROP.load(Ordering::SeqCst), 2usize);
    }
}

// -----------------------------------------------------------------------------
