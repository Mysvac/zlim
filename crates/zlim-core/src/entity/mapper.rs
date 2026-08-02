use core::fmt::Debug;
use core::ops::Index;

use serde::{Deserialize, Serialize};
use zlim_utils::hash::{HashMap, SparseState};

use crate::entity::EntityId;

// ------------------------------------------------------------------------------
// MapEntities

pub trait MapEntities {
    fn map_entities<E: EntityMapper>(&mut self, entity_mapper: &mut E);
}

// ------------------------------------------------------------------------------
// EntityMapper

pub trait EntityMapper {
    fn get_mapped(&mut self, source: EntityId) -> EntityId;

    fn set_mapped(&mut self, source: EntityId, target: EntityId);
}

// ------------------------------------------------------------------------------
// EntityMap

#[derive(Serialize, Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct EntityMap<T>(HashMap<EntityId, T, SparseState>);

impl<T> Default for EntityMap<T> {
    fn default() -> Self {
        Self(HashMap::with_hasher(SparseState))
    }
}

impl<T> EntityMap<T> {
    /// Create a empty [`EntityMap`]
    #[inline(always)]
    pub const fn new() -> Self {
        Self(HashMap::with_hasher(SparseState))
    }

    /// Create a empty [`EntityMap`] with specific capacity
    #[inline(always)]
    pub fn with_capacity(capacity: usize) -> Self {
        Self(HashMap::with_capacity_and_hasher(capacity, SparseState))
    }

    /// Returns the number of elements the map can hold without reallocating.
    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    /// An iterator visiting all entities in arbitrary order.
    #[inline(always)]
    pub fn entities(&self) -> impl ExactSizeIterator<Item = EntityId> {
        self.0.keys().copied()
    }

    /// An iterator visiting all keys in arbitrary order.
    #[inline(always)]
    pub fn keys(&self) -> impl ExactSizeIterator<Item = &EntityId> {
        self.0.keys()
    }

    /// An iterator visiting all values in arbitrary order.
    #[inline(always)]
    pub fn values(&self) -> impl ExactSizeIterator<Item = &T> {
        self.0.values()
    }

    /// An iterator visiting all values mutably in arbitrary order.
    #[inline(always)]
    pub fn values_mut(&mut self) -> impl ExactSizeIterator<Item = &mut T> {
        self.0.values_mut()
    }

    /// An iterator visiting all key-value pairs in arbitrary order.
    #[inline]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (EntityId, &T)> {
        self.0.iter().map(|(&k, v)| (k, v))
    }

    /// An iterator visiting all key-value pairs in arbitrary order, with mutable references to the values.
    #[inline]
    pub fn iter_mut(&mut self) -> impl ExactSizeIterator<Item = (EntityId, &mut T)> {
        self.0.iter_mut().map(|(&k, v)| (k, v))
    }

    /// Returns the number of elements in the map.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if the map contains no elements.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Clears the map, removing all key-value pairs. Keeps the allocated memory for reuse.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Reserves capacity for at least additional more elements to be inserted in the HashMap.
    #[inline(always)]
    pub fn reserve(&mut self, additional: usize) {
        self.0.reserve(additional);
    }

    /// Shrinks the capacity of the map as much as possible.
    #[inline(always)]
    pub fn shrink_to_fit(&mut self) {
        self.0.shrink_to_fit();
    }

    /// Shrinks the capacity of the map with a lower limit.
    #[inline(always)]
    pub fn shrink_to(&mut self, min_capacity: usize) {
        self.0.shrink_to(min_capacity);
    }

    /// Returns a reference to the value corresponding to the key.
    #[inline(always)]
    pub fn get(&self, k: EntityId) -> Option<&T> {
        self.0.get(&k)
    }

    /// Returns a mutable reference to the value corresponding to the key.
    #[inline(always)]
    pub fn get_mut(&mut self, k: EntityId) -> Option<&mut T> {
        self.0.get_mut(&k)
    }

    /// Returns true if the map contains a value for the specified entity.
    #[inline(always)]
    pub fn contains(&self, k: EntityId) -> bool {
        self.0.contains_key(&k)
    }

    /// Inserts a key-value pair into the map.
    #[inline(always)]
    pub fn insert(&mut self, k: EntityId, v: T) -> Option<T> {
        self.0.insert(k, v)
    }

    /// Removes a key from the map, returning the value at the key if the key
    /// was previously in the map. Keeps the allocated memory for reuse.
    #[inline(always)]
    pub fn remove(&mut self, k: EntityId) -> Option<T> {
        self.0.remove(&k)
    }
}

impl<T: Debug> Debug for EntityMap<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_map().entries(&self.0).finish()
    }
}

impl<T: Clone> Clone for EntityMap<T> {
    #[inline(always)]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }

    #[inline(always)]
    fn clone_from(&mut self, source: &Self) {
        self.0.clone_from(&source.0);
    }
}

impl<T> Index<EntityId> for EntityMap<T> {
    type Output = T;

    #[inline(always)]
    fn index(&self, index: EntityId) -> &Self::Output {
        self.0.index(&index)
    }
}

impl<T> IntoIterator for EntityMap<T> {
    type Item = <HashMap<EntityId, T, SparseState> as IntoIterator>::Item;
    type IntoIter = <HashMap<EntityId, T, SparseState> as IntoIterator>::IntoIter;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a EntityMap<T> {
    type Item = <&'a HashMap<EntityId, T, SparseState> as IntoIterator>::Item;
    type IntoIter = <&'a HashMap<EntityId, T, SparseState> as IntoIterator>::IntoIter;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        (&self.0).into_iter()
    }
}

impl<'a, T> IntoIterator for &'a mut EntityMap<T> {
    type Item = <&'a mut HashMap<EntityId, T, SparseState> as IntoIterator>::Item;
    type IntoIter = <&'a mut HashMap<EntityId, T, SparseState> as IntoIterator>::IntoIter;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        (&mut self.0).into_iter()
    }
}

impl<T> Extend<T> for EntityMap<T>
where
    HashMap<EntityId, T, SparseState>: Extend<T>,
{
    #[inline(always)]
    fn extend<U: IntoIterator<Item = T>>(&mut self, iter: U) {
        self.0.extend(iter);
    }
}
