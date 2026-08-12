use core::fmt::{Debug, Formatter};
use core::num::NonZeroU32;
use std::collections::BTreeSet;

use zlim_core_derive::Error;

use crate::table::MovedEntityRow;

use super::{EntityId, Location};

// -----------------------------------------------------------------------------
// EntityNode
// -----------------------------------------------------------------------------

#[derive(Clone)]
pub struct EntityNode {
    pub generation: NonZeroU32,
    pub location: Option<Location>,
    pub child_of: Option<EntityId>,
    pub children: BTreeSet<EntityId>,
}

const DEFAULT_NODE: EntityNode = EntityNode {
    generation: NonZeroU32::MIN,
    location: None,
    child_of: None,
    children: BTreeSet::new(),
};

const DEFAULT_REF: &EntityNode = &EntityNode {
    generation: NonZeroU32::MIN,
    location: None,
    child_of: None,
    children: BTreeSet::new(),
};

impl Debug for EntityNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut debugger = f.debug_struct("Node");
        debugger.field("generation", &self.generation);

        if let Some(location) = &self.location {
            debugger.field("location", location);

            if let Some(child_of) = &self.child_of {
                debugger.field("child_of", child_of);
            }
            if !self.children.is_empty() {
                debugger.field("children", &self.children);
            }
        }

        debugger.finish()
    }
}

// -----------------------------------------------------------------------------
// EntityTree
// -----------------------------------------------------------------------------

pub struct EntityTree {
    pub(crate) root: BTreeSet<EntityId>,
    pub(crate) entities: Vec<EntityNode>,
}

impl Debug for EntityTree {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let iter = self
            .entities
            .iter()
            .enumerate()
            .filter(|(_, info)| info.location.is_some());

        f.debug_map().entries(iter).finish()
    }
}

impl Default for EntityTree {
    fn default() -> Self {
        let root: BTreeSet<EntityId> = BTreeSet::new();
        let mut entities: Vec<EntityNode> = Vec::with_capacity(256);
        let new_len = entities.capacity();
        entities.resize_with(new_len, || DEFAULT_NODE);
        Self { root, entities }
    }
}

// -----------------------------------------------------------------------------
// Private Methods

impl EntityTree {
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

impl EntityTree {
    /// Tries to retrieve the location of a spawned entity.
    ///
    /// Time Complexity: `O(1)`
    ///
    /// # Returns
    /// - `Ok(Some(Location))` - The entity's current storage location
    /// - `Ok(None)` - The entity is not spawned but the generation matches.
    /// - `Err(FetchError)` - Generation mismatches.
    ///
    /// # Errors
    /// - `FetchError::Mismatch` - Generation counter mismatch (stale entity)
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
    /// - `Err(FetchError)` - If the entity doesn't exist, generation
    ///   mismatches, or the entity is not spawned
    ///
    /// # Errors
    /// - `FetchError::NotFound` - Entity index out of bounds
    /// - `FetchError::Mismatch` - Generation counter mismatch (stale entity)
    /// - `FetchError::NotSpawned` - Entity exists but is not spawned
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

    /// Retrieves the `EntityNode` from given index.
    ///
    /// Time Complexity: `O(1)`
    pub fn get_by_index(&self, index: u32) -> Result<&EntityNode, EntityError> {
        let Some(info) = self.entities.get(index as usize) else {
            core::hint::cold_path();
            return Err(EntityError::NotFound(index));
        };

        Ok(info)
    }

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

impl EntityTree {
    /// Resolves an index to its current `EntityId` with correct generation.
    ///
    /// Time Complexity: `O(1)`
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

