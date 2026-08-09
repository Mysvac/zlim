use core::fmt::{Debug, Formatter};

use super::Reflect;

// -----------------------------------------------------------------------------
// Map trait

/// A reflection trait for map-like types.
///
/// e.g. `HashMap<K, V>`, `BTreeMap<K, V>`
///
/// # Examples
///
/// ## Looking up values and iterating
///
/// ```
/// use std::collections::BTreeMap;
/// use zlim_reflect::ops::{Reflect, Map};
///
/// let map: BTreeMap<i32, i32> = BTreeMap::from([(1, 10), (2, 20), (3, 30)]);
/// let m: &dyn Map = &map;
///
/// assert_eq!(m.entry_len(), 3);
///
/// // Look up an existing key.
/// let key: Box<dyn Reflect> = Box::new(2i32);
/// let val = m.value(&*key).unwrap();
/// assert_eq!(*val.downcast_ref::<i32>().unwrap(), 20);
///
/// // Missing key returns None.
/// let missing: Box<dyn Reflect> = Box::new(99i32);
/// assert!(m.value(&*missing).is_none());
///
/// // Iteration follows key order for BTreeMap.
/// let keys: Vec<i32> = m
///     .iter_entries()
///     .map(|(k, _)| *k.downcast_ref::<i32>().unwrap())
///     .collect();
/// assert_eq!(keys, vec![1, 2, 3]);
/// ```
///
/// ## Mutating values
///
/// ```
/// use std::collections::BTreeMap;
/// use zlim_reflect::ops::{Reflect, Map};
///
/// let mut map: BTreeMap<i32, i32> = BTreeMap::from([(1, 10), (2, 20)]);
/// let m: &mut dyn Map = &mut map;
///
/// // Mutate a value in-place.
/// let key: Box<dyn Reflect> = Box::new(1i32);
/// let val = m.value_mut(&*key).unwrap();
/// *val.downcast_mut::<i32>().unwrap() = 100;
///
/// // Insert a new entry — returns `Ok(false)` because no old value was replaced.
/// assert_eq!(m.insert_entry(Box::new(3i32), Box::new(30i32)), Ok(false));
/// assert_eq!(m.entry_len(), 3);
///
/// // Insert with an existing key — returns `Ok(true)` because the old value was updated.
/// assert_eq!(m.insert_entry(Box::new(3i32), Box::new(99i32)), Ok(true));
///
/// // Remove an entry.
/// let key: Box<dyn Reflect> = Box::new(2i32);
/// let removed = m.remove_entry(&*key).unwrap();
/// assert_eq!(*removed.downcast_ref::<i32>().unwrap(), 20);
/// assert_eq!(m.entry_len(), 2);
/// ```
///
/// ## Bulk operations
///
/// ```
/// use std::collections::BTreeMap;
/// use zlim_reflect::ops::{Reflect, Map};
///
/// let mut map: BTreeMap<i32, i32> = BTreeMap::from([(1, 10), (2, 20), (3, 30), (4, 40)]);
/// let m: &mut dyn Map = &mut map;
///
/// // Keep only entries where the key is odd.
/// m.retain_entry(&mut |k, _v| k.downcast_ref::<i32>().unwrap() % 2 != 0);
/// assert_eq!(m.entry_len(), 2);
///
/// // Drain all remaining entries.
/// let drained: Vec<(Box<dyn Reflect>, Box<dyn Reflect>)> = m.drain_all();
/// assert_eq!(drained.len(), 2);
/// assert_eq!(m.entry_len(), 0);
///
/// // Drained entries retain their key-value pairing.
/// let k = *drained[0].0.downcast_ref::<i32>().unwrap();
/// assert!(k == 1 || k == 3);
/// ```
pub trait Map: Reflect {
    /// Returns a reference to the value associated with `key`.
    fn value(&self, key: &dyn Reflect) -> Option<&dyn Reflect>;

    /// Returns a mutable reference to the value associated with `key`.
    fn value_mut(&mut self, key: &dyn Reflect) -> Option<&mut dyn Reflect>;

    /// Returns the number of key-value pairs in the map.
    fn entry_len(&self) -> usize;

    /// Returns a boxed iterator over `(key, value)` pairs.
    fn iter_entries(&self) -> Box<dyn Iterator<Item = (&dyn Reflect, &dyn Reflect)> + '_>;

    /// Inserts a key-value pair. Returns `Ok(true)` if an existing entry was
    /// updated, `Ok(false)` if the key was newly inserted.
    ///
    /// Returns `Err((key, value))` if the key or value type is not
    /// compatible.
    fn insert_entry(
        &mut self,
        key: Box<dyn Reflect>,
        value: Box<dyn Reflect>,
    ) -> Result<bool, (Box<dyn Reflect>, Box<dyn Reflect>)>;

    /// Removes and returns the value associated with `key`, if present.
    fn remove_entry(&mut self, key: &dyn Reflect) -> Option<Box<dyn Reflect>>;

    /// Retains only the entries for which `f` returns `true`.
    fn retain_entry(&mut self, f: &mut dyn FnMut(&dyn Reflect, &mut dyn Reflect) -> bool);

    /// Removes all entries and returns them as a `Vec`.
    fn drain_all(&mut self) -> Vec<(Box<dyn Reflect>, Box<dyn Reflect>)>;
}

impl Debug for dyn Map {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Map")
            .field(&self.reflect_type_path())
            .finish()
    }
}

// -----------------------------------------------------------------------------
