//! Entity spawning methods and the batch-spawn iterator.

use core::iter::FusedIterator;
use core::ptr::NonNull;

use zlim_ptr::OwningPtr;
use zlim_utils::debug::DebugLocation;

use crate::bundle::{Bundle, BundleId, DataBundle};
use crate::component::ComponentWriter;
use crate::entity::{AllocEntitiesIter, EntityId, Location};
use crate::ops::EntityOwned;
use crate::table::Table;
use crate::utils::ForgetEntityOnPanic;
use crate::world::{DeferredWorld, World, WorldCell};

// -----------------------------------------------------------------------------
// BundleSpawner
// -----------------------------------------------------------------------------

type WriteFunc = unsafe fn(OwningPtr<'_>, &mut ComponentWriter);
type RequiredWriteFunc = unsafe fn(&mut ComponentWriter);

struct BundleSpawner<'a> {
    world: WorldCell<'a>,
    table: NonNull<Table>,
    writer: WriteFunc,
    required_writer: RequiredWriteFunc,
    caller: DebugLocation,
}

impl<'a> BundleSpawner<'a> {
    #[inline(never)] // reduce compilation overload
    fn new(
        world: &'a mut World,
        bundle: BundleId,
        writer: WriteFunc,
        required_writer: RequiredWriteFunc,
        caller: DebugLocation,
    ) -> BundleSpawner<'a> {
        let table_id = world
            .tables
            .register(bundle, &world.bundles, &world.components);

        let table = unsafe { world.tables.get_unchecked_mut(table_id) };

        BundleSpawner {
            table: table.into(),
            world: world.into(),
            writer,
            required_writer,
            caller,
        }
    }

    #[inline]
    fn alloc(&mut self) -> EntityId {
        unsafe { self.world.full_mut().allocator.alloc_mut() }
    }

    #[inline] // `allocator.alloc_many` is `#[inline(never)]`
    fn alloc_many(&mut self, count: u32) -> AllocEntitiesIter<'a> {
        unsafe { self.world.full_mut().allocator.alloc_many(count) }
    }

    #[inline(never)] // reduce compilation overload
    fn spawn_at(
        &mut self,
        data: OwningPtr<'_>,
        entity: EntityId,
        parent: Option<EntityId>,
    ) -> (&'a mut Table, Location) {
        let world_cell = self.world;
        let world = unsafe { world_cell.full_mut() };

        #[cfg(debug_assertions)]
        if let Err(e) = world.entities.check_spawnable(entity) {
            ::core::hint::cold_path();
            panic!("entity {entity} cannot spawned: {e}.\n\t{}", self.caller)
        }

        let table = unsafe { self.table.as_mut() };

        let guard = ForgetEntityOnPanic {
            entity,
            world: self.world,
            caller: self.caller,
        };

        let tick = world.this_run_fast();
        let table_id = table.id();
        let table_row = unsafe { table.alloc_row(entity) };

        unsafe {
            let mut writer = ComponentWriter::from_table(table, table_row, tick);
            (self.writer)(data, &mut writer);
            (self.required_writer)(&mut writer);
        }

        let location = Location {
            table_id,
            table_row,
        };

        if let Err(e) = world.entities.insert_one(entity, parent, location) {
            ::core::hint::cold_path();
            let l = self.caller;
            panic!("parent `{parent:?}` is invalid: {e}.\n\t{l}");
        }

        ::core::mem::forget(guard);

        {
            let mut world: DeferredWorld = unsafe { world_cell.deferred() };
            table.trigger_on_add(entity, world.reborrow(), self.caller);
            table.trigger_on_insert(entity, world.reborrow(), self.caller);
        }

        (table, location)
    }

    #[inline(never)] // reduce compilation overload
    fn spawn_at_flush(
        &mut self,
        data: OwningPtr<'_>,
        entity: EntityId,
        parent: Option<EntityId>,
    ) -> Option<(&'a mut Table, Location)> {
        let mut storage = Some(self.spawn_at(data, entity, parent));

        let cell = self.world;
        let world = unsafe { cell.full_mut() };

        if !world.command_queue.is_empty() {
            unsafe {
                world.flush();
                storage = cell.full_mut().entities.locate(entity).ok().map(|x| {
                    let tables = &mut cell.full_mut().tables;
                    (tables.get_unchecked_mut(x.table_id), x)
                });
            }
        }

        storage
    }
}

// -----------------------------------------------------------------------------
// World spawn

