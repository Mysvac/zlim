//! Provide [`HashMap`] based on [hashbrown]'s implementation.
//!
//! Unlike [`hashbrown::HashMap`], [`HashMap`] defaults to [`FixedState`]
//! instead of `RandomState`.
//!
//! This provides determinism by default with an acceptable compromise to denial
//! of service resistance in the context of a game engine.

use core::fmt::Debug;
use core::hash::{BuildHasher, Hash};
use core::ops::Index;

use hashbrown::{Equivalent, TryReserveError, hash_map as hb};

use super::hasher::FixedState;

// -----------------------------------------------------------------------------
// Re-Exports

pub use hb::{Drain, Entry, EntryRef, ExtractIf, IntoIter, IntoKeys};
pub use hb::{IntoValues, Iter, IterMut, Keys, OccupiedEntry};
pub use hb::{OccupiedError, VacantEntry, Values, ValuesMut};

// -----------------------------------------------------------------------------
// HashMap

/// New-type for [`HashMap`] with [`FixedState`] as the default hashing provider.
///
/// Can be trivially converted to and from a [hashbrown] [`HashMap`] using [`From`].
///
/// This provides determinism by default with an acceptable compromise to denial
/// of service resistance in the context of a game engine.
///
/// # Examples
///
/// ```
/// use zlim_utils::hash::HashMap;
///
/// let mut scores = HashMap::new();
///
/// scores.insert("a", 25);
/// scores.insert("b", 24);
/// scores.insert("c", 12);
///
/// for (name, score) in &scores {
///     // Fixed printing order,
///     // but may not be a -> b -> c.
///     println!("{}: {}", name, score);
/// }
/// ```
///
/// [`HashMap`]: hb::HashMap
#[repr(transparent)]
pub struct HashMap<K, V, S = FixedState>(hb::HashMap<K, V, S>);

// -----------------------------------------------------------------------------
// `FixedState` specific methods

impl<K: Eq + Hash, V, const N: usize> From<[(K, V); N]> for HashMap<K, V> {
    #[inline]
    fn from(value: [(K, V); N]) -> Self {
        value.into_iter().collect()
    }
}

impl<K, V> HashMap<K, V> {
    /// Create a empty [`HashMap`]
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_utils::hash::HashMap;
    ///
    /// let map = HashMap::new();
    /// # // docs test
    /// # let mut map = map;
    /// # map.insert(0usize, "foo");
    /// # assert_eq!(map.get(&0), Some("foo").as_ref());
    /// ```
    #[inline(always)]
    pub const fn new() -> Self {
        Self(hb::HashMap::with_hasher(FixedState))
    }

    /// Create a empty [`HashMap`] with specific capacity
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// #
    /// let map = HashMap::with_capacity(5);
    /// # // docs test
    /// # let mut map = map;
    /// # map.insert(0usize, "foo");
    /// # assert_eq!(map.get(&0), Some("foo").as_ref());
    /// ```
    #[inline(always)]
    pub fn with_capacity(capacity: usize) -> Self {
        Self(hb::HashMap::with_capacity_and_hasher(capacity, FixedState))
    }
}

// -----------------------------------------------------------------------------
// Transmute

impl<K, V, S> HashMap<K, V, S> {
    /// Returns the inner [`hashbrown::HashMap`].
    #[inline(always)]
    pub fn into_inner(self) -> hb::HashMap<K, V, S> {
        self.0
    }
}

impl<K, V, S> From<hb::HashMap<K, V, S>> for HashMap<K, V, S> {
    #[inline(always)]
    fn from(value: hb::HashMap<K, V, S>) -> Self {
        Self(value)
    }
}

impl<K, V, S> From<HashMap<K, V, S>> for hb::HashMap<K, V, S> {
    #[inline(always)]
    fn from(value: HashMap<K, V, S>) -> Self {
        value.0
    }
}

