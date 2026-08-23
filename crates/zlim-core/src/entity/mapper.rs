//! Entity-id remapping for cloning and scene instantiation.
//!
//! [`MapEntities`] rewrites every [`EntityId`] a type holds, translating
//! each one through an [`EntityMapper`]. [`EntityMap`] is a sparse
//! id-keyed map that doubles as an [`EntityMapper`] implementation, so it
//! can serve as the translation table for clone and deserialization
//! operations.

use core::fmt::Debug;
use core::hash::{BuildHasher, Hash};

use serde::{Deserialize, Serialize};
use zlim_utils::hash::{HashMap, HashSet, SparseState};

use super::EntityId;

// -----------------------------------------------------------------------------
// EntityMapper & MapEntities

/// Trait for types that contain entity IDs and support remapping.
///
/// Implementing types traverse and remap every [`EntityId`] they hold
/// via the provided [`EntityMapper`]. Used during operations like cloning
/// and deserialization to rewrite entity handles.
///
/// The trait is implemented for most containers out of the box (vectors,
/// arrays, maps, sets, [`Option`], ...), so a type that stores entity
/// references in such containers usually only needs to implement it for
/// its own fields.
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// let mut map = EntityMap::new();
/// let old_a = EntityId::from_bits(0x1_0000_0001).unwrap();
/// let old_b = EntityId::from_bits(0x1_0000_0002).unwrap();
/// let new_a = EntityId::from_bits(0x1_0000_0003).unwrap();
/// map.insert(old_a, new_a);
///
/// // `MapEntities` rewrites every entity reference it holds; ids without
/// // a registered mapping are left unchanged.
/// let mut batch = vec![old_a, old_b];
/// batch.map_entities(&mut map);
/// assert_eq!(batch, vec![new_a, old_b]);
/// ```
pub trait MapEntities {
    /// Walks all [`EntityId`] values in `self` and remaps each one
    /// through `entity_mapper`.
    fn map_entities<E: EntityMapper>(&mut self, entity_mapper: &mut E);
}

/// Trait for mapping old [`EntityId`] values to new ones.
///
/// An [`EntityMapper`] records source-to-target mappings and provides
/// lookups for [`MapEntities`] to use during remapping. Implementations
/// range from no-op identity mappers to full [`EntityMap`]-backed
/// translators.
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::entity::EntityMapper;
///
/// // A mapper that shifts every entity id's index by a fixed offset.
/// struct OffsetMapper(u32);
///
/// impl EntityMapper for OffsetMapper {
///     fn get_mapped(&mut self, source: EntityId) -> EntityId {
///         let shifted = source.to_bits() + self.0 as u64;
///         EntityId::from_bits(shifted).unwrap_or(source)
///     }
///
///     fn set_mapped(&mut self, _source: EntityId, _target: EntityId) {
///         // Read-only mappers do not need to record mappings.
///     }
/// }
///
/// let mut mapper = OffsetMapper(1);
/// let id = EntityId::from_bits(0x1_0000_0001).unwrap();
/// assert_eq!(mapper.get_mapped(id).index(), 2);
/// ```
pub trait EntityMapper {
    /// Returns the mapped [`EntityId`] for `source`, or `source` itself
    /// if no mapping has been registered.
    fn get_mapped(&mut self, source: EntityId) -> EntityId;

    /// Records a mapping from `source` to `target`.
    fn set_mapped(&mut self, source: EntityId, target: EntityId);
}

// -----------------------------------------------------------------------------
// EntityMap

/// A map keyed by [`EntityId`], optimized for sparse entity storage.
///
/// Uses [`SparseState`] hashing for efficient lookups and iteration over
/// entity-indexed data. Implements [`EntityMapper`] so it can serve as
/// a remapping table during clone and deserialization operations.
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// let mut map = EntityMap::with_capacity(16);
/// let a = EntityId::from_bits(0x1_0000_0001).unwrap();
/// let b = EntityId::from_bits(0x1_0000_0002).unwrap();
///
/// map.insert(a, 10);
/// map.insert(b, 20);
///
/// assert_eq!(map.len(), 2);
/// assert_eq!(map.get(a), Some(&10));
/// assert_eq!(map.remove(a), Some(10));
/// ```
///
/// [`SparseState`]: zlim_utils::hash::SparseState
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
    /// Creates an empty [`EntityMap`].
    ///
    /// This is a `const` constructor; for a pre-sized map, use
    /// [`with_capacity`](Self::with_capacity).
    #[inline(always)]
    pub const fn new() -> Self {
        Self(HashMap::with_hasher(SparseState))
    }

    /// Creates an empty [`EntityMap`] with the given capacity.
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

    /// Returns `true` if the map contains no elements.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Clears the map, removing all key-value pairs. Keeps the allocated memory for reuse.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Reserves capacity for at least `additional` more elements.
    #[inline(always)]
    pub fn reserve(&mut self, additional: usize) {
        self.0.reserve(additional);
    }

    /// Shrinks the capacity of the map as much as possible.
    #[inline(always)]
    pub fn shrink_to_fit(&mut self) {
        self.0.shrink_to_fit();
    }

    /// Shrinks the capacity of the map down to at least `min_capacity`.
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

    /// Returns `true` if the map contains a value for the specified entity.
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