impl World {
    /// Spawns a new entity and returns an owned handle to it.
    ///
    /// # Panics
    /// - Panics if `parent` is `Some` but the target entity is not spawned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::derive::Component;
    ///
    /// #[derive(TypePath, Component, Clone, PartialEq, Debug)]
    /// struct Health(u32);
    ///
    /// let mut world = World::alloc();
    ///
    /// // A root entity with a single component.
    /// let mut hero = world.spawn(Health(100), None);
    /// assert_eq!(hero.get::<Health>(), Some(&Health(100)));
    ///
    /// // Passing a `parent` links the new entity into the hierarchy.
    /// let hero_id = hero.id();
    /// drop(hero); // release the world borrow before spawning again
    /// let sidekick_id = world.spawn(Health(50), Some(hero_id)).id();
    ///
    /// let mut hero = world.entity_owned(hero_id);
    /// assert!(hero.as_view().children().any(|id| id == sidekick_id));
    /// ```
    #[inline(always)] // We enable inlining to avoid copying data
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn spawn<B: Bundle>(&mut self, bundle: B, parent: Option<EntityId>) -> EntityOwned<'_> {
        self.spawn_with_caller(bundle, parent, DebugLocation::caller())
    }

    /// Spawns a new entity at given `id` and returns an owned handle to it.
    ///
    /// # Panics
    /// - Panics if given `id` cannot spawn (e.g. already spawned).
    /// - Panics if `parent` is `Some` but the target entity is not spawned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    ///
    /// // Reserve an ID first, then spawn the entity at that exact ID.
    /// let id = world.alloc_entity();
    /// let entity = world.spawn_at((), id, None);
    /// assert_eq!(entity.id(), id);
    /// ```
    #[inline(always)] // We enable inlining to avoid copying data
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn spawn_at<B: Bundle>(
        &mut self,
        bundle: B,
        entity: EntityId,
        parent: Option<EntityId>,
    ) -> EntityOwned<'_> {
        self.spawn_at_with_caller(bundle, entity, parent, DebugLocation::caller())
    }

    #[inline] // We enable inlining to avoid copying data
    pub(crate) fn spawn_with_caller<B: Bundle>(
        &mut self,
        bundle: B,
        parent: Option<EntityId>,
        caller: DebugLocation,
    ) -> EntityOwned<'_> {
        let bundle_id = self.register_required_bundle::<B>();

        let cell = self.cell();
        let world = unsafe { cell.full_mut() };

        let mut spawner = BundleSpawner::new(
            world,
            bundle_id,
            B::write_explicit,
            B::write_required,
            caller,
        );

        let entity = spawner.alloc();

        #[cfg(debug_assertions)]
        if let Err(e) = self.entities.check_spawnable(entity) {
            ::core::hint::cold_path();
            panic!("entity {entity} cannot spawned: {e}.\n\t{caller}")
        }

        zlim_ptr::into_owning!(bundle as data);

        let mut ptr = data;
        let data = unsafe { ptr.borrow_mut().promote() };

        let storage = spawner.spawn_at_flush(data, entity, parent);

        let mut owned = EntityOwned {
            id: entity,
            storage,
            world: cell,
        };

        if B::NEED_APPLY_EFFECT {
            unsafe { B::apply_effect(ptr, &mut owned) };
        }

        owned
    }

    #[inline] // We enable inlining to avoid copying data
    pub(crate) fn spawn_at_with_caller<B: Bundle>(
        &mut self,
        bundle: B,
        entity: EntityId,
        parent: Option<EntityId>,
        caller: DebugLocation,
    ) -> EntityOwned<'_> {
        if let Err(e) = self.entities.check_spawnable(entity) {
            ::core::hint::cold_path();
            panic!("entity {entity} cannot spawned: {e}.\n\t{caller}")
        }
        let bundle_id = self.register_required_bundle::<B>();

        let cell = self.cell();
        let world = unsafe { cell.full_mut() };

        let mut spawner = BundleSpawner::new(
            world,
            bundle_id,
            B::write_explicit,
            B::write_required,
            caller,
        );

        zlim_ptr::into_owning!(bundle as data);

        let mut ptr = data;
        let data = unsafe { ptr.borrow_mut().promote() };

        let storage = spawner.spawn_at_flush(data, entity, parent);

        let mut owned = EntityOwned {
            id: entity,
            storage,
            world: cell,
        };

        if B::NEED_APPLY_EFFECT {
            unsafe { B::apply_effect(ptr, &mut owned) };
        }

        owned
    }
}

// -----------------------------------------------------------------------------
// Spawn Batch Iter
// -----------------------------------------------------------------------------

/// An iterator that spawns one entity per [`DataBundle`] produced by the
/// underlying iterator.
///
/// Returned by [`World::spawn_batch`].  If not fully consumed, the remaining
/// bundles are spawned on drop.
pub struct SpawnBatchIter<'w, I>
where
    I: Iterator,
    I::Item: DataBundle,
{
    inner: I,
    parent: Option<EntityId>,
    spawner: BundleSpawner<'w>,
    allocator: AllocEntitiesIter<'w>,
}

impl<I> Drop for SpawnBatchIter<'_, I>
where
    I: Iterator,
    I::Item: DataBundle,
{
    fn drop(&mut self) {
        self.by_ref().for_each(|_| {});

        let world = unsafe { self.spawner.world.full_mut() };

        for e in self.allocator.by_ref() {
            world.allocator.free(e);
        }

        world.flush();
    }
}

