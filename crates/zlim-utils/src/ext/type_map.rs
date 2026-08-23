//! Provides a TypeId Map.

use core::any::TypeId;
use core::fmt::Debug;

use crate::hash::NoopState;
use hashbrown::HashMap;
use hashbrown::hash_map::Entry;

// -----------------------------------------------------------------------------
// TypeMap

pub type TypeMapEntry<'a, V> = Entry<'a, TypeId, V, NoopState>;

/// A specialized map container with [`TypeId`] as the fixed key type.
///
/// The current implementation uses [`HashMap`], assuming its performance
/// is generally superior to `BTreeMap` for most use cases, though this
/// has not been extensively benchmarked.
///
/// The container's interface is fully abstracted, exposing no [`HashMap`]
/// specific APIs. This allows for potential future changes to the underlying
/// implementation without breaking external code.
pub struct TypeMap<V>(HashMap<TypeId, V, NoopState>);

impl<V> TypeMap<V> {
    /// Creates an empty `TypeMap`.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::ext::TypeMap;
    /// let map = TypeMap::<i32>::new();
    /// ```
    #[inline]
    pub const fn new() -> Self {
        Self(HashMap::with_hasher(NoopState))
    }

    /// Creates an empty `TypeMap` with the specified capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::ext::TypeMap;
    /// let map = TypeMap::<i32>::with_capacity(10);
    /// ```
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self(HashMap::with_capacity_and_hasher(capacity, NoopState))
    }

    /// Shrinks the capacity of the map as much as possible.
    ///
    /// It will drop down as much as possible while maintaining the internal rules
    /// and possibly leaving some space in accordance with the resize policy.
    #[inline]
    pub fn shrink_to_fit(&mut self) {
        self.0.shrink_to_fit();
    }

    /// Reserves capacity for at least `additional` more elements to be inserted.
    ///
    /// The collection may reserve more space to avoid frequent reallocations.
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.0.reserve(additional);
    }

    /// Attempts to insert a key-value pair into the map.
    ///
    /// - Returns `true` if the key was not present and the pair was successfully inserted.
    /// - Returns `false` if the key already exists, leaving the map unchanged.
    ///
    /// The closure `f` is only called if the key is not present.
    #[inline]
    pub fn try_insert(&mut self, type_id: TypeId, f: impl FnOnce() -> V) -> bool {
        match self.0.entry(type_id) {
            Entry::Vacant(entry) => {
                entry.insert(f());
                true
            }
            Entry::Occupied(_) => false,
        }
    }

    /// Gets a mutable reference to the value associated with the given key,
    /// inserting the result of `f` if the key is not present.
    ///
    /// If the key exists, returns a mutable reference to the existing value.
    /// If the key does not exist, calls the closure `f` to create a value,
    /// inserts it, and returns a mutable reference to it.
    ///
    /// The closure `f` is only called if the key is not present.
    #[inline]
    pub fn get_or_insert(&mut self, type_id: TypeId, f: impl FnOnce() -> V) -> &mut V {
        match self.0.entry(type_id) {
            Entry::Vacant(entry) => entry.insert(f()),
            Entry::Occupied(entry) => entry.into_mut(),
        }
    }

    /// Returns a reference to the value corresponding to the type.
    #[inline]
    pub fn get(&self, type_id: TypeId) -> Option<&V> {
        self.0.get(&type_id)
    }

    /// Returns a mutable reference to the value corresponding to the type.
    #[inline]
    pub fn get_mut(&mut self, type_id: TypeId) -> Option<&mut V> {
        self.0.get_mut(&type_id)
    }

    /// Inserts a key-value pair into the map.
    #[inline]
    pub fn insert(&mut self, type_id: TypeId, v: V) -> Option<V> {
        self.0.insert(type_id, v)
    }

    /// Remove a pair if present, the order is random.
    #[inline]
    pub fn remove_one(&mut self) -> Option<(TypeId, V)> {
        let type_id = *self.0.keys().next()?;
        self.0.remove_entry(&type_id)
    }

    /// Removes a key from the map, returning the value
    /// at the key if the key was previously in the map.
    ///
    /// Keeps the allocated memory for reuse.
    #[inline]
    pub fn remove(&mut self, type_id: TypeId) -> Option<V> {
        self.0.remove(&type_id)
    }

    /// Clears the map, removing all key-value pairs.
    ///
    /// Keeps the allocated memory for reuse.
    #[inline]
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Returns `true` if the map contains a value for the specified key.
    #[inline]
    pub fn contains(&self, type_id: TypeId) -> bool {
        self.0.contains_key(&type_id)
    }

    /// Returns the number of elements in the map.
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the map contains no elements.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// An iterator visiting all key-value pairs in arbitrary order.
    ///
    /// The iterator element type is `(&'a K, &'a V)`.
    #[inline]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (TypeId, &V)> {
        self.0.iter().map(|(&k, v)| (k, v))
    }

    /// An iterator visiting all key-value pairs in arbitrary order,
    /// with mutable references to the values.
    ///
    /// The iterator element type is `(&'a K, &'a mut V)`.
    #[inline]
    pub fn iter_mut(&mut self) -> impl ExactSizeIterator<Item = (TypeId, &mut V)> {
        self.0.iter_mut().map(|(&k, v)| (k, v))
    }

    /// An iterator visiting all values in arbitrary order.
    ///
    /// The iterator element type is `&'a V`.
    #[inline]
    pub fn values(&self) -> impl ExactSizeIterator<Item = &V> {
        self.0.values()
    }

    /// An iterator visiting all values mutably in arbitrary order.
    ///
    /// The iterator element type is `&'a mut V`.
    #[inline]
    pub fn values_mut(&mut self) -> impl ExactSizeIterator<Item = &mut V> {
        self.0.values_mut()
    }

    /// An iterator visiting all keys in arbitrary order.
    ///
    /// The iterator element type is `&'a TypeId`.
    #[inline]
    pub fn keys(&self) -> impl ExactSizeIterator<Item = &TypeId> {
        self.0.keys()
    }

    /// An iterator visiting all keys in arbitrary order.
    ///
    /// The iterator element type is `TypeId`, similar to `keys().copied()`.
    #[inline]
    pub fn types(&self) -> impl ExactSizeIterator<Item = TypeId> {
        self.0.keys().copied()
    }

    /// Gets the given key's corresponding entry in the map for in-place manipulation.
    #[inline]
    pub fn entry(&mut self, type_id: TypeId) -> TypeMapEntry<'_, V> {
        self.0.entry(type_id)
    }
}

// -----------------------------------------------------------------------------
// Traits

impl<T> Default for TypeMap<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> Clone for TypeMap<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }

    #[inline]
    fn clone_from(&mut self, source: &Self) {
        self.0.clone_from(&source.0);
    }
}

impl<T: Debug> Debug for TypeMap<T> {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}