// -----------------------------------------------------------------------------
// Impl MapEntities

impl MapEntities for () {
    #[inline(always)]
    fn map_entities<E: EntityMapper>(&mut self, _: &mut E) {}
}

impl MapEntities for EntityId {
    #[inline]
    fn map_entities<E: EntityMapper>(&mut self, entity_mapper: &mut E) {
        *self = entity_mapper.get_mapped(*self);
    }
}

impl<T: MapEntities> MapEntities for Option<T> {
    #[inline]
    fn map_entities<E: EntityMapper>(&mut self, entity_mapper: &mut E) {
        if let Some(entities) = self {
            entities.map_entities(entity_mapper);
        }
    }
}

macro_rules! impl_map_entities_for_map {
    ($($ty:tt)*) => {
        impl $($ty)* {
            fn map_entities<E: EntityMapper>(&mut self, entity_mapper: &mut E) {
                *self = core::mem::take(self)
                    .into_iter()
                    .map(|(mut key_entities, mut value_entities)| {
                        key_entities.map_entities(entity_mapper);
                        value_entities.map_entities(entity_mapper);
                        (key_entities, value_entities)
                    })
                    .collect();
            }
        }
    };
}

impl_map_entities_for_map! {
    <K: MapEntities + Eq + Hash, V: MapEntities, S: BuildHasher + Default> MapEntities for HashMap<K, V, S>
}

impl_map_entities_for_map! {
    <K: MapEntities + Eq + Hash, V: MapEntities, S: BuildHasher + Default> MapEntities for std::collections::HashMap<K, V, S>
}

impl_map_entities_for_map! {
    <K: MapEntities + Ord, V: MapEntities> MapEntities for std::collections::BTreeMap<K, V>
}

macro_rules! impl_map_entities_for_set {
    ($($ty:tt)*) => {
        impl $($ty)* {
            fn map_entities<E: EntityMapper>(&mut self, entity_mapper: &mut E) {
                *self = core::mem::take(self)
                    .into_iter()
                    .map(|mut entities| {
                        entities.map_entities(entity_mapper);
                        entities
                    })
                    .collect();
            }
        }
    };
}

impl_map_entities_for_set! {
    <T: MapEntities + Eq + Hash, S: BuildHasher + Default> MapEntities for HashSet<T, S>
}

impl_map_entities_for_set! {
    <T: MapEntities + Eq + Hash, S: BuildHasher + Default> MapEntities for std::collections::HashSet<T, S>
}

impl_map_entities_for_set! {
    <T: MapEntities + Ord> MapEntities for std::collections::BTreeSet<T>
}

macro_rules! impl_map_entities_for_list {
    ($($ty:tt)*) => {
        impl $($ty)* {
            fn map_entities<E: EntityMapper>(&mut self, entity_mapper: &mut E) {
                for entities in self.iter_mut() {
                    entities.map_entities(entity_mapper);
                }
            }
        }
    };
}

impl_map_entities_for_list!(<T: MapEntities, const N: usize> MapEntities for [T; N]);
impl_map_entities_for_list!(<T: MapEntities, const N: usize> MapEntities for zlim_utils::vec::ArrayVec<T, N>);
impl_map_entities_for_list!(<T: MapEntities, const N: usize> MapEntities for zlim_utils::vec::SmallVec<T, N>);
impl_map_entities_for_list!(<T: MapEntities, const N: usize> MapEntities for zlim_utils::ext::ArrayDeque<T, N>);
impl_map_entities_for_list!(<T: MapEntities> MapEntities for &mut [T]);
impl_map_entities_for_list!(<T: MapEntities> MapEntities for Vec<T>);
impl_map_entities_for_list!(<T: MapEntities> MapEntities for zlim_utils::ext::BlockList<T>);
impl_map_entities_for_list!(<T: MapEntities> MapEntities for std::collections::VecDeque<T>);
impl_map_entities_for_list!(<T: MapEntities> MapEntities for std::collections::LinkedList<T>);

// -----------------------------------------------------------------------------
// EntityMapper Implementation

impl EntityMapper for () {
    #[inline]
    fn get_mapped(&mut self, source: EntityId) -> EntityId {
        source
    }

    #[inline]
    fn set_mapped(&mut self, _source: EntityId, _target: EntityId) {}
}

impl EntityMapper for (EntityId, EntityId) {
    #[inline]
    fn get_mapped(&mut self, source: EntityId) -> EntityId {
        if source == self.0 { self.1 } else { source }
    }

    #[inline]
    fn set_mapped(&mut self, _source: EntityId, _target: EntityId) {}
}

impl EntityMapper for EntityMap<EntityId> {
    #[inline]
    fn get_mapped(&mut self, source: EntityId) -> EntityId {
        self.get(source).copied().unwrap_or(source)
    }

    #[inline]
    fn set_mapped(&mut self, source: EntityId, target: EntityId) {
        self.insert(source, target);
    }
}

impl EntityMapper for &mut dyn EntityMapper {
    #[inline(always)]
    fn get_mapped(&mut self, source: EntityId) -> EntityId {
        (*self).get_mapped(source)
    }

    #[inline(always)]
    fn set_mapped(&mut self, source: EntityId, target: EntityId) {
        (*self).set_mapped(source, target);
    }
}

// -----------------------------------------------------------------------------