// impl<K, V, S> Deref for HashMap<K, V, S> {
//     type Target = hb::HashMap<K, V, S>;
//
//     #[inline(always)]
//     fn deref(&self) -> &Self::Target {
//         &self.0
//     }
// }

// impl<K, V, S> DerefMut for HashMap<K, V, S> {
//     #[inline(always)]
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         &mut self.0
//     }
// }

// -----------------------------------------------------------------------------
// Re-export the underlying method

impl<K, V, S> Clone for HashMap<K, V, S>
where
    hb::HashMap<K, V, S>: Clone,
{
    #[inline(always)]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }

    #[inline(always)]
    fn clone_from(&mut self, source: &Self) {
        self.0.clone_from(&source.0);
    }
}

impl<K, V, S> Debug for HashMap<K, V, S>
where
    hb::HashMap<K, V, S>: Debug,
{
    #[inline(always)]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        <hb::HashMap<K, V, S> as Debug>::fmt(&self.0, f)
    }
}

impl<K, V, S> Default for HashMap<K, V, S>
where
    hb::HashMap<K, V, S>: Default,
{
    #[inline(always)]
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<K, V, S> PartialEq for HashMap<K, V, S>
where
    hb::HashMap<K, V, S>: PartialEq,
{
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

impl<K, V, S> Eq for HashMap<K, V, S> where hb::HashMap<K, V, S>: Eq {}

impl<K, V, S, T> FromIterator<T> for HashMap<K, V, S>
where
    hb::HashMap<K, V, S>: FromIterator<T>,
{
    #[inline(always)]
    fn from_iter<U: IntoIterator<Item = T>>(iter: U) -> Self {
        Self(FromIterator::from_iter(iter))
    }
}

impl<K, V, S, T> Index<T> for HashMap<K, V, S>
where
    hb::HashMap<K, V, S>: Index<T>,
{
    type Output = <hb::HashMap<K, V, S> as Index<T>>::Output;

    #[inline(always)]
    fn index(&self, index: T) -> &Self::Output {
        self.0.index(index)
    }
}

impl<K, V, S> IntoIterator for HashMap<K, V, S>
where
    hb::HashMap<K, V, S>: IntoIterator,
{
    type Item = <hb::HashMap<K, V, S> as IntoIterator>::Item;
    type IntoIter = <hb::HashMap<K, V, S> as IntoIterator>::IntoIter;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, K, V, S> IntoIterator for &'a HashMap<K, V, S>
where
    &'a hb::HashMap<K, V, S>: IntoIterator,
{
    type Item = <&'a hb::HashMap<K, V, S> as IntoIterator>::Item;
    type IntoIter = <&'a hb::HashMap<K, V, S> as IntoIterator>::IntoIter;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        (&self.0).into_iter()
    }
}

impl<'a, K, V, S> IntoIterator for &'a mut HashMap<K, V, S>
where
    &'a mut hb::HashMap<K, V, S>: IntoIterator,
{
    type Item = <&'a mut hb::HashMap<K, V, S> as IntoIterator>::Item;
    type IntoIter = <&'a mut hb::HashMap<K, V, S> as IntoIterator>::IntoIter;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        (&mut self.0).into_iter()
    }
}

impl<K, V, S, T> Extend<T> for HashMap<K, V, S>
where
    hb::HashMap<K, V, S>: Extend<T>,
{
    #[inline(always)]
    fn extend<U: IntoIterator<Item = T>>(&mut self, iter: U) {
        self.0.extend(iter);
    }
}

impl<K, V, S> serde_core::Serialize for HashMap<K, V, S>
where
    hb::HashMap<K, V, S>: serde_core::Serialize,
{
    #[inline(always)]
    fn serialize<T>(&self, serializer: T) -> Result<T::Ok, T::Error>
    where
        T: serde_core::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de, K, V, S> serde_core::Deserialize<'de> for HashMap<K, V, S>
where
    hb::HashMap<K, V, S>: serde_core::Deserialize<'de>,
{
    #[inline(always)]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde_core::Deserializer<'de>,
    {
        Ok(Self(serde_core::Deserialize::deserialize(deserializer)?))
    }
}

impl<K, V, S> HashMap<K, V, S> {
    /// Creates an empty [`HashMap`] which will use the given hash builder to hash keys.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// # use zlim_utils::hash::FixedState as SomeHasher;
    ///
    /// let map = HashMap::with_hasher(SomeHasher);
    /// # // doc test
    /// # let mut map = map;
    /// # map.insert(0usize, "foo");
    /// # assert_eq!(map.get(&0), Some("foo").as_ref());
    /// ```
    #[inline(always)]
    pub const fn with_hasher(hash_builder: S) -> Self {
        Self(hb::HashMap::with_hasher(hash_builder))
    }

    /// Creates an empty [`HashMap`] with the specified capacity,
    /// using hash_builder to hash the keys.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// # use zlim_utils::hash::FixedState as SomeHasher;
    ///
    /// let map = HashMap::with_capacity_and_hasher(5, SomeHasher);
    /// # // doc test
    /// # let mut map = map;
    /// # map.insert(0usize, "foo");
    /// # assert_eq!(map.get(&0), Some("foo").as_ref());
    /// ```
    #[inline(always)]
    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Self {
        Self(hb::HashMap::with_capacity_and_hasher(
            capacity,
            hash_builder,
        ))
    }

    /// Returns a reference to the map's [`BuildHasher`].
    #[inline(always)]
    pub fn hasher(&self) -> &S {
        self.0.hasher()
    }

    /// Returns the number of elements the map can hold without reallocating.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let map = HashMap::with_capacity(5);
    ///
    /// # // doc test
    /// # let map: HashMap<(), ()> = map;
    /// # assert!(map.capacity() >= 5);
    /// ```
    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    /// An iterator visiting all keys in arbitrary order.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// #
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    /// map.insert("bar", 1);
    /// map.insert("baz", 2);
    ///
    /// for key in map.keys() {
    ///     // foo, bar, baz (arbitrary order)
    /// }
    /// # assert_eq!(map.keys().count(), 3);
    /// ```
    #[inline(always)]
    pub fn keys(&self) -> Keys<'_, K, V> {
        self.0.keys()
    }

    /// An iterator visiting all values in arbitrary order.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// #
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    /// map.insert("bar", 1);
    /// map.insert("baz", 2);
    ///
    /// for key in map.values() {
    ///     // 0, 1, 2 (arbitrary order)
    /// }
    /// # assert_eq!(map.values().count(), 3);
    /// ```
    #[inline(always)]
    pub fn values(&self) -> Values<'_, K, V> {
        self.0.values()
    }

    /// An iterator visiting all values mutably in arbitrary order.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// #
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    /// map.insert("bar", 1);
    /// map.insert("baz", 2);
    ///
    /// for key in map.values_mut() {
    ///     // 0, 1, 2 (arbitrary order)
    /// }
    /// # assert_eq!(map.values_mut().count(), 3);
    /// ```
    #[inline(always)]
    pub fn values_mut(&mut self) -> ValuesMut<'_, K, V> {
        self.0.values_mut()
    }

    /// An iterator visiting all key-value pairs in arbitrary order.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// #
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    /// map.insert("bar", 1);
    /// map.insert("baz", 2);
    ///
    /// for (key, value) in map.iter() {
    ///     // ("foo", 0), ("bar", 1), ("baz", 2) (arbitrary order)
    /// }
    /// # assert_eq!(map.iter().count(), 3);
    /// ```
    #[inline(always)]
    pub fn iter(&self) -> Iter<'_, K, V> {
        self.0.iter()
    }

    /// An iterator visiting all key-value pairs in arbitrary order, with mutable references to the values.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// #
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    /// map.insert("bar", 1);
    /// map.insert("baz", 2);
    ///
    /// for (key, value) in map.iter_mut() {
    ///     // ("foo", 0), ("bar", 1), ("baz", 2) (arbitrary order)
    /// }
    /// # assert_eq!(map.iter_mut().count(), 3);
    /// ```
    #[inline(always)]
    pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
        self.0.iter_mut()
    }

    /// Returns the number of elements in the map.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::new();
    ///
    /// assert_eq!(map.len(), 0);
    ///
    /// map.insert("foo", 0);
    ///
    /// assert_eq!(map.len(), 1);
    /// ```
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if the map contains no elements.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::new();
    ///
    /// assert!(map.is_empty());
    ///
    /// map.insert("foo", 0);
    ///
    /// assert!(!map.is_empty());
    /// ```
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Clears the map, returning all key-value pairs as an iterator. Keeps the allocated memory for reuse.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// #
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    /// map.insert("bar", 1);
    /// map.insert("baz", 2);
    ///
    /// for (key, value) in map.drain() {
    ///     // ("foo", 0), ("bar", 1), ("baz", 2) (arbitrary order)
    /// }
    ///
    /// assert!(map.is_empty());
    /// ```
    #[inline(always)]
    pub fn drain(&mut self) -> Drain<'_, K, V> {
        self.0.drain()
    }

    /// Retains only the elements specified by the predicate. Keeps the allocated memory for reuse.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// #
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    /// map.insert("bar", 1);
    /// map.insert("baz", 2);
    ///
    /// map.retain(|key, value| *value == 2);
    ///
    /// assert_eq!(map.len(), 1);
    /// ```
    #[inline(always)]
    pub fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&K, &mut V) -> bool,
    {
        self.0.retain(f);
    }

    /// Drains elements which are true under the given predicate, and returns an iterator over the removed items.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// #
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    /// map.insert("bar", 1);
    /// map.insert("baz", 2);
    ///
    /// let extracted = map
    ///     .extract_if(|key, value| *value == 2)
    ///     .collect::<Vec<_>>();
    ///
    /// assert_eq!(map.len(), 2);
    /// assert_eq!(extracted.len(), 1);
    /// ```
    #[inline(always)]
    pub fn extract_if<F>(&mut self, f: F) -> ExtractIf<'_, K, V, F>
    where
        F: FnMut(&K, &mut V) -> bool,
    {
        self.0.extract_if(f)
    }

    /// Clears the map, removing all key-value pairs. Keeps the allocated memory for reuse.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// #
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    /// map.insert("bar", 1);
    /// map.insert("baz", 2);
    ///
    /// map.clear();
    ///
    /// assert!(map.is_empty());
    /// ```
    #[inline(always)]
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Creates a consuming iterator visiting all the keys in arbitrary order.
    /// The map cannot be used after calling this. The iterator element type is K.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// #
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    /// map.insert("bar", 1);
    /// map.insert("baz", 2);
    ///
    /// for key in map.into_keys() {
    ///     // "foo", "bar", "baz" (arbitrary order)
    /// }
    /// ```
    #[inline(always)]
    pub fn into_keys(self) -> IntoKeys<K, V> {
        self.0.into_keys()
    }

    /// Creates a consuming iterator visiting all the values in arbitrary order.
    /// The map cannot be used after calling this. The iterator element type is V.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// #
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    /// map.insert("bar", 1);
    /// map.insert("baz", 2);
    ///
    /// for key in map.into_values() {
    ///     // 0, 1, 2 (arbitrary order)
    /// }
    /// ```
    #[inline(always)]
    pub fn into_values(self) -> IntoValues<K, V> {
        self.0.into_values()
    }
}

