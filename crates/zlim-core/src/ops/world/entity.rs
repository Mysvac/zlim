//! Entity view accessors and the `FetchEntities` trait.

use core::mem::MaybeUninit;

use crate::entity::{AllocEntitiesIter, EntityError, EntityId};
use crate::ops::{Entity, EntityMut, EntityOwned, EntityRef};
use crate::utils::DebugCheckedUnwrap;
use crate::world::{DeferredWorld, World, WorldCell};

/// Builds a shared [`EntityRef`] view for one entity with cached tick
/// context.
fn get_entity_ref(world: &World, entity: EntityId) -> Result<EntityRef<'_>, EntityError> {
    let location = world.entities.locate(entity)?;
    let last_run = world.last_run();
    let this_run = world.this_run();
    let table = unsafe { world.tables.get_unchecked(location.table_id) };

    Ok(EntityRef {
        id: entity,
        table,
        location,
        last_run,
        this_run,
    })
}

/// Builds a mutable [`EntityMut`] view for one entity with cached tick
/// context.
fn get_entity_mut(world: &mut World, entity: EntityId) -> Result<EntityMut<'_>, EntityError> {
    let location = world.entities.locate(entity)?;
    let last_run = world.last_run();
    let this_run = world.this_run();
    let table = unsafe { world.tables.get_unchecked_mut(location.table_id) };

    Ok(EntityMut {
        id: entity,
        table,
        location,
        last_run,
        this_run,
    })
}

/// Builds an [`EntityOwned`] handle for one entity, keeping a raw
/// [`WorldCell`] so the handle can perform direct per-entity operations.
fn get_entity_owned(world: &mut World, entity: EntityId) -> Result<EntityOwned<'_>, EntityError> {
    let world_cell = world.cell();
    let world = unsafe { world_cell.full_mut() };
    let location = world.entities.locate(entity)?;
    let table = unsafe { world.tables.get_unchecked_mut(location.table_id) };
    Ok(EntityOwned {
        id: entity,
        world: world_cell,
        storage: Some((table, location)),
    })
}

/// Builds an [`Entity`] handle for one entity, keeping its metadata node and
/// a raw [`WorldCell`] for direct per-entity operations.
fn get_entity(world: &mut World, entity: EntityId) -> Result<Entity<'_>, EntityError> {
    let world_cell = world.cell();
    let world = unsafe { world_cell.full_mut() };
    let node = world.entities.get(entity)?;
    let location = unsafe { node.location.debug_checked_unwrap() };
    let table = unsafe { world.tables.get_unchecked_mut(location.table_id) };
    Ok(Entity {
        id: entity,
        world: world_cell,
        table,
        node,
        location,
    })
}

/// Produces entity views from one or more entity identifiers.
///
/// This trait backs [`World::get_entity_ref`] and [`World::get_entity_mut`].
/// It is implemented for [`EntityId`], arrays, and slices of
/// [`EntityId`]s.
///
/// # Safety
///
/// Implementations must uphold the per-method safety contracts below.
///
/// [`World::get_entity_ref`]: crate::world::World::get_entity_ref
/// [`World::get_entity_mut`]: crate::world::World::get_entity_mut
pub unsafe trait FetchEntities {
    /// The shared view type.
    type Ref<'a>;

    /// The non-structural mutable view type.
    type Mut<'a>;

