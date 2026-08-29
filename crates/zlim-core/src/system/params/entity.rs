use core::fmt::{Debug, Formatter};

use crate::entity::{EntityError, EntityId, EntityNode, RootEntities};
use crate::system::AccessTable;
use crate::system::{SystemParam, SystemParamError};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------
// RootEntities

unsafe impl SystemParam for RootEntities<'_> {
    type State = ();
    type Item<'world, 'state> = RootEntities<'world>;

    const DEFERRED: bool = false;
    const NON_SEND: bool = false;
    const EXCLUSIVE: bool = false;

    #[inline(always)]
    fn init_state(_: &World) -> Self::State {}

    fn register_access(_: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        let ok = !matches!(table, AccessTable::WorldMut);
        if ok && strict {
            table.log_error(
                "`RootEntities` system param should not be used with exclusive world \
                access. Entity operation may cause internal iterators to become invalid.",
            );
        }
        ok
    }

    #[inline]
    unsafe fn build_param<'w, 's>(
        _state: &'s mut Self::State,
        world: WorldCell<'w>,
        _last_run: Tick,
        _this_run: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        Ok(unsafe { world.read_only().entities.root_entities() })
    }
}

// -----------------------------------------------------------------------------
// HierarchyQuery

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct HierarchyQuery<'w>(&'w [EntityNode]);

impl Debug for HierarchyQuery<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.pad("HierarchyQuery { .. }")
    }
}

unsafe impl SystemParam for HierarchyQuery<'_> {
    type State = ();
    type Item<'world, 'state> = HierarchyQuery<'world>;

    const DEFERRED: bool = false;
    const NON_SEND: bool = false;
    const EXCLUSIVE: bool = false;

    #[inline(always)]
    fn init_state(_: &World) -> Self::State {}

    fn register_access(_: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        let ok = !matches!(table, AccessTable::WorldMut);
        if ok && strict {
            table.log_error(
                "`HierarchyQuery` system param should not be used with exclusive world \
                access. Entity operation may cause internal reference to become invalid.",
            );
        }
        ok
    }

    #[inline]
    unsafe fn build_param<'w, 's>(
        _state: &'s mut Self::State,
        world: WorldCell<'w>,
        _last_run: Tick,
        _this_run: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        Ok(unsafe { HierarchyQuery(&world.read_only().entities.entities) })
    }
}

impl<'w> HierarchyQuery<'w> {
    /// Returns the parent of the given entity, if any.
    ///
    /// - `Ok(Some(parent))` — the entity has a parent.
    /// - `Ok(None)` — the entity exists but is a root entity (has no parent).
    /// - `Err(EntityError)` — the entity does not exist.
    #[inline]
    pub fn get_parent(self, id: EntityId) -> Result<Option<EntityId>, EntityError> {
        let Some(info) = self.0.get(id.index() as usize) else {
            core::hint::cold_path();
            return Err(EntityError::NotFound(id.index()));
        };

        if info.generation != id.generation() {
            core::hint::cold_path();
            let generation = info.generation;
            let expect = id;
            let actual = EntityId::new(id.index(), generation);
            return Err(EntityError::Mismatch { expect, actual });
        }

        if info.location.is_none() {
            ::core::hint::cold_path();
            return Err(EntityError::NotSpawned(id));
        }

        Ok(info.parent)
    }

    /// Returns a slice of all children of the given entity.
    ///
    /// - `Ok(slice)` — the entity exists. The slice is empty if it has no children.
    /// - `Err(EntityError)` — the entity does not exist.
    #[inline]
    pub fn get_children(self, id: EntityId) -> Result<&'w [EntityId], EntityError> {
        let Some(info) = self.0.get(id.index() as usize) else {
            core::hint::cold_path();
            return Err(EntityError::NotFound(id.index()));
        };

        if info.generation != id.generation() {
            core::hint::cold_path();
            let generation = info.generation;
            let expect = id;
            let actual = EntityId::new(id.index(), generation);
            return Err(EntityError::Mismatch { expect, actual });
        }

        if info.location.is_none() {
            ::core::hint::cold_path();
            return Err(EntityError::NotSpawned(id));
        }

        Ok(info.children.as_slice())
    }

    /// Returns the parent of the given entity, skipping generation and spawn-state validation.
    ///
    /// This method does **not** verify that:
    /// - The entity's generation matches the provided `id`.
    /// - The entity has been spawned.
    ///
    /// It only checks that the index is within bounds of the internal storage.
    ///
    /// # Errors
    ///
    /// - `Err(EntityError::NotFound)` — the entity index is out of bounds.
    #[inline]
    pub fn get_parent_weak(self, id: EntityId) -> Result<Option<EntityId>, EntityError> {
        let Some(info) = self.0.get(id.index() as usize) else {
            core::hint::cold_path();
            return Err(EntityError::NotFound(id.index()));
        };

        Ok(info.parent)
    }

    /// Returns a slice of all children of the given entity, skipping generation and spawn-state validation.
    ///
    /// This method does **not** verify that:
    /// - The entity's generation matches the provided `id`.
    /// - The entity has been spawned.
    ///
    /// It only checks that the index is within bounds of the internal storage.
    ///
    /// # Errors
    ///
    /// - `Err(EntityError::NotFound)` — the entity index is out of bounds.
    #[inline]
    pub fn get_children_weak(self, id: EntityId) -> Result<&'w [EntityId], EntityError> {
        let Some(info) = self.0.get(id.index() as usize) else {
            core::hint::cold_path();
            return Err(EntityError::NotFound(id.index()));
        };

        Ok(info.children.as_slice())
    }

    /// Returns the parent of the given entity without performing any bounds,
    /// generation, or spawn-state checks.
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// - `id.index()` is a valid index into the internal storage.
    ///
    /// Violating any of these conditions may result in reading uninitialized or stale memory,
    /// returning a semantically invalid `EntityId`, or causing undefined behavior.
    ///
    /// This method is intended for use in hot loops where all preconditions have been
    /// established by prior validation or by the program's invariants.
    #[inline]
    pub unsafe fn get_parent_unchecked(self, id: EntityId) -> Option<EntityId> {
        debug_assert!((id.index() as usize) < self.0.len());
        unsafe { self.0.get_unchecked(id.index() as usize).parent }
    }

    /// Returns a slice of all children of the given entity without performing any bounds,
    /// generation, or spawn-state checks.
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// - `id.index()` is a valid index into the internal storage.
    ///
    /// Violating any of these conditions may result in reading invalid memory,
    /// observing partially mutated data, or causing undefined behavior.
    ///
    /// This method is intended for use in hot loops where all preconditions have been
    /// established by prior validation or by the program's invariants.
    #[inline]
    pub unsafe fn get_children_unchecked(self, id: EntityId) -> &'w [EntityId] {
        debug_assert!((id.index() as usize) < self.0.len());
        unsafe {
            self.0
                .get_unchecked(id.index() as usize)
                .children
                .as_slice()
        }
    }
}