    /// Return the number of **spawned** entities.
    ///
    /// Time Complexity: `O(N)`
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
    /// - `Err(SpawnError)` - If spawning is not possible
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

impl EntityTree {
    pub fn modify_child_of(
        &mut self,
        id: EntityId,
        child_of: Option<EntityId>,
    ) -> Result<(), EntityError> {
        //--------------------------------------------------------------------
        // validate new child_of

        if let Some(p) = child_of {
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

        if info.child_of == child_of {
            core::hint::cold_path();
            return Ok(());
        }

        //--------------------------------------------------------------------
        // validate hierarchy

        let mut iter = child_of;
        while let Some(p) = iter {
            if p == id {
                core::hint::cold_path();
                return Err(EntityError::CycleHierarchy {
                    id,
                    to: child_of.unwrap(),
                });
            }
            debug_assert!(self.entities.len() < (p.index as usize));
            iter = unsafe { self.entities.get_unchecked(p.index as usize).child_of };
        }

        //--------------------------------------------------------------------
        // modify child_of

        let slot = unsafe { self.entities.get_unchecked_mut(id.index as usize) };
        let old = slot.child_of;
        slot.child_of = child_of;

        //--------------------------------------------------------------------
        // modify old parent

        if let Some(o) = old {
            debug_assert!(self.entities.len() < (o.index as usize));
            let slot = unsafe { self.entities.get_unchecked_mut(o.index as usize) };
            debug_assert!(slot.location.is_some());
            debug_assert!(slot.children.contains(&id));
            let _ = slot.children.remove(&id);
        } else {
            debug_assert!(self.root.contains(&id));
            self.root.remove(&id);
        }

        //--------------------------------------------------------------------
        // modify new parent

        if let Some(p) = child_of {
            let slot = unsafe { self.entities.get_unchecked_mut(p.index as usize) };
            debug_assert!(!slot.children.contains(&id));
            slot.children.insert(id);
        } else {
            debug_assert!(!self.root.contains(&id));
            self.root.insert(id);
        }

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Remove

impl EntityTree {
    /// Frees an entity slot for reuse.
    ///
    /// # Panic
    ///
    /// May panic if the entity of slot is un-despawned.
    pub fn free_slot(&mut self, index: u32) -> EntityId {
        self.ensure_exist(index);

        // SAFETY: already ensure exist through `ensure_exist(index)`;
        let info = unsafe { self.entities.get_unchecked_mut(index as usize) };
        debug_assert!(info.location.is_none());

        let generation = info.generation.checked_add(1).unwrap_or_else(|| {
            ::core::hint::cold_path();
            log::warn!(
                "Entity({index}) generation wrapped on `EntityTree::free_slot`, aliasing may occur."
            );
            NonZeroU32::MIN
        });

        info.generation = generation;

        EntityId { index, generation }
    }

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

        let child_of = info.child_of.take();
        let children = core::mem::take(&mut info.children);

        if let Some(p) = child_of {
            debug_assert!(self.entities.len() < (p.index as usize));
            let slot = unsafe { self.entities.get_unchecked_mut(p.index as usize) };
            debug_assert!(slot.location.is_some());
            debug_assert!(slot.children.contains(&id));
            let _ = slot.children.remove(&id);
        } else {
            debug_assert!(self.root.contains(&id));
            self.root.remove(&id);
        }

        for c in children {
            debug_assert!(self.entities.len() < (c.index as usize));
            let slot = unsafe { self.entities.get_unchecked_mut(c.index as usize) };
            debug_assert_eq!(slot.child_of, Some(c));
            assert_eq!(c.generation, slot.generation);
            slot.child_of = None;
            self.root.insert(c);
        }

        Ok(location)
    }
}

impl EntityTree {
    pub fn insert_one(
        &mut self,
        id: EntityId,
        child_of: Option<EntityId>,
        location: Location,
    ) -> Result<(), EntityError> {
        self.ensure_exist(id.index);

        //--------------------------------------------------------------------
        // validate child_of

        if let Some(p) = child_of {
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
        info.child_of = child_of;
        debug_assert!(info.children.is_empty());

        if let Some(p) = child_of {
            let slot = unsafe { self.entities.get_unchecked_mut(p.index as usize) };
            debug_assert!(!slot.children.contains(&id));
            slot.children.insert(id);
        } else {
            debug_assert!(!self.root.contains(&id));
            self.root.insert(id);
        }

        Ok(())
    }
}

impl EntityTree {
    pub fn insert_uninit(
        &mut self,
        id: EntityId,
        child_of: Option<EntityId>,
        location: Location,
    ) -> Result<(), EntityError> {
        self.ensure_exist(id.index);

        //--------------------------------------------------------------------
        // validate child_of
        if let Some(p) = child_of {
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
        info.child_of = child_of;
        debug_assert!(info.children.is_empty());

        // Not modify `child_of`'s children

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Error
// -----------------------------------------------------------------------------

#[derive(Debug, Error, Clone, Copy)]
#[zlim_error(warning)]
pub enum EntityError {
    #[error("Entity with Index {_0} was not found")]
    NotFound(u32),

    #[error("Entity {_0} has not been spawned yet")]
    NotSpawned(EntityId),

    #[error("Entity {_0} has already been spawned")]
    AlreadySpawned(EntityId),

    #[error("Entity mismatch: expected {expect}, found {actual}")]
    Mismatch { expect: EntityId, actual: EntityId },

    #[error("Try move `{id}` as `{to}`'s children, be `{id}` is a ancestor of `{to}`.")]
    CycleHierarchy { id: EntityId, to: EntityId },
}

// -----------------------------------------------------------------------------
