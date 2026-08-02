use core::fmt::{Debug, Formatter};

use super::Reflect;

// ----------------------------------------------------------------------------
// Set trait

/// A reflection trait for set-like types.
///
/// e.g. `HashSet<T>` and `BTreeSet<T>`.
///
/// # Examples
///
/// ## Looking up values and iterating
///
/// ```
/// use std::collections::BTreeSet;
/// use zlim_reflect::ops::{Reflect, Set};
///
/// let set: BTreeSet<i32> = BTreeSet::from([1, 2, 3]);
/// let s: &dyn Set = &set;
///
/// assert_eq!(s.value_len(), 3);
///
/// // Look up values.
/// let probe: Box<dyn Reflect> = Box::new(2i32);
/// assert!(s.value(&*probe).is_some());
///
/// let missing: Box<dyn Reflect> = Box::new(99i32);
/// assert!(s.value(&*missing).is_none());
///
/// // Iteration is in natural order for BTreeSet.
/// let values: Vec<i32> = s
///     .iter_values()
///     .map(|v| *v.downcast_ref::<i32>().unwrap())
///     .collect();
/// assert_eq!(values, vec![1, 2, 3]);
/// ```
///
/// ## Inserting and removing values
///
/// ```
/// use std::collections::BTreeSet;
/// use zlim_reflect::ops::{Reflect, Set};
///
/// let mut set: BTreeSet<i32> = BTreeSet::new();
/// let s: &mut dyn Set = &mut set;
///
/// // Insert new values.
/// assert_eq!(s.insert_value(Box::new(42i32)), Ok(true));
/// assert_eq!(s.insert_value(Box::new(7i32)), Ok(true));
/// assert_eq!(s.value_len(), 2);
///
/// // Duplicate insertion is rejected.
/// assert_eq!(s.insert_value(Box::new(42i32)), Ok(false));
///
/// // Remove a value.
/// let probe: Box<dyn Reflect> = Box::new(7i32);
/// assert!(s.remove_value(&*probe));
/// assert_eq!(s.value_len(), 1);
/// ```
///
/// ## Bulk operations
///
/// ```
/// use std::collections::BTreeSet;
/// use zlim_reflect::ops::{Reflect, Set};
///
/// let mut set: BTreeSet<i32> = BTreeSet::from([1, 2, 3, 4, 5, 6]);
/// let s: &mut dyn Set = &mut set;
///
/// // Keep only even values.
/// s.retain_value(&mut |v| v.downcast_ref::<i32>().unwrap() % 2 == 0);
/// assert_eq!(s.value_len(), 3);
///
/// // Drain all remaining values.
/// let drained: Vec<Box<dyn Reflect>> = s.drain_all();
/// assert_eq!(drained.len(), 3);
/// assert_eq!(s.value_len(), 0);
/// ```
pub trait Set: Reflect {
    /// Returns a reference to the element matching `value`, if present.
    fn value(&self, value: &dyn Reflect) -> Option<&dyn Reflect>;

    /// Returns the number of elements in the set.
    fn value_len(&self) -> usize;

    /// Returns a boxed iterator over the set's elements.
    fn iter_values(&self) -> Box<dyn Iterator<Item = &dyn Reflect> + '_>;

    /// Inserts a value into the set. Returns `Ok(true)` if the value was
    /// newly inserted, `Ok(false)` if it already existed.
    ///
    /// Returns `Err(value)` if the value type is not compatible.
    fn insert_value(&mut self, value: Box<dyn Reflect>) -> Result<bool, Box<dyn Reflect>>;

    /// Removes and returns the element matching `value`, if present.
    fn remove_value(&mut self, value: &dyn Reflect) -> bool;

    /// Retains only the elements for which `f` returns `true`.
    fn retain_value(&mut self, f: &mut dyn FnMut(&dyn Reflect) -> bool);

    /// Removes all elements and returns them as a `Vec`.
    fn drain_all(&mut self) -> Vec<Box<dyn Reflect>>;
}

impl Debug for dyn Set {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Set")
            .field(&self.reflect_type_path())
            .finish()
    }
}

// ----------------------------------------------------------------------------
