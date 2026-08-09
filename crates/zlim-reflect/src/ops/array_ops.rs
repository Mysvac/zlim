use core::fmt::{Debug, Formatter};
use core::iter::FusedIterator;

use super::Reflect;

// -----------------------------------------------------------------------------
// Array trait

/// A reflection trait for fixed-size arrays.
///
/// e.g. `[i32; 5]`
///
/// Unlike [`List`], arrays have a compile-time-known length and
/// do not support push/pop operations.
///
/// # Field Strictness
///
/// [`from_reflect`] and [`reflect_apply`] for array is **strict**:
/// the source must have the exact same number of fields as the target.
/// Missing or extra fields cause conversion failure (return `Err(_)`).
///
/// # Examples
///
/// ## Accessing elements by index
///
/// ```
/// use zlim_reflect::ops::{Array, Reflect};
///
/// let mut arr = [10i32, 20, 30];
/// let a: &dyn Array = &arr;
///
/// assert_eq!(a.item_len(), 3);
/// assert!(a.item(0).is_some());
/// assert!(a.item(3).is_none());
///
/// // Mutate by index.
/// let a: &mut dyn Array = &mut arr;
/// *a.item_mut(1).unwrap().downcast_mut::<i32>().unwrap() = 99;
/// let v: &i32 = a.item(1).unwrap().downcast_ref().unwrap();
/// assert_eq!(*v, 99);
/// ```
///
/// ## Iterating and unpacking
///
/// ```
/// use zlim_reflect::ops::{Array, Reflect};
///
/// let arr = [1i32, 2, 3, 4];
/// let a: &dyn Array = &arr;
///
/// // Iterate over elements.
/// let mut iter = a.iter_items();
/// assert_eq!(iter.len(), 4);
/// let sum: i32 = iter.map(|v| *v.downcast_ref::<i32>().unwrap()).sum();
/// assert_eq!(sum, 10);
///
/// // Unpack consumes the array into boxed values.
/// let packed: Box<dyn Array> = Box::new(arr);
/// let items: Vec<Box<dyn Reflect>> = packed.unpack();
/// assert_eq!(items.len(), 4);
/// ```
///
/// [`from_reflect`]: crate::Reflect::from_reflect
/// [`reflect_apply`]: crate::Reflect::reflect_apply
/// [`List`]: crate::ops::List
pub trait Array: Reflect {
    /// Returns a reference to the element at `index`, or `None` if out of
    /// bounds.
    fn item(&self, index: usize) -> Option<&dyn Reflect>;

    /// Returns a mutable reference to the element at `index`.
    fn item_mut(&mut self, index: usize) -> Option<&mut dyn Reflect>;

    /// Returns the number of elements in the array.
    fn item_len(&self) -> usize;

    /// Returns an iterator over the array's elements.
    fn iter_items(&self) -> ArrayItemIter<'_>;

    /// Consumes the array and returns its elements as a `Vec`.
    fn unpack(self: Box<Self>) -> Vec<Box<dyn Reflect>>;
}

impl Debug for dyn Array {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Array")
            .field(&self.reflect_type_path())
            .finish()
    }
}

// -----------------------------------------------------------------------------
// Array Iterator

/// An iterator over the elements of a reflected array.
pub struct ArrayItemIter<'a> {
    array: &'a dyn Array,
    index: usize,
}

impl ArrayItemIter<'_> {
    /// Creates a new iterator for the given array.
    #[inline(always)]
    pub const fn new(array: &dyn Array) -> ArrayItemIter<'_> {
        ArrayItemIter { array, index: 0 }
    }
}

impl<'a> Iterator for ArrayItemIter<'a> {
    type Item = &'a dyn Reflect;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let value = self.array.item(self.index);
        self.index += value.is_some() as usize;
        value
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let hint = self.array.item_len() - self.index;
        (hint, Some(hint))
    }
}

impl ExactSizeIterator for ArrayItemIter<'_> {}
impl FusedIterator for ArrayItemIter<'_> {}

// -----------------------------------------------------------------------------
