//! Entity despawn methods implemented on `World`.

use zlim_utils::debug::DebugLocation;
use zlim_utils::vec::{FastVec, FastVecData};

use crate::entity::{EntityError, EntityId};
use crate::utils::{DebugCheckedUnwrap, ForgetEntityOnPanic};
use crate::world::{DeferredWorld, World, WorldCell};

// -----------------------------------------------------------------------------
// despawn
// -----------------------------------------------------------------------------

impl World {
    /// Despawns an entity, recursively despawning all of its descendants.
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
    /// let mut world = World::alloc();
    ///
    /// // Spawn a small hierarchy: root -> child.
    /// let root = world.spawn((), None).id();
    /// let child = world.spawn((), Some(root)).id();
    ///
    /// // Despawning the root recursively despawns its descendants.
    /// world.despawn(root).unwrap();
    /// assert!(world.get_entity_owned(child).is_err());
    /// ```
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn despawn(&mut self, entity: EntityId) -> Result<(), EntityError> {
        let caller = DebugLocation::caller();
        self.despawn_with_caller(entity, caller)
    }

    /// Despawns an entity, recursively despawning all of its descendants.
    ///
    /// Returns `false` (and does nothing) if the entity is not spawned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let id = world.spawn((), None).id();
    ///
    /// // Returns `true` when the entity was actually despawned...
    /// assert!(world.try_despawn(id));
    ///
    /// // ...and `false` when it was already gone.
    /// assert!(!world.try_despawn(id));
    /// ```
    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn try_despawn(&mut self, entity: EntityId) -> bool {
        let caller = DebugLocation::caller();
        self.try_despawn_with_caller(entity, caller)
    }

    #[inline(always)]
    pub(crate) fn despawn_with_caller(
        &mut self,
        entity: EntityId,
        caller: DebugLocation,
    ) -> Result<(), EntityError> {
        let _ = self.entities.locate(entity)?;
        despawn_internal(self, entity, caller);
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn try_despawn_with_caller(
        &mut self,
        entity: EntityId,
        caller: DebugLocation,
    ) -> bool {
        if !self.entities.contains(entity) {
            return false;
        }

        despawn_internal(self, entity, caller);
        true
    }
}

// -----------------------------------------------------------------------------
// Internal
// -----------------------------------------------------------------------------

#[inline(never)]
pub(crate) fn despawn_internal(this: &mut World, entity: EntityId, caller: DebugLocation) {
    let cell = this.cell();

    let guard = ForgetEntityOnPanic {
        entity,
        caller,
        world: cell,
    };

    let world = unsafe { cell.full_mut() };

    fn dfs_trigger(
        buf: &mut FastVecData<EntityId, 1>,
        world: WorldCell<'_>,
        entity: EntityId,
        caller: DebugLocation,
    ) {
        let entities = unsafe { &world.data_mut().entities };

        let node = entities.get(entity).unwrap_or_else(|e| {
            core::hint::cold_path();
            panic!("invalid entity node: {e}.\n\t{caller}");
        });

        for &child in node.children.iter() {
            dfs_trigger(buf, world, child, caller);
        }

        let location = unsafe { node.location.unwrap_unchecked() };
        let table_id = location.table_id;
        let table = unsafe { world.data_mut().tables.get_unchecked_mut(table_id) };

        buf.push(entity);

        // Trigger all hook and events
        let mut world: DeferredWorld = unsafe { world.deferred() };
        table.trigger_on_discard(entity, world.reborrow(), caller);
        table.trigger_on_remove(entity, world.reborrow(), caller);
        table.trigger_on_despawn(entity, world.reborrow(), caller);
    }

    let mut entities = FastVec::<EntityId, 1>::new();
    let buf = entities.data();

    dfs_trigger(buf, cell, entity, caller);

    for &id in buf.iter() {
        // Removes the entity from the entity tree (clearing its location and
        // re-parenting any children to the root) and returns its storage
        // location.
        let location = unsafe { world.entities.remove_one(id).debug_checked_unwrap() };
        let table_id = location.table_id;
        let table_row = location.table_row;
        let table = unsafe { world.tables.get_unchecked_mut(table_id) };
        let moved = unsafe { table.dealloc_row::<true>(table_row) };
        world.entities.update_row(moved).unwrap();

        // Advance the generation so stale handles no longer alias the slot,
        // then recycle the slot with the fresh id.
        let new_id = world.entities.free_slot(id.index());
        world.allocator.free(new_id);
    }

    ::core::mem::forget(guard);

    world.flush();
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::component::Component;
    use crate::world::World;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use serde::{Deserialize, Serialize};
    use zlim_reflect::TypePath;

    #[derive(TypePath, Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
    struct Foo;

    #[derive(TypePath, Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
    struct Bar(u64);

    #[derive(TypePath, Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
    struct Baz(String);

    #[test]
    fn drop_entity() {
        static DROP_COUNTER: AtomicUsize = AtomicUsize::new(0);

        #[derive(TypePath, Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
        struct DropTracker;

        impl Drop for DropTracker {
            fn drop(&mut self) {
                DROP_COUNTER.fetch_add(1, Ordering::SeqCst);
            }
        }

        let mut world = World::alloc();

        // Single
        DROP_COUNTER.store(0, Ordering::SeqCst);
        let entity = world.spawn(DropTracker, None).id();
        DROP_COUNTER.store(0, Ordering::SeqCst);
        world.despawn(entity).unwrap();
        assert_eq!(DROP_COUNTER.load(Ordering::SeqCst), 1);

        // Combined
        DROP_COUNTER.store(0, Ordering::SeqCst);
        let entity = world
            .spawn((DropTracker, Bar(3), Baz(String::from("123"))), None)
            .id();
        world.despawn(entity).unwrap();
        assert_eq!(DROP_COUNTER.load(Ordering::SeqCst), 1);

        // Repeated
        DROP_COUNTER.store(0, Ordering::SeqCst);
        let entity = world.spawn((DropTracker, DropTracker, Foo), None).id();
        world.despawn(entity).unwrap();
        assert_eq!(DROP_COUNTER.load(Ordering::SeqCst), 2);

        // Hierarchy
        DROP_COUNTER.store(0, Ordering::SeqCst);
        let e1 = world.spawn(DropTracker, None).id();
        let e2 = world.spawn(DropTracker, Some(e1)).id();
        let _e3 = world.spawn(DropTracker, Some(e1)).id();
        let _e4 = world.spawn(DropTracker, Some(e2)).id();
        world.despawn(e1).unwrap();
        assert_eq!(DROP_COUNTER.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn drop_world() {
        static DROP_COUNTER: AtomicUsize = AtomicUsize::new(0);

        #[derive(TypePath, Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
        struct DropTracker;

        impl Drop for DropTracker {
            fn drop(&mut self) {
                DROP_COUNTER.fetch_add(1, Ordering::SeqCst);
            }
        }

        let mut world = World::alloc();
        DROP_COUNTER.store(0, Ordering::SeqCst);

        for _ in 0..100 {
            world.spawn(DropTracker, None);
            world.spawn((Foo, DropTracker), None);
        }

        ::core::mem::drop(world);

        assert_eq!(DROP_COUNTER.load(Ordering::SeqCst), 200);
    }
}

// -----------------------------------------------------------------------------
