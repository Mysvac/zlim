//! Sparse entity storage and hierarchy management.
//!
//! [`Entities`] stores one [`EntityNode`] per entity index. Each node
//! tracks the entity's generation (for stale-handle detection), its
//! storage [`Location`], and its parent/children links; [`Entities`] also
//! maintains the set of root entities. Every lookup validates generation
//! and spawn state, reporting failures as [`EntityError`].

use core::fmt::{Debug, Formatter};
use core::num::NonZeroU32;
use std::collections::BTreeSet;

use zlim_core_derive::Error;
use zlim_log as log;

use super::{EntityId, Location};
use crate::table::MovedEntityRow;
use crate::utils::position_entity;

// -----------------------------------------------------------------------------
// EntityNode
// -----------------------------------------------------------------------------

/// Per-entity metadata stored in the [`Entities`].
///
/// Each slot in the tree holds one node, indexed by the entity's raw index.
/// A node is only meaningful for entities that are currently spawned;
/// otherwise its generation may still be useful for stale-handle checks.
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// let mut world = World::alloc();
/// let parent = world.spawn((), None).id();
/// let child = world.spawn((), Some(parent)).id();
///
/// let node = world.entities().get(child).unwrap();
/// assert_eq!(node.parent, Some(parent));
/// assert!(node.location.is_some());
/// ```
#[derive(Clone)]
pub struct EntityNode {
    /// The entity's current generation, used for stale-handle detection.
    pub generation: NonZeroU32,
    /// The entity's storage location, `Some` only while it is spawned.
    pub location: Option<Location>,
    /// The entity's parent, `None` for root entities.
    pub parent: Option<EntityId>,
    /// The entity's children, in insertion order.
    pub children: Vec<EntityId>,
}

const DEFAULT_NODE: EntityNode = EntityNode {
    generation: NonZeroU32::MIN,
    location: None,
    parent: None,
    children: Vec::new(),
};

const DEFAULT_REF: &EntityNode = &EntityNode {
    generation: NonZeroU32::MIN,
    location: None,
    parent: None,
    children: Vec::new(),
};

impl Debug for EntityNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut debugger = f.debug_struct("Node");
        debugger.field("generation", &self.generation);

        if let Some(location) = &self.location {
            debugger.field("location", location);

            if let Some(parent) = &self.parent {
                debugger.field("parent", parent);
            }
            if !self.children.is_empty() {
                debugger.field("children", &self.children);
            }
        }

        debugger.finish()
    }
}

// -----------------------------------------------------------------------------
// Entities
// -----------------------------------------------------------------------------

/// Sparse storage for entity metadata and parent/child relationships.
///
/// Entities are indexed by their raw [`EntityId::index`]; slots are grown
/// lazily as new indices are used. Access is normally gained through
/// [`World::entities`]; the mutating methods (spawn, despawn, re-parent)
/// are driven by the `World` and entity-op APIs.
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// let mut world = World::alloc();
/// let a = world.spawn((), None).id();
/// let b = world.spawn((), Some(a)).id();
///
/// let entities = world.entities();
/// assert!(entities.contains(a));
/// assert!(entities.try_locate(b).unwrap().is_some());
/// assert_eq!(entities.count_spawned(), 2);
/// ```
///
/// [`World::entities`]: crate::world::World::entities
pub struct Entities {
    /// The set of root entities (those without a parent).
    pub(crate) root: BTreeSet<EntityId>,
    /// Per-index entity nodes, grown on demand.
    pub(crate) entities: Vec<EntityNode>,
}

impl Debug for Entities {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let iter = self
            .entities
            .iter()
            .enumerate()
            .filter(|(_, info)| info.location.is_some());

        f.debug_map().entries(iter).finish()
    }
}

impl Entities {
    pub(crate) fn new() -> Self {
        let root: BTreeSet<EntityId> = BTreeSet::new();
        let mut entities: Vec<EntityNode> = Vec::with_capacity(256);
        let new_len = entities.capacity();
        entities.resize_with(new_len, || DEFAULT_NODE);
        Self { root, entities }
    }
}

// -----------------------------------------------------------------------------
// Private Methods

impl Entities {
    /// Reserves capacity for at least additional more elements to be inserted.
    #[cold]
    #[inline(never)]
    fn reserve(&mut self, additional: usize) {
        self.entities.reserve(additional);
        let new_len = self.entities.capacity();
        self.entities.resize_with(new_len, || DEFAULT_NODE);
    }

