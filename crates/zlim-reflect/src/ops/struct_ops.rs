use core::fmt::{Debug, Formatter};
use core::iter::FusedIterator;
use std::borrow::Cow;

use super::Reflect;

// -----------------------------------------------------------------------------
// Struct trait

/// A reflection trait for named structs.
///
/// e.g. `Foo{}`, `Bar{ a: i32, b: f32 }`
///
/// Fields can be accessed by name or by declaration-order index.
///
/// # Field Leniency
///
/// [`from_reflect`] and [`reflect_apply`] for structs is **lenient**:
/// extra fields in the source are silently ignored, but existing fields
/// must matched. Fields are matched by name; declaration order does not
/// matter.
///
/// For [`from_reflect`], every field that is *not* annotated with
/// `#[reflect(default)]` must be present in the source.
///
/// # Examples
///
/// ## Accessing fields by name and index
///
/// ```
/// use zlim_reflect::ops::{Struct, Reflect};
///
/// #[derive(Reflect)]
/// struct Point {
///     x: f32,
///     y: f32,
/// }
///
/// let point = Point { x: 1.0, y: 2.0 };
/// let s: &dyn Struct = &point;
///
/// assert_eq!(s.field_len(), 2);
/// assert!(s.field("x").is_some());
/// assert!(s.field("z").is_none());
///
/// // Name-index mapping in declaration order.
/// assert_eq!(s.name_at(0), Some("x"));
/// assert_eq!(s.index_of("y"), Some(1));
/// ```
///
/// ## Mutating fields
///
/// ```
/// use zlim_reflect::ops::{Struct, Reflect};
///
/// #[derive(Reflect)]
/// struct Point {
///     x: f32,
///     y: f32,
/// }
///
/// let mut point = Point { x: 1.0, y: 2.0 };
/// let s: &mut dyn Struct = &mut point;
///
/// let field: &mut dyn Reflect = s.field_mut("x").unwrap();
/// assert!(field.downcast_mut::<f32>().is_some());
///
/// let field: &mut dyn Reflect = s.field_at_mut(1).unwrap();
/// assert!(field.downcast_mut::<f32>().is_some());
/// ```
///
/// ## Iteration and unpacking
///
/// ```
/// use std::borrow::Cow;
/// use zlim_reflect::ops::{Struct, Reflect};
///
/// #[derive(Reflect)]
/// struct Point {
///     x: f32,
///     y: f32,
/// }
///
/// let point = Point { x: 1.0, y: 2.0 };
/// let s: &dyn Struct = &point;
///
/// let mut iter = s.iter_fields();
/// assert_eq!(iter.len(), 2);
/// assert!(iter.next().is_some());
/// assert!(iter.next().is_some());
/// assert!(iter.next().is_none());
///
/// let packed: Box<dyn Struct> = Box::new(point);
/// let fields: Vec<(Cow<'static, str>, Box<dyn Reflect>)> = packed.unpack();
/// assert_eq!(fields.len(), 2);
/// assert_eq!(fields[0].0.as_ref(), "x");
/// assert_eq!(fields[1].0.as_ref(), "y");
/// ```
///
/// [`from_reflect`]: crate::Reflect::from_reflect
/// [`reflect_apply`]: crate::Reflect::reflect_apply
pub trait Struct: Reflect {
    /// Returns a reference to the field with the given name.
    fn field(&self, name: &str) -> Option<&dyn Reflect>;

    /// Returns a mutable reference to the field with the given name.
    fn field_mut(&mut self, name: &str) -> Option<&mut dyn Reflect>;

    /// Returns a reference to the field at `index` (in declaration order).
    fn field_at(&self, index: usize) -> Option<&dyn Reflect>;

    /// Returns a mutable reference to the field at `index`.
    fn field_at_mut(&mut self, index: usize) -> Option<&mut dyn Reflect>;

    /// Returns the name of the field at `index`.
    fn name_at(&self, index: usize) -> Option<&str>;

    /// Returns the declaration-order index of the field with the given
    /// name.
    fn index_of(&self, name: &str) -> Option<usize>;

    /// Returns the number of fields in the struct.
    fn field_len(&self) -> usize;

    /// Returns an iterator over the struct's fields in declaration order.
    fn iter_fields(&self) -> StructFieldIter<'_>;

    /// Consumes the struct and returns its fields as `(name, value)` pairs.
    fn unpack(self: Box<Self>) -> Vec<(Cow<'static, str>, Box<dyn Reflect>)>;
}

impl Debug for dyn Struct {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Struct")
            .field(&self.reflect_type_path())
            .finish()
    }
}

// -----------------------------------------------------------------------------
// Struct Field Iterator

/// An iterator over the fields of a reflected struct.
pub struct StructFieldIter<'a> {
    data: &'a dyn Struct,
    index: usize,
}

impl<'a> StructFieldIter<'a> {
    /// Creates a new iterator for the given struct.
    #[inline(always)]
    pub const fn new(data: &'a dyn Struct) -> Self {
        StructFieldIter { data, index: 0 }
    }
}

impl<'a> Iterator for StructFieldIter<'a> {
    type Item = &'a dyn Reflect;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let value = self.data.field_at(self.index);
        self.index += value.is_some() as usize;
        value
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let hint = self.data.field_len() - self.index;
        (hint, Some(hint))
    }
}

impl ExactSizeIterator for StructFieldIter<'_> {}
impl FusedIterator for StructFieldIter<'_> {}

// -----------------------------------------------------------------------------
