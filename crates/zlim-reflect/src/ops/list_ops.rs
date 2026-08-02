use core::fmt::{Debug, Formatter};
use core::iter::FusedIterator;

use super::Reflect;

// ----------------------------------------------------------------------------
// List trait

/// A reflection trait for list-like types.
///
/// e.g. `Vec<T>`, `VecDeque<T>`
///
/// Lists support mutable extension at runtime:
/// elements can be pushed, popped, drained, and iterated.
///
/// # Examples
///
/// ## Accessing and iterating elements
///
/// ```
/// use zlim_reflect::ops::{List, Reflect};
///
/// let mut vec = vec![10i32, 20, 30];
/// let l: &dyn List = &vec;
///
/// assert_eq!(l.item_len(), 3);
/// assert!(l.item(1).is_some());
/// assert!(l.item(5).is_none());
///
/// // Mutate by index.
/// let l: &mut dyn List = &mut vec;
/// *l.item_mut(0).unwrap().downcast_mut::<i32>().unwrap() = 99;
///
/// // Iterate over elements.
/// let values: Vec<i32> = l
///     .iter_items()
///     .map(|v| *v.downcast_ref::<i32>().unwrap())
///     .collect();
/// assert_eq!(values, vec![99, 20, 30]);
/// ```
///
/// ## Pushing and popping
///
/// ```
/// use zlim_reflect::ops::{List, Reflect};
///
/// let mut vec: Vec<i32> = Vec::new();
/// let l: &mut dyn List = &mut vec;
///
/// l.push_back(Box::new(1i32)).unwrap();
/// l.push_back(Box::new(2i32)).unwrap();
/// l.push_front(Box::new(0i32)).unwrap();
/// assert_eq!(l.item_len(), 3);
///
/// let front = l.pop_front().unwrap();
/// assert_eq!(*front.downcast_ref::<i32>().unwrap(), 0);
///
/// let back = l.pop_back().unwrap();
/// assert_eq!(*back.downcast_ref::<i32>().unwrap(), 2);
///
/// assert_eq!(l.item_len(), 1);
/// ```
///
/// ## Draining all elements
///
/// ```
/// use zlim_reflect::ops::{List, Reflect};
///
/// let mut vec: Vec<i32> = vec![1, 2, 3];
/// let l: &mut dyn List = &mut vec;
///
/// let drained: Vec<Box<dyn Reflect>> = l.drain_all();
/// assert_eq!(drained.len(), 3);
/// assert_eq!(l.item_len(), 0);
/// ```
pub trait List: Reflect {
    /// Returns a reference to the element at `index`, or `None` if out of
    /// bounds.
    fn item(&self, index: usize) -> Option<&dyn Reflect>;

    /// Returns a mutable reference to the element at `index`.
    fn item_mut(&mut self, index: usize) -> Option<&mut dyn Reflect>;

    /// Returns the number of elements in the list.
    fn item_len(&self) -> usize;

    /// Returns an iterator over the list's elements.
    fn iter_items(&self) -> ListItemIter<'_>;

    /// Appends an element to the back of the list.
    ///
    /// Returns `Err(value)` if the element type is not compatible.
    fn push_back(&mut self, value: Box<dyn Reflect>) -> Result<(), Box<dyn Reflect>>;

    /// Inserts an element at the front of the list.
    fn push_front(&mut self, value: Box<dyn Reflect>) -> Result<(), Box<dyn Reflect>>;

    /// Removes and returns the last element.
    fn pop_back(&mut self) -> Option<Box<dyn Reflect>>;

    /// Removes and returns the first element.
    fn pop_front(&mut self) -> Option<Box<dyn Reflect>>;

    /// Removes all elements and returns them as a `Vec`.
    fn drain_all(&mut self) -> Vec<Box<dyn Reflect>>;
}

impl Debug for dyn List {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("List")
            .field(&self.reflect_type_path())
            .finish()
    }
}

// ----------------------------------------------------------------------------
// List Iterator

/// An iterator over the elements of a reflected list.
pub struct ListItemIter<'a> {
    data: &'a dyn List,
    index: usize,
}

impl ListItemIter<'_> {
    /// Creates a new iterator for the given list.
    #[inline(always)]
    pub const fn new(data: &dyn List) -> ListItemIter<'_> {
        ListItemIter { data, index: 0 }
    }
}

impl<'a> Iterator for ListItemIter<'a> {
    type Item = &'a dyn Reflect;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let value = self.data.item(self.index);
        self.index += value.is_some() as usize;
        value
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let hint = self.data.item_len() - self.index;
        (hint, Some(hint))
    }
}

impl ExactSizeIterator for ListItemIter<'_> {}
impl FusedIterator for ListItemIter<'_> {}

// ----------------------------------------------------------------------------