    /// # Safety
    /// - The world can be read.
    /// - Returns only read-only references.
    unsafe fn fetch_ref(this: Self, world: WorldCell<'_>) -> Result<Self::Ref<'_>, EntityError>;

    /// # Safety
    /// - The world is non-structurally-mutable.
    /// - Returns only non-structurally-mutable references.
    unsafe fn fetch_mut(this: Self, world: WorldCell<'_>) -> Result<Self::Mut<'_>, EntityError>;
}

unsafe impl FetchEntities for EntityId {
    type Ref<'a> = EntityRef<'a>;
    type Mut<'a> = EntityMut<'a>;

    #[inline]
    unsafe fn fetch_ref(this: Self, world: WorldCell<'_>) -> Result<Self::Ref<'_>, EntityError> {
        get_entity_ref(unsafe { world.read_only() }, this)
    }

    #[inline]
    unsafe fn fetch_mut(this: Self, world: WorldCell<'_>) -> Result<Self::Mut<'_>, EntityError> {
        get_entity_mut(unsafe { world.data_mut() }, this)
    }
}

unsafe impl<const N: usize> FetchEntities for &[EntityId; N] {
    type Ref<'a> = [EntityRef<'a>; N];
    type Mut<'a> = [EntityMut<'a>; N];

    unsafe fn fetch_ref(this: Self, world: WorldCell<'_>) -> Result<Self::Ref<'_>, EntityError> {
        let mut result = MaybeUninit::<[EntityRef; N]>::uninit();
        let inner = unsafe { result.assume_init_mut() };
        for (r, &e) in core::iter::zip(inner, this) {
            *r = get_entity_ref(unsafe { world.read_only() }, e)?;
        }
        Ok(unsafe { result.assume_init() })
    }

    unsafe fn fetch_mut(this: Self, world: WorldCell<'_>) -> Result<Self::Mut<'_>, EntityError> {
        let mut result = MaybeUninit::<[EntityMut; N]>::uninit();
        let inner = unsafe { result.assume_init_mut() };
        for (r, &e) in core::iter::zip(inner, this) {
            *r = get_entity_mut(unsafe { world.data_mut() }, e)?;
        }
        Ok(unsafe { result.assume_init() })
    }
}

unsafe impl<const N: usize> FetchEntities for [EntityId; N] {
    type Ref<'a> = [EntityRef<'a>; N];
    type Mut<'a> = [EntityMut<'a>; N];

    #[inline(always)]
    unsafe fn fetch_ref(this: Self, world: WorldCell<'_>) -> Result<Self::Ref<'_>, EntityError> {
        unsafe { <&Self as FetchEntities>::fetch_ref(&this, world) }
    }

    #[inline(always)]
    unsafe fn fetch_mut(this: Self, world: WorldCell<'_>) -> Result<Self::Mut<'_>, EntityError> {
        unsafe { <&Self as FetchEntities>::fetch_mut(&this, world) }
    }
}

unsafe impl FetchEntities for &[EntityId] {
    type Ref<'a> = Vec<EntityRef<'a>>;
    type Mut<'a> = Vec<EntityMut<'a>>;

    unsafe fn fetch_ref(this: Self, world: WorldCell<'_>) -> Result<Self::Ref<'_>, EntityError> {
        let mut ret = Vec::with_capacity(this.len());

        for &e in this {
            ret.push(get_entity_ref(unsafe { world.read_only() }, e)?);
        }

        Ok(ret)
    }

    unsafe fn fetch_mut(this: Self, world: WorldCell<'_>) -> Result<Self::Mut<'_>, EntityError> {
        let mut ret = Vec::with_capacity(this.len());

        for &e in this {
            ret.push(get_entity_mut(unsafe { world.data_mut() }, e)?);
        }

        Ok(ret)
    }
}

impl World {
    /// Reserves a fresh entity ID from the lock-free allocator.
    ///
    /// This is a low-level operation that only allocates an ID — it does not
    /// register the entity in [`World::entities`] storage.  Prefer
    /// `spawn`/`insert` unless you need to control ID allocation manually.
    ///
    /// [`World::entities`]: crate::world::World::entities
    #[inline]
    #[must_use]
    pub fn alloc_entity(&self) -> EntityId {
        self.allocator.alloc()
    }

    /// Reserves `count` fresh entity IDs in a single batch.
    ///
    /// # Panics
    ///
    /// Panics if `count` is greater than or equal to `u32::MAX`.
    #[inline]
    #[must_use]
    pub fn alloc_entities(&self, count: usize) -> AllocEntitiesIter<'_> {
        assert!(count < u32::MAX as usize, "too many entities");
        self.allocator.alloc_many(count as u32)
    }

    /// Returns shared entity view(s) for the given entity ID(s).
    ///
    /// `E` may be a single [`EntityId`], an array, or a slice of
    /// [`EntityId`]s, producing a matching set of [`EntityRef`] views with
    /// cached tick context.
    ///
    /// Returns `Err(EntityError)` if any entity is not spawned or does not
    /// exist.
    #[inline]
    pub fn get_entity_ref<E: FetchEntities>(&self, entities: E) -> Result<E::Ref<'_>, EntityError> {
        unsafe { E::fetch_ref(entities, self.cell()) }
    }

    /// Returns mutable entity view(s) for the given entity ID(s).
    ///
    /// `E` may be a single [`EntityId`], an array, or a slice of
    /// [`EntityId`]s, producing a matching set of [`EntityMut`] views with
    /// cached tick context.
    ///
    /// Returns `Err(EntityError)` if any entity is not spawned or does not
    /// exist.
    #[inline]
    pub fn get_entity_mut<E: FetchEntities>(
        &mut self,
        entities: E,
    ) -> Result<E::Mut<'_>, EntityError> {
        unsafe { E::fetch_mut(entities, self.cell()) }
    }

    /// Returns an [`EntityOwned`] handle for one entity.
    ///
    /// The handle keeps raw world access for direct per-entity operations
    /// (spawning, inserting/removing components).
    ///
    /// Returns `Err(EntityError)` if the entity is not spawned or does not
    /// exist.
    #[inline]
    pub fn get_entity_owned(&mut self, entity: EntityId) -> Result<EntityOwned<'_>, EntityError> {
        get_entity_owned(self, entity)
    }

    /// Returns an [`Entity`] handle for one entity.
    ///
    /// The handle keeps the entity's metadata node and raw world access for
    /// direct per-entity operations.
    ///
    /// Returns `Err(EntityError)` if the entity is not spawned or does not
    /// exist.
    #[inline]
    pub fn get_entity(&mut self, entity: EntityId) -> Result<Entity<'_>, EntityError> {
        get_entity(self, entity)
    }

    /// Returns a shared entity view with cached tick context.
    ///
    /// Convenience wrapper around [`World::get_entity_ref`] that panics on
    /// failure.
    ///
    /// # Panics
    ///
    /// Panics if the entity is not spawned or does not exist.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let ids = [world.spawn((), None).id(), world.spawn((), None).id()];
    ///
    /// // `FetchEntities` accepts a single ID, an array, or a slice,
    /// // producing a matching set of views.
    /// let [a, b] = world.entity_ref(ids);
    /// assert_eq!(a.id(), ids[0]);
    /// assert_eq!(b.id(), ids[1]);
    ///
    /// let single = world.entity_ref(ids[0]);
    /// assert_eq!(single.id(), ids[0]);
    /// ```
    #[inline]
    pub fn entity_ref<E: FetchEntities>(&self, entities: E) -> E::Ref<'_> {
        self.get_entity_ref::<E>(entities).unwrap()
    }