    /// Check the capacity to ensure that the target slot exists.
    #[inline(always)]
    fn ensure_exist(&mut self, index: u32) {
        if self.entities.len() <= index as usize {
            self.reserve(index as usize - self.entities.len() + 1);
        }
    }
}

// -----------------------------------------------------------------------------
// Locate

impl Entities {
    /// Tries to retrieve the location of a spawned entity.
    ///
    /// Time Complexity: `O(1)`
    ///
    /// # Returns
    /// - `Ok(Some(Location))` - The entity's current storage location
    /// - `Ok(None)` - The entity is not spawned but the generation matches.
    /// - `Err(EntityError)` - Generation mismatches.
    ///
    /// # Errors
    /// - `EntityError::Mismatch` - Generation counter mismatch (stale entity)
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let id = world.spawn((), None).id();
    ///
    /// // A spawned entity resolves to its storage location.
    /// assert!(world.entities().try_locate(id).unwrap().is_some());
    /// ```
    pub fn try_locate(&self, id: EntityId) -> Result<Option<Location>, EntityError> {
        let info = self.entities.get(id.index as usize).unwrap_or(DEFAULT_REF);

        if info.generation != id.generation {
            core::hint::cold_path();
            let expect = id;
            let actual = EntityId {
                index: id.index,
                generation: info.generation,
            };
            return Err(EntityError::Mismatch { expect, actual });
        }

        Ok(info.location)
    }

    /// Retrieves the location of a spawned entity.
    ///
    /// Time Complexity: `O(1)`
    ///
    /// # Returns
    /// - `Ok(Location)` - The entity's current storage location
    /// - `Err(EntityError)` - If the entity doesn't exist, generation
    ///   mismatches, or the entity is not spawned
    ///
    /// # Errors
    /// - `EntityError::NotFound` - Entity index out of bounds
    /// - `EntityError::Mismatch` - Generation counter mismatch (stale entity)
    /// - `EntityError::NotSpawned` - Entity exists but is not spawned
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let id = world.spawn((), None).id();
    ///
    /// let location = world.entities().locate(id).unwrap();
    /// assert_eq!(location.table_row.0, 0);
    /// ```
    pub fn locate(&self, id: EntityId) -> Result<Location, EntityError> {
        let Some(info) = self.entities.get(id.index as usize) else {
            core::hint::cold_path();
            return Err(EntityError::NotFound(id.index));
        };

        if info.generation != id.generation {
            core::hint::cold_path();
            let generation = info.generation;
            let expect = id;
            let actual = EntityId {
                index: id.index,
                generation,
            };
            return Err(EntityError::Mismatch { expect, actual });
        }

        info.location.ok_or(EntityError::NotSpawned(id))
    }

    /// Retrieves the `EntityNode` of a spawned entity.
    ///
    /// Time Complexity: `O(1)`
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let id = world.spawn((), None).id();
    ///
    /// let node = world.entities().get(id).unwrap();
    /// assert!(node.location.is_some());
    /// ```
    pub fn get(&self, id: EntityId) -> Result<&EntityNode, EntityError> {
        let Some(info) = self.entities.get(id.index as usize) else {
            core::hint::cold_path();
            return Err(EntityError::NotFound(id.index));
        };

        if info.generation != id.generation {
            core::hint::cold_path();
            let generation = info.generation;
            let expect = id;
            let actual = EntityId {
                index: id.index,
                generation,
            };
            return Err(EntityError::Mismatch { expect, actual });
        }

        if info.location.is_none() {
            ::core::hint::cold_path();
            return Err(EntityError::NotSpawned(id));
        }

        Ok(info)
    }

    /// Retrieves the `EntityNode` stored at the given raw index.
    ///
    /// Unlike [`get`](Self::get), this performs **no** generation or
    /// spawn-state validation — the caller must interpret the returned node
    /// accordingly (e.g. to check the slot's current generation).
    ///
    /// Time Complexity: `O(1)`
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let id = world.spawn((), None).id();
    ///
    /// let node = world.entities().get_by_index(id.index()).unwrap();
    /// assert_eq!(node.generation, id.generation());
    /// ```
    pub fn get_by_index(&self, index: u32) -> Result<&EntityNode, EntityError> {
        let Some(info) = self.entities.get(index as usize) else {
            core::hint::cold_path();
            return Err(EntityError::NotFound(index));
        };

        Ok(info)
    }

