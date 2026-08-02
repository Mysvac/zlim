use core::fmt::{Debug, Formatter};
use core::iter::FusedIterator;

use super::Reflect;

// ----------------------------------------------------------------------------
// Tuple trait

/// A reflection trait for tuples and tuple-structs.
///
/// e.g. `()`, `(i32, f32)`, `Foo()`, `Bar(i32, String)`
///
/// Fields are accessed by index; there are no field names.
///
/// # Field Strictness
///
/// [`from_reflect`] and [`reflect_apply`] for tuples is **strict**:
/// the source must have the exact same number of fields as the target.
/// Missing or extra fields cause conversion failure (return `Err(_)`).
///
/// # Examples
///
/// ## Accessing fields by index
///
/// ```
/// use zlim_reflect::ops::Tuple;
///
/// let t = (42i32, 3.14f32, "hello");
/// assert_eq!(t.field_len(), 3);
/// assert!(t.field(0).is_some());
/// assert!(t.field(5).is_none());
/// ```
///
/// ## Iterating over fields
///
/// ```
/// use zlim_reflect::ops::Tuple;
///
/// let t = (42i32, 3.14f32);
/// let mut iter = t.iter_fields();
/// assert_eq!(iter.len(), 2);
/// assert!(iter.next().is_some());
/// assert!(iter.next().is_some());
/// assert!(iter.next().is_none());
/// ```
///
/// ## Unpacking into boxed values
///
/// ```
/// use zlim_reflect::ops::{Tuple, Reflect};
///
/// let packed = Box::new((42i32, 3.14f32));
/// let fields: Vec<Box<dyn Reflect>> = packed.unpack();
/// assert_eq!(fields.len(), 2);
/// ```
///
/// [`from_reflect`]: crate::Reflect::from_reflect
/// [`reflect_apply`]: crate::Reflect::reflect_apply
pub trait Tuple: Reflect {
    /// Returns a reference to the field at `index`.
    fn field(&self, index: usize) -> Option<&dyn Reflect>;

    /// Returns a mutable reference to the field at `index`.
    fn field_mut(&mut self, index: usize) -> Option<&mut dyn Reflect>;

    /// Returns the number of fields in the tuple.
    fn field_len(&self) -> usize;

    /// Returns an iterator over the tuple's fields in index order.
    fn iter_fields(&self) -> TupleFieldIter<'_>;

    /// Consumes the tuple and returns its fields as a `Vec`.
    fn unpack(self: Box<Self>) -> Vec<Box<dyn Reflect>>;
}

impl Debug for dyn Tuple {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Tuple")
            .field(&self.reflect_type_path())
            .finish()
    }
}

// ----------------------------------------------------------------------------
// Tuple Field Iterator

/// An iterator over the fields of a reflected tuple.
pub struct TupleFieldIter<'a> {
    data: &'a dyn Tuple,
    index: usize,
}

impl<'a> TupleFieldIter<'a> {
    /// Creates a new iterator for the given tuple.
    #[inline(always)]
    pub const fn new(data: &'a dyn Tuple) -> Self {
        TupleFieldIter { data, index: 0 }
    }
}

impl<'a> Iterator for TupleFieldIter<'a> {
    type Item = &'a dyn Reflect;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let value = self.data.field(self.index);
        self.index += value.is_some() as usize;
        value
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let hint = self.data.field_len() - self.index;
        (hint, Some(hint))
    }
}

impl ExactSizeIterator for TupleFieldIter<'_> {}
impl FusedIterator for TupleFieldIter<'_> {}

// ----------------------------------------------------------------------------