impl<K, V, S> HashMap<K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    /// Reserves capacity for at least additional more elements to be inserted in the HashMap.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::with_capacity(5);
    ///
    /// # let mut map: HashMap<(), ()> = map;
    /// #
    /// assert!(map.capacity() >= 5);
    ///
    /// map.reserve(10);
    ///
    /// assert!(map.capacity() - map.len() >= 10);
    /// ```
    #[inline(always)]
    pub fn reserve(&mut self, additional: usize) {
        self.0.reserve(additional);
    }

    /// Tries to reserve capacity for at least additional more elements to be inserted in the given HashMap<K,V>.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::with_capacity(5);
    ///
    /// # let mut map: HashMap<(), ()> = map;
    /// #
    /// assert!(map.capacity() >= 5);
    ///
    /// map.try_reserve(10).expect("Out of Memory!");
    ///
    /// assert!(map.capacity() - map.len() >= 10);
    /// ```
    #[inline(always)]
    pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError> {
        self.0.try_reserve(additional)
    }

    /// Shrinks the capacity of the map as much as possible.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::with_capacity(5);
    ///
    /// map.insert("foo", 0);
    /// map.insert("bar", 1);
    /// map.insert("baz", 2);
    ///
    /// assert!(map.capacity() >= 5);
    ///
    /// map.shrink_to_fit();
    ///
    /// assert_eq!(map.capacity(), 3);
    /// ```
    #[inline(always)]
    pub fn shrink_to_fit(&mut self) {
        self.0.shrink_to_fit();
    }

    /// Shrinks the capacity of the map with a lower limit.
    ///
    /// It will drop down no lower than the supplied limit while maintaining the internal rules
    /// and possibly leaving some space in accordance with the resize policy.
    #[inline(always)]
    pub fn shrink_to(&mut self, min_capacity: usize) {
        self.0.shrink_to(min_capacity);
    }

    /// Gets the given key's corresponding entry in the map for in-place manipulation.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::new();
    ///
    /// let value = map.entry("foo").or_insert(0);
    /// #
    /// # assert_eq!(*value, 0);
    /// ```
    #[inline(always)]
    pub fn entry(&mut self, key: K) -> Entry<'_, K, V, S> {
        self.0.entry(key)
    }

    /// Gets the given key's corresponding entry by reference in the map for in-place manipulation.
    ///
    /// Refer to [`entry_ref`](hb::HashMap::entry_ref) for further details.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::<String, usize>::new();
    ///
    /// let value = map.entry_ref("foo").or_insert(0);
    /// #
    /// # assert_eq!(*value, 0);
    /// ```
    #[inline(always)]
    pub fn entry_ref<'a, 'b, Q>(&'a mut self, key: &'b Q) -> EntryRef<'a, 'b, K, Q, V, S>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        self.0.entry_ref(key)
    }

    /// Returns a reference to the value corresponding to the key.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    ///
    /// assert_eq!(map.get("foo"), Some(&0));
    /// ```
    #[inline(always)]
    pub fn get<Q>(&self, k: &Q) -> Option<&V>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        self.0.get(k)
    }

    /// eturns the key-value pair corresponding to the supplied key.
    ///
    /// Refer to [`get_key_value`](hb::HashMap::get_key_value) for further details.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    ///
    /// assert_eq!(map.get_key_value("foo"), Some((&"foo", &0)));
    /// ```
    #[inline(always)]
    pub fn get_key_value<Q>(&self, k: &Q) -> Option<(&K, &V)>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        self.0.get_key_value(k)
    }

    /// Returns the key-value pair corresponding to the supplied key, with a mutable reference to value.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    ///
    /// assert_eq!(map.get_key_value_mut("foo"), Some((&"foo", &mut 0)));
    /// ```
    #[inline(always)]
    pub fn get_key_value_mut<Q>(&mut self, k: &Q) -> Option<(&K, &mut V)>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        self.0.get_key_value_mut(k)
    }

    /// Returns true if the map contains a value for the specified key.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    ///
    /// assert!(map.contains_key("foo"));
    /// ```
    #[inline(always)]
    pub fn contains_key<Q>(&self, k: &Q) -> bool
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        self.0.contains_key(k)
    }

    /// Returns a mutable reference to the value corresponding to the key.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    ///
    /// assert_eq!(map.get_mut("foo"), Some(&mut 0));
    /// ```
    #[inline(always)]
    pub fn get_mut<Q>(&mut self, k: &Q) -> Option<&mut V>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        self.0.get_mut(k)
    }

    /// Attempts to get mutable references to N values in the map at once.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    /// map.insert("bar", 1);
    /// map.insert("baz", 2);
    ///
    /// let result = map.get_disjoint_mut(["foo", "bar"]);
    ///
    /// assert_eq!(result, [Some(&mut 0), Some(&mut 1)]);
    /// ```
    #[inline(always)]
    pub fn get_disjoint_mut<Q, const N: usize>(&mut self, ks: [&Q; N]) -> [Option<&'_ mut V>; N]
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        self.0.get_disjoint_mut(ks)
    }

    /// Attempts to get mutable references to N values in the map at once,
    /// with immutable references to the corresponding keys.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    /// map.insert("bar", 1);
    /// map.insert("baz", 2);
    ///
    /// let result = map.get_disjoint_key_value_mut(["foo", "bar"]);
    ///
    /// assert_eq!(result, [Some((&"foo", &mut 0)), Some((&"bar", &mut 1))]);
    /// ```
    #[inline(always)]
    pub fn get_disjoint_key_value_mut<Q, const N: usize>(
        &mut self,
        ks: [&Q; N],
    ) -> [Option<(&'_ K, &'_ mut V)>; N]
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        self.0.get_disjoint_key_value_mut(ks)
    }

    /// Inserts a key-value pair into the map.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    ///
    /// assert_eq!(map.get("foo"), Some(&0));
    /// ```
    #[inline(always)]
    pub fn insert(&mut self, k: K, v: V) -> Option<V> {
        self.0.insert(k, v)
    }

    /// Tries to insert a key-value pair into the map, and returns a mutable reference to the value in the entry.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::new();
    ///
    /// map.try_insert("foo", 0).unwrap();
    ///
    /// assert!(map.try_insert("foo", 1).is_err());
    /// ```
    #[inline(always)]
    pub fn try_insert(&mut self, key: K, value: V) -> Result<&mut V, OccupiedError<'_, K, V, S>> {
        self.0.try_insert(key, value)
    }

    /// Removes a key from the map, returning the value at the key if the key was previously in the map. Keeps the allocated memory for reuse.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    ///
    /// assert_eq!(map.remove("foo"), Some(0));
    ///
    /// assert!(map.is_empty());
    /// ```
    #[inline(always)]
    pub fn remove<Q>(&mut self, k: &Q) -> Option<V>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        self.0.remove(k)
    }

    /// Removes a key from the map, returning the stored key and value if the key was previously in the map. Keeps the allocated memory for reuse.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::new();
    ///
    /// map.insert("foo", 0);
    ///
    /// assert_eq!(map.remove_entry("foo"), Some(("foo", 0)));
    ///
    /// assert!(map.is_empty());
    /// ```
    #[inline(always)]
    pub fn remove_entry<Q>(&mut self, k: &Q) -> Option<(K, V)>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        self.0.remove_entry(k)
    }

    /// Returns the total amount of memory allocated internally by the hash set, in bytes.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use zlim_utils::hash::HashMap;
    /// let mut map = HashMap::new();
    ///
    /// assert_eq!(map.allocation_size(), 0);
    ///
    /// map.insert("foo", 0u32);
    ///
    /// assert!(map.allocation_size() >= size_of::<&'static str>() + size_of::<u32>());
    /// ```
    #[inline(always)]
    pub fn allocation_size(&self) -> usize {
        self.0.allocation_size()
    }
}
