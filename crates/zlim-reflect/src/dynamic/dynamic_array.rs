use core::fmt::{self, Debug, Formatter};

use crate::Reflect;
use crate::info::ReflectKind;
use crate::ops::ApplyError;
use crate::ops::Array;
use crate::ops::ArrayItemIter;
use crate::ops::CloneError;

use super::{impl_dynamic_reflect_cast, impl_dynamic_type_info, impl_dynamic_type_path};

// ----------------------------------------------------------------------------
// DynamicArray
// ----------------------------------------------------------------------------

/// A dynamic container representing a fixed-size array.
///
/// Unlike [`DynamicList`](super::DynamicList), `DynamicArray` does not
/// support push/pop — the number of elements is fixed once set, matching
/// the semantics of Rust's `[T; N]` arrays.
///
/// # Conversion
///
/// `DynamicArray` can be constructed from any type that implements
/// [`Array`](crate::ops::Array) via [`from_ref`](Self::from_ref), and
/// can be converted back to a concrete type via
/// [`from_reflect`](crate::Reflect::from_reflect). Array conversion is
/// **strict** — the element count must match exactly.
///
/// # Examples
///
/// ```no_run
/// use zlim_reflect::dynamic::DynamicArray;
/// use zlim_reflect::ops::Array;
///
/// // Construct from standard reflected types.
/// let mut dynamic = DynamicArray::new();
/// dynamic.push(Box::new(1i32));
/// dynamic.push(Box::new(2i32));
/// dynamic.push(Box::new(3i32));
///
/// assert_eq!(dynamic.item_len(), 3);
/// assert!(dynamic.item(1).is_some());
/// ```
#[derive(Default)]
pub struct DynamicArray {
    values: Vec<Box<dyn Reflect>>,
}

impl_dynamic_type_path!(DynamicArray);
impl_dynamic_type_info!(DynamicArray);

impl DynamicArray {
    /// Creates an empty `DynamicArray`.
    #[inline]
    pub const fn new() -> Self {
        Self { values: Vec::new() }
    }

    /// Creates an empty `DynamicArray` with at least the specified capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            values: Vec::with_capacity(capacity),
        }
    }

    /// Appends a boxed [`Reflect`] value to the array.
    #[inline]
    pub fn push(&mut self, value: Box<dyn Reflect>) {
        self.values.push(value);
    }

    /// Constructs a `DynamicArray` by cloning each element from an [`Array`].
    ///
    /// This is the fallible version of [`Reflect::reflect_clone`]; it
    /// propagates any [`CloneError`] from inner elements.
    #[inline(never)]
    pub fn from_ref(value: &dyn Array) -> Result<Self, CloneError> {
        let hint = value.item_len();
        let mut values = Vec::with_capacity(hint);
        for v in value.iter_items() {
            values.push(v.reflect_clone()?);
        }
        Ok(Self { values })
    }

    /// Fallible clone that propagates [`CloneError`] from inner elements.
    ///
    /// Unlike [`reflect_clone`](Reflect::reflect_clone) which panics on
    /// failure, this returns `Err` so callers can inspect the error.
    pub fn try_clone(&self) -> Result<Self, CloneError> {
        let hint = self.values.len();
        let mut values = Vec::with_capacity(hint);
        for v in &self.values {
            values.push(v.reflect_clone()?);
        }
        Ok(Self { values })
    }
}

// ----------------------------------------------------------------------------
// Reflect
// ----------------------------------------------------------------------------

impl Reflect for DynamicArray {
    impl_dynamic_reflect_cast!(Array);

    #[inline]
    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        self.try_clone().map(|x| Box::new(x) as Box<dyn Reflect>)
    }

    #[inline]
    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        crate::impls::array_apply(self, value)
    }

    #[inline]
    fn reflect_hash(&self) -> u64 {
        crate::impls::array_hash(self)
    }

    #[inline]
    fn reflect_eq(&self, other: &dyn Reflect) -> bool {
        crate::impls::array_eq(self, other)
    }

    #[inline]
    fn reflect_debug(&self, f: &mut Formatter) -> fmt::Result {
        crate::impls::array_debug(self, f)
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>>
    where
        Self: Sized,
    {
        if value.is::<Self>() {
            return value.downcast::<Self>();
        }

        if value.reflect_kind() != ReflectKind::Array {
            return Err(value);
        }

        let value = value.reflect_owned().into_array().unwrap();

        Ok(Box::new(Self {
            values: value.unpack(),
        }))
    }
}

impl Debug for DynamicArray {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        crate::impls::array_debug(self, f)
    }
}

// ----------------------------------------------------------------------------
// Array
// ----------------------------------------------------------------------------

impl Array for DynamicArray {
    #[inline]
    fn item(&self, index: usize) -> Option<&dyn Reflect> {
        self.values.get(index).map(|v| &**v)
    }

    #[inline]
    fn item_mut(&mut self, index: usize) -> Option<&mut dyn Reflect> {
        self.values.get_mut(index).map(|v| &mut **v)
    }

    #[inline]
    fn item_len(&self) -> usize {
        self.values.len()
    }

    #[inline]
    fn iter_items(&self) -> ArrayItemIter<'_> {
        ArrayItemIter::new(self)
    }

    #[inline]
    fn unpack(self: Box<Self>) -> Vec<Box<dyn Reflect>> {
        self.values
    }
}