    /// Returns `true` if the entity is currently spawned.
    ///
    /// This checks both the generation and the presence of a storage location.
    ///
    /// Time Complexity: `O(1)`
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let id = world.spawn((), None).id();
    /// assert!(world.entities().contains(id));
    ///
    /// world.despawn(id).unwrap();
    /// assert!(!world.entities().contains(id));
    /// ```
    pub fn contains(&self, id: EntityId) -> bool {
        let index = id.index as usize;

        let Some(info) = self.entities.get(index) else {
            core::hint::cold_path();
            return false;
        };

        if info.generation != id.generation || info.location.is_none() {
            core::hint::cold_path();
            return false;
        }

        true
    }
}

// -----------------------------------------------------------------------------
// Checker

impl Entities {
    /// Resolves an index to its current `EntityId` with correct generation.
    ///
    /// Slots that have never been used resolve to an id with generation `1`,
    /// which never matches a recycled (already-despawned) slot.
    ///
    /// Time Complexity: `O(1)`
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let id = world.spawn((), None).id();
    ///
    /// // While the entity is live, `resolve` returns the same id...
    /// assert_eq!(world.entities().resolve(id.index()), id);
    ///
    /// // ...but after despawn the slot's generation advances.
    /// world.despawn(id).unwrap();
    /// assert_ne!(world.entities().resolve(id.index()), id);
    /// ```
    pub fn resolve(&self, index: u32) -> EntityId {
        match self.entities.get(index as usize) {
            Some(info) => EntityId {
                index,
                generation: info.generation,
            },
            None => EntityId {
                index,
                generation: NonZeroU32::MIN,
            },
        }
    }

    /// Returns the number of **spawned** entities.
    ///
    /// Time Complexity: `O(N)` — iterates every allocated slot.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// world.spawn((), None);
    /// world.spawn((), None);
    /// assert_eq!(world.entities().count_spawned(), 2);
    /// ```
    pub fn count_spawned(&self) -> usize {
        self.entities
            .iter()
            .filter(|info| info.location.is_some())
            .count()
    }

    /// Checks if an entity can be spawned.
    ///
    /// Time Complexity: `O(1)`
    ///
    /// # Returns
    /// - `Ok(())` - Entity can be spawned
    /// - `Err(EntityError)` - If spawning is not possible
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let id = world.spawn((), None).id();
    ///
    /// // A spawned entity cannot be spawned again.
    /// assert!(world.entities().check_spawnable(id).is_err());
    /// ```
    pub fn check_spawnable(&self, id: EntityId) -> Result<(), EntityError> {
        let info = self.entities.get(id.index as usize).unwrap_or(DEFAULT_REF);

        if info.location.is_some() {
            ::core::hint::cold_path();
            return Err(EntityError::AlreadySpawned(id));
        }

        if info.generation != id.generation {
            ::core::hint::cold_path();
            let generation = info.generation;
            let expect = id;
            let actual = EntityId {
                index: id.index,
                generation,
            };
            return Err(EntityError::Mismatch { expect, actual });
        }

        Ok(())
    }