impl<I> Iterator for SpawnBatchIter<'_, I>
where
    I: Iterator,
    I::Item: DataBundle,
{
    type Item = EntityId;

    fn next(&mut self) -> Option<Self::Item> {
        let bundle = self.inner.next()?;
        let entity = self
            .allocator
            .next()
            .unwrap_or_else(|| self.spawner.alloc());

        zlim_ptr::into_owning!(bundle as data);

        self.spawner.spawn_at(data, entity, self.parent);

        Some(entity)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<I: ExactSizeIterator<Item: DataBundle>> ExactSizeIterator for SpawnBatchIter<'_, I> {}
impl<I: FusedIterator<Item: DataBundle>> FusedIterator for SpawnBatchIter<'_, I> {}

// -----------------------------------------------------------------------------
// Spawn Batch
// -----------------------------------------------------------------------------

impl World {
    /// Returns an iterator for batch spawning entities.
    ///
    /// If the iterator is not fully consumed, remaining data will
    /// be spawned during `Drop::drop`.
    ///
    /// # Panics
    /// - Panics if `parent` is `Some` but the target entity is not spawned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::derive::Component;
    ///
    /// #[derive(TypePath, Component, Clone, PartialEq, Debug)]
    /// struct X(f32);
    ///
    /// let mut world = World::alloc();
    ///
    /// // Spawn one entity per bundle, all under a common parent.
    /// let root_id = world.spawn((), None).id();
    /// let ids: Vec<EntityId> = world
    ///     .spawn_batch([X(1.0), X(2.0), X(3.0)], Some(root_id))
    ///     .collect();
    /// assert_eq!(ids.len(), 3);
    ///
    /// let mut root = world.entity_owned(root_id);
    /// assert_eq!(root.child_count(), 3);
    /// ```
    #[inline(always)] // We enable inlining to avoid copying data
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn spawn_batch<B, I>(
        &mut self,
        iter: I,
        parent: Option<EntityId>,
    ) -> SpawnBatchIter<'_, I::IntoIter>
    where
        B: DataBundle,
        I: IntoIterator<Item = B>,
    {
        self.spawn_batch_with_caller(iter, parent, DebugLocation::caller())
    }

    #[inline] // We enable inlining to avoid copying data
    pub(crate) fn spawn_batch_with_caller<B, I>(
        &mut self,
        iter: I,
        parent: Option<EntityId>,
        caller: DebugLocation,
    ) -> SpawnBatchIter<'_, I::IntoIter>
    where
        B: DataBundle,
        I: IntoIterator<Item = B>,
    {
        let bundle_id = self.register_required_bundle::<B>();

        let mut spawner = BundleSpawner::new(
            self,
            bundle_id,
            B::write_explicit,
            B::write_required,
            caller,
        );

        let inner = iter.into_iter();
        let count = inner.size_hint().0 as u32;
        let allocator = spawner.alloc_many(count);

        SpawnBatchIter {
            inner,
            parent,
            spawner,
            allocator,
        }
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::component::Component;
    use crate::world::World;
    use serde::{Deserialize, Serialize};
    use zlim_reflect::TypePath;

    #[derive(TypePath, Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
    struct Foo;

    #[derive(TypePath, Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
    struct Bar(u64);

    #[derive(TypePath, Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
    struct Baz(String);

    #[test]
    fn spawn_single() {
        let mut world = World::alloc();

        let entity = world.spawn(Foo, None);
        assert!(entity.contains::<Foo>());
        assert!(!entity.contains::<Bar>());

        let entity = world.spawn(Bar(123), None);
        assert_eq!(entity.get::<Bar>(), Some(&Bar(123)));
        assert!(entity.get::<Foo>().is_none());

        let entity = world.spawn(Baz(String::from("hello")), None);
        assert_eq!(entity.get::<Baz>(), Some(&Baz(String::from("hello"))));
        assert!(entity.get::<Foo>().is_none());
    }

    #[test]
    fn spawn_combined() {
        let mut world = World::alloc();

        let entity = world.spawn((Foo, Bar(123), Baz(String::from("hello"))), None);
        assert_eq!(entity.get::<Foo>().unwrap(), &Foo);
        assert_eq!(entity.get::<Bar>().unwrap(), &Bar(123));
        assert_eq!(entity.get::<Baz>().unwrap(), &Baz(String::from("hello")));

        // Repeat again to ensure that the access does not change the data.
        assert_eq!(entity.get::<Foo>().unwrap(), &Foo);
        assert_eq!(entity.get::<Bar>().unwrap(), &Bar(123));
        assert_eq!(entity.get::<Baz>().unwrap(), &Baz(String::from("hello")));
    }
}

// -----------------------------------------------------------------------------