    /// Returns a mutable entity view with cached tick context.
    ///
    /// Convenience wrapper around [`World::get_entity_mut`] that panics on
    /// failure.
    ///
    /// # Panics
    ///
    /// Panics if the entity is not spawned or does not exist.
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
    /// let mut world = World::alloc();
    /// let id = world.spawn(Hp(100), None).id();
    ///
    /// let mut view = world.entity_mut(id);
    /// *view.get_mut::<Hp>().unwrap().into_inner() = Hp(50);
    /// assert_eq!(view.get::<Hp>(), Some(&Hp(50)));
    /// ```
    #[inline]
    pub fn entity_mut<E: FetchEntities>(&mut self, entities: E) -> E::Mut<'_> {
        self.get_entity_mut::<E>(entities).unwrap()
    }

    /// Returns an [`EntityOwned`] handle for one entity.
    ///
    /// Convenience wrapper around [`World::get_entity_owned`] that panics on
    /// failure.
    ///
    /// # Panics
    ///
    /// Panics if the entity is not spawned or does not exist.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let id = world.spawn((), None).id();
    ///
    /// // Panics on failure; use `get_entity_owned` for the fallible variant.
    /// let mut handle = world.entity_owned(id);
    /// assert!(handle.is_spawned());
    /// ```
    #[inline]
    pub fn entity_owned(&mut self, entities: EntityId) -> EntityOwned<'_> {
        self.get_entity_owned(entities).unwrap()
    }

    /// Returns an [`Entity`] handle for one entity.
    ///
    /// Convenience wrapper around [`World::get_entity`] that panics on
    /// failure.
    ///
    /// # Panics
    ///
    /// Panics if the entity is not spawned or does not exist.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let id = world.spawn((), None).id();
    ///
    /// let view: Entity<'_> = world.entity(id);
    /// assert_eq!(view.id(), id);
    /// ```
    #[inline]
    pub fn entity(&mut self, entities: EntityId) -> Entity<'_> {
        self.get_entity(entities).unwrap()
    }
}