    /// Applies a row movement reported by the table layer.
    ///
    /// Updates the storage location of the entity displaced by a swap-remove.
    pub fn update_row(&mut self, moved: MovedEntityRow) -> Result<(), EntityError> {
        let MovedEntityRow::Some { entity, new_row } = moved else {
            return Ok(());
        };
        let Some(info) = self.entities.get_mut(entity.index as usize) else {
            core::hint::cold_path();
            return Err(EntityError::NotFound(entity.index));
        };

        if info.generation != entity.generation {
            core::hint::cold_path();
            let expect = entity;
            let actual = EntityId {
                index: entity.index,
                generation: info.generation,
            };
            return Err(EntityError::Mismatch { expect, actual });
        }
        let Some(location) = &mut info.location else {
            core::hint::cold_path();
            return Err(EntityError::NotSpawned(entity));
        };
        location.table_row = new_row;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Move

impl Entities {
    /// Re-parents an entity, validating the new hierarchy first.
    ///
    /// Fails if the new parent (or the entity itself) is missing, mismatched,
    /// not spawned, or would create a cycle.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let root = world.spawn((), None).id();
    /// let other = world.spawn((), None).id();
    /// let child = world.spawn((), Some(root)).id();
    ///
    /// // `EntityOwned::modify_parent` drives `Entities::modify_parent`.
    /// world.entity_owned(child).modify_parent(Some(other)).unwrap();
    /// assert_eq!(world.entity(child).parent(), Some(other));
    ///
    /// // Making an entity its own parent would create a cycle.
    /// assert!(world.entity_owned(child).modify_parent(Some(child)).is_err());
    /// ```
    pub fn modify_parent(
        &mut self,
        id: EntityId,
        parent: Option<EntityId>,
    ) -> Result<(), EntityError> {
        //--------------------------------------------------------------------
        // validate new parent

        if let Some(p) = parent {
            let Some(info) = self.entities.get(p.index as usize) else {
                core::hint::cold_path();
                return Err(EntityError::NotFound(p.index));
            };
            if info.generation != p.generation {
                core::hint::cold_path();
                let generation = info.generation;
                let expect = p;
                let actual = EntityId {
                    index: p.index,
                    generation,
                };
                return Err(EntityError::Mismatch { expect, actual });
            }
            if info.location.is_none() {
                core::hint::cold_path();
                return Err(EntityError::NotSpawned(p));
            }
        }

        //--------------------------------------------------------------------
        // validate self

        let Some(info) = self.entities.get_mut(id.index as usize) else {
            core::hint::cold_path();
            return Err(EntityError::NotFound(id.index));
        };

        if info.generation != id.generation {
            core::hint::cold_path();
            let generation = info.generation;
            let expect = id;
            let actual = EntityId {
                index: id.index,
                generation,
            };
            return Err(EntityError::Mismatch { expect, actual });
        }

        if info.location.is_none() {
            core::hint::cold_path();
            return Err(EntityError::NotSpawned(id));
        }

        if info.parent == parent {
            core::hint::cold_path();
            return Ok(());
        }

        //--------------------------------------------------------------------
        // validate hierarchy

        let mut iter = parent;
        while let Some(p) = iter {
            if p == id {
                core::hint::cold_path();
                return Err(EntityError::CycleHierarchy {
                    id,
                    to: parent.unwrap(),
                });
            }
            debug_assert!((p.index as usize) < self.entities.len());
            iter = unsafe { self.entities.get_unchecked(p.index as usize).parent };
        }

        //--------------------------------------------------------------------
        // modify parent

        let slot = unsafe { self.entities.get_unchecked_mut(id.index as usize) };
        let old = slot.parent;
        slot.parent = parent;

        //--------------------------------------------------------------------
        // modify old parent

        if let Some(o) = old {
            debug_assert!((o.index as usize) < self.entities.len());
            let slot = unsafe { self.entities.get_unchecked_mut(o.index as usize) };
            debug_assert!(slot.location.is_some());

            match position_entity(id, &slot.children) {
                Some(index) => {
                    slot.children.remove(index);
                }
                None => {
                    ::core::hint::cold_path();
                    #[cfg(debug_assertions)]
                    unreachable!("missing child `{id}` in `{o}`");
                }
            }
        } else {
            debug_assert!(self.root.contains(&id));
            self.root.remove(&id);
        }

        //--------------------------------------------------------------------
        // modify new parent

        if let Some(p) = parent {
            let slot = unsafe { self.entities.get_unchecked_mut(p.index as usize) };
            debug_assert!(!slot.children.contains(&id));
            slot.children.push(id);
        } else {
            debug_assert!(!self.root.contains(&id));
            self.root.insert(id);
        }

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Remove

impl Entities {
    /// Frees an entity slot for reuse.
    ///
    /// Advances the slot's generation (wrapping back to `1` on overflow) and
    /// returns the id that now names the slot, so that the caller can hand
    /// it to the allocator for recycling.
    ///
    /// # Panic
    ///
    /// May panic (in debug builds) if the slot's entity is still spawned.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let id = world.spawn((), None).id();
    ///
    /// // `World::despawn` frees the slot and advances the generation so a
    /// // stale handle can never alias the recycled slot.
    /// world.despawn(id).unwrap();
    /// assert_ne!(world.entities().resolve(id.index()), id);
    /// ```
    pub fn free_slot(&mut self, index: u32) -> EntityId {
        self.ensure_exist(index);

        // SAFETY: already ensure exist through `ensure_exist(index)`;
        let info = unsafe { self.entities.get_unchecked_mut(index as usize) };
        debug_assert!(info.location.is_none());

        let generation = info.generation.checked_add(1).unwrap_or_else(|| {
            ::core::hint::cold_path();
            log::warn!(
                "Entity({index}) generation wrapped on `Entities::free_slot`, aliasing may occur."
            );
            NonZeroU32::MIN
        });

        info.generation = generation;

        EntityId { index, generation }
    }

    /// Removes a spawned entity and returns its storage location.
    ///
    /// The entity's children are re-parented to the root. This is the
    /// per-entity removal step driven by `World::despawn`, which calls it
    /// for the entity and every descendant.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let parent = world.spawn((), None).id();
    /// let child = world.spawn((), Some(parent)).id();
    ///
    /// // Despawning the parent removes it (and its descendants) from the
    /// // tree through `Entities::remove_one`.
    /// world.despawn(parent).unwrap();
    /// assert!(!world.entities().contains(parent));
    /// assert!(!world.entities().contains(child));
    /// ```
    pub fn remove_one(&mut self, id: EntityId) -> Result<Location, EntityError> {
        self.ensure_exist(id.index);

        let Some(info) = self.entities.get_mut(id.index as usize) else {
            core::hint::cold_path();
            return Err(EntityError::NotFound(id.index));
        };

        if info.generation != id.generation {
            core::hint::cold_path();
            let generation = info.generation;
            let expect = id;
            let actual = EntityId {
                index: id.index,
                generation,
            };
            return Err(EntityError::Mismatch { expect, actual });
        }

        let location = info.location.take().ok_or(EntityError::NotSpawned(id))?;

        let parent = info.parent.take();
        let children = core::mem::take(&mut info.children);

        if let Some(p) = parent {
            debug_assert!((p.index as usize) < self.entities.len());
            let slot = unsafe { self.entities.get_unchecked_mut(p.index as usize) };
            debug_assert!(slot.location.is_some());

            match position_entity(id, &slot.children) {
                Some(index) => {
                    slot.children.remove(index);
                }
                None => {
                    ::core::hint::cold_path();
                    #[cfg(debug_assertions)]
                    unreachable!("missing child `{id}` in `{p}`");
                }
            }
        } else {
            debug_assert!(self.root.contains(&id));
            self.root.remove(&id);
        }

        for c in children {
            debug_assert!((c.index as usize) < self.entities.len());
            let slot = unsafe { self.entities.get_unchecked_mut(c.index as usize) };
            debug_assert_eq!(slot.parent, Some(id));
            assert_eq!(c.generation, slot.generation);
            slot.parent = None;
            self.root.insert(c);
        }

        Ok(location)
    }
}

impl Entities {
    /// Inserts a spawned entity into the tree, recording its location and
    /// optional parent.
    ///
    /// Validates the parent (existence, generation, spawn state, and that
    /// it is not the entity itself) before wiring the hierarchy: the entity
    /// is added to the parent's children, or to the root set if it has no
    /// parent. This is driven internally by the `World` spawn methods.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let parent = world.spawn((), None).id();
    ///
    /// // `World::spawn((), Some(parent))` records the new entity through
    /// // `Entities::insert_one`.
    /// let child = world.spawn((), Some(parent)).id();
    /// assert_eq!(world.entities().get(child).unwrap().parent, Some(parent));
    /// ```
    pub fn insert_one(
        &mut self,
        id: EntityId,
        parent: Option<EntityId>,
        location: Location,
    ) -> Result<(), EntityError> {
        self.ensure_exist(id.index);

        //--------------------------------------------------------------------
        // validate parent

        if let Some(p) = parent {
            if p == id {
                core::hint::cold_path();
                return Err(EntityError::CycleHierarchy { id, to: p });
            }
            let Some(info) = self.entities.get(p.index as usize) else {
                core::hint::cold_path();
                return Err(EntityError::NotFound(p.index));
            };
            if info.generation != p.generation {
                core::hint::cold_path();
                let generation = info.generation;
                let expect = p;
                let actual = EntityId {
                    index: p.index,
                    generation,
                };
                return Err(EntityError::Mismatch { expect, actual });
            }
            if info.location.is_none() {
                core::hint::cold_path();
                return Err(EntityError::NotSpawned(p));
            }
        }

        //--------------------------------------------------------------------
        // validate self

        let Some(info) = self.entities.get_mut(id.index as usize) else {
            core::hint::cold_path();
            return Err(EntityError::NotFound(id.index));
        };

        if info.generation != id.generation {
            core::hint::cold_path();
            let generation = info.generation;
            let expect = id;
            let actual = EntityId {
                index: id.index,
                generation,
            };
            return Err(EntityError::Mismatch { expect, actual });
        }

        if info.location.is_some() {
            core::hint::cold_path();
            return Err(EntityError::AlreadySpawned(id));
        }

        info.location = Some(location);
        info.parent = parent;
        debug_assert!(info.children.is_empty());

        if let Some(p) = parent {
            let slot = unsafe { self.entities.get_unchecked_mut(p.index as usize) };
            debug_assert!(!slot.children.contains(&id));
            slot.children.push(id);
        } else {
            debug_assert!(!self.root.contains(&id));
            self.root.insert(id);
        }

        Ok(())
    }
}

impl Entities {
    /// Inserts a spawned entity without wiring up parent/child links.
    ///
    /// The caller is responsible for establishing the hierarchy separately;
    /// this is used internally by `World::spawn_uninit`, where the parent
    /// is only a placeholder until the entity's data is fully initialized.
    pub fn insert_uninit(
        &mut self,
        id: EntityId,
        parent: Option<EntityId>,
        location: Location,
    ) -> Result<(), EntityError> {
        self.ensure_exist(id.index);

        //--------------------------------------------------------------------
        // validate parent
        if let Some(p) = parent {
            if p == id {
                core::hint::cold_path();
                return Err(EntityError::CycleHierarchy { id, to: p });
            }
            let Some(info) = self.entities.get(p.index as usize) else {
                core::hint::cold_path();
                return Err(EntityError::NotFound(p.index));
            };
            if info.generation != p.generation {
                core::hint::cold_path();
                let generation = info.generation;
                let expect = p;
                let actual = EntityId {
                    index: p.index,
                    generation,
                };
                return Err(EntityError::Mismatch { expect, actual });
            }
            if info.location.is_none() {
                core::hint::cold_path();
                return Err(EntityError::NotSpawned(p));
            }
        }

        let Some(info) = self.entities.get_mut(id.index as usize) else {
            core::hint::cold_path();
            return Err(EntityError::NotFound(id.index));
        };

        if info.generation != id.generation {
            core::hint::cold_path();
            let generation = info.generation;
            let expect = id;
            let actual = EntityId {
                index: id.index,
                generation,
            };
            return Err(EntityError::Mismatch { expect, actual });
        }

        if info.location.is_some() {
            core::hint::cold_path();
            return Err(EntityError::AlreadySpawned(id));
        }

        info.location = Some(location);
        info.parent = parent;
        debug_assert!(info.children.is_empty());

        // Not modify `parent`'s children

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Error
// -----------------------------------------------------------------------------

/// Errors returned by [`Entities`] operations.
///
/// All variants carry enough information to diagnose the failing entity
/// operation, and implement `Display` for user-facing messages.
#[derive(Debug, Error, Clone, Copy)]
#[zlim_error(warning)]
pub enum EntityError {
    /// The slot for the given index has never been used (index out of bounds).
    #[error("Entity with Index {_0} was not found")]
    NotFound(u32),

    /// The entity exists (generation matches) but is not spawned, so it has
    /// no storage location.
    #[error("Entity {_0} has not been spawned yet")]
    NotSpawned(EntityId),

    /// The entity is already spawned and cannot be spawned a second time.
    #[error("Entity {_0} has already been spawned")]
    AlreadySpawned(EntityId),

    /// The id's generation does not match the slot's current generation,
    /// meaning the handle is stale.
    #[error("Entity mismatch: expected {expect}, found {actual}")]
    Mismatch { expect: EntityId, actual: EntityId },

    /// Moving `id` under `to` would make it its own ancestor (a cycle).
    #[error("Cannot move `{id}` as a child of `{to}`: `{id}` is an ancestor of `{to}`.")]
    CycleHierarchy { id: EntityId, to: EntityId },
}

// -----------------------------------------------------------------------------