impl DeferredWorld<'_> {
    /// Returns shared entity view(s) for the given entity ID(s).
    ///
    /// `E` may be a single [`EntityId`], an array, or a slice of
    /// [`EntityId`]s, producing a matching set of [`EntityRef`] views with
    /// cached tick context.
    ///
    /// Returns `Err(EntityError)` if any entity is not spawned or does not
    /// exist.
    #[inline]
    pub fn get_entity_ref<E: FetchEntities>(&self, entities: E) -> Result<E::Ref<'_>, EntityError> {
        unsafe { E::fetch_ref(entities, self.cell()) }
    }

    /// Returns mutable entity view(s) for the given entity ID(s).
    ///
    /// `E` may be a single [`EntityId`], an array, or a slice of
    /// [`EntityId`]s, producing a matching set of [`EntityMut`] views with
    /// cached tick context.
    ///
    /// Returns `Err(EntityError)` if any entity is not spawned or does not
    /// exist.
    #[inline]
    pub fn get_entity_mut<E: FetchEntities>(
        &mut self,
        entities: E,
    ) -> Result<E::Mut<'_>, EntityError> {
        unsafe { E::fetch_mut(entities, self.cell()) }
    }

    /// Returns an [`Entity`] handle for one entity.
    ///
    /// The handle keeps the entity's metadata node and raw world access for
    /// direct per-entity operations.
    ///
    /// Returns `Err(EntityError)` if the entity is not spawned or does not
    /// exist.
    #[inline]
    pub fn get_entity(&mut self, entity: EntityId) -> Result<Entity<'_>, EntityError> {
        get_entity(unsafe { self.cell().data_mut() }, entity)
    }

    /// Returns a shared entity view with cached tick context.
    ///
    /// Convenience wrapper around [`DeferredWorld::get_entity_ref`] that
    /// panics on failure.
    ///
    /// # Panics
    ///
    /// Panics if the entity is not spawned or does not exist.
    #[inline]
    pub fn entity_ref<E: FetchEntities>(&self, entities: E) -> E::Ref<'_> {
        self.get_entity_ref::<E>(entities).unwrap()
    }

    /// Returns a mutable entity view with cached tick context.
    ///
    /// Convenience wrapper around [`DeferredWorld::get_entity_mut`] that
    /// panics on failure.
    ///
    /// # Panics
    ///
    /// Panics if the entity is not spawned or does not exist.
    #[inline]
    pub fn entity_mut<E: FetchEntities>(&mut self, entities: E) -> E::Mut<'_> {
        self.get_entity_mut::<E>(entities).unwrap()
    }

    /// Returns an [`Entity`] handle for one entity.
    ///
    /// Convenience wrapper around [`DeferredWorld::get_entity`] that panics
    /// on failure.
    ///
    /// # Panics
    ///
    /// Panics if the entity is not spawned or does not exist.
    #[inline]
    pub fn entity(&mut self, entities: EntityId) -> Entity<'_> {
        self.get_entity(entities).unwrap()
    }
}
