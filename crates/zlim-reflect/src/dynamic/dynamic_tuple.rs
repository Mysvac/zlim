use core::fmt::{self, Debug, Formatter};

use crate::Reflect;
use crate::info::ReflectKind;
use crate::ops::ApplyError;
use crate::ops::CloneError;
use crate::ops::Tuple;
use crate::ops::TupleFieldIter;

use super::{impl_dynamic_reflect_cast, impl_dynamic_type_info, impl_dynamic_type_path};

// ----------------------------------------------------------------------------
// DynamicTuple
// ----------------------------------------------------------------------------

/// A dynamic container representing a tuple or tuple-struct.
///
/// Fields are accessed by index; there are no field names.
///
/// # Conversion
///
/// `DynamicTuple` can be constructed from any type that implements
/// [`Tuple`](crate::ops::Tuple) via [`from_ref`](Self::from_ref), and
/// can be converted back to a concrete type via
/// [`from_reflect`](crate::Reflect::from_reflect).
///
/// # Examples
///
/// ```no_run
/// use zlim_reflect::dynamic::DynamicTuple;
/// use zlim_reflect::ops::Tuple;
///
/// // Construct from standard reflected types.
/// let mut dynamic = DynamicTuple::new();
/// dynamic.push(Box::new(42i32));
/// dynamic.push(Box::new(3.14f32));
///
/// assert_eq!(dynamic.field_len(), 2);
/// assert!(dynamic.field(0).is_some());
///
/// // Convert from any Tuple-implementing type.
/// // let t: &dyn Tuple = &my_tuple_struct;
/// // let cloned = DynamicTuple::from_ref(t).unwrap();
/// ```
#[derive(Default)]
pub struct DynamicTuple {
    pub(super) fields: Vec<Box<dyn Reflect>>,
}

impl_dynamic_type_path!(DynamicTuple);
impl_dynamic_type_info!(DynamicTuple);

impl DynamicTuple {
    /// Creates an empty `DynamicTuple`.
    #[inline]
    pub const fn new() -> Self {
        Self { fields: Vec::new() }
    }

    /// Creates an empty `DynamicTuple` with at least the specified capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            fields: Vec::with_capacity(capacity),
        }
    }

    /// Appends a boxed [`Reflect`] value to the tuple.
    #[inline]
    pub fn push(&mut self, value: Box<dyn Reflect>) {
        self.fields.push(value);
    }

    /// Constructs a `DynamicTuple` by cloning each field from a [`Tuple`].
    ///
    /// This is the fallible version of [`Reflect::reflect_clone`]; it
    /// propagates any [`CloneError`] from inner fields.
    #[inline(never)]
    pub fn from_ref(value: &dyn Tuple) -> Result<Self, CloneError> {
        let hint = value.field_len();
        let mut fields = Vec::with_capacity(hint);
        for f in value.iter_fields() {
            fields.push(f.reflect_clone()?);
        }
        Ok(Self { fields })
    }

    /// Fallible clone that propagates [`CloneError`] from inner fields.
    ///
    /// Unlike [`reflect_clone`](Reflect::reflect_clone) which panics on
    /// failure, this returns `Err` so callers can inspect the error.
    pub fn try_clone(&self) -> Result<Self, CloneError> {
        let hint = self.fields.len();
        let mut fields = Vec::with_capacity(hint);
        for value in self.fields.iter() {
            fields.push(value.reflect_clone()?);
        }
        Ok(Self { fields })
    }
}

// ----------------------------------------------------------------------------
// Reflect
// ----------------------------------------------------------------------------

impl Reflect for DynamicTuple {
    impl_dynamic_reflect_cast!(Tuple);

    #[inline]
    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        self.try_clone().map(|x| Box::new(x) as Box<dyn Reflect>)
    }

    #[inline]
    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        crate::impls::tuple_apply(self, value)
    }

    #[inline]
    fn reflect_hash(&self) -> u64 {
        crate::impls::tuple_hash(self)
    }

    #[inline]
    fn reflect_eq(&self, other: &dyn Reflect) -> bool {
        crate::impls::tuple_eq(self, other)
    }

    #[inline]
    fn reflect_debug(&self, f: &mut Formatter) -> fmt::Result {
        crate::impls::tuple_debug(self, f)
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>>
    where
        Self: Sized,
    {
        // Fast path: already a DynamicTuple.
        if value.is::<Self>() {
            return value.downcast::<Self>();
        }

        if value.reflect_kind() != ReflectKind::Tuple {
            return Err(value);
        }

        let value = value.reflect_owned().into_tuple().unwrap();

        Ok(Box::new(Self {
            fields: value.unpack(),
        }))
    }
}

impl Debug for DynamicTuple {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        crate::impls::tuple_debug(self, f)
    }
}

// ----------------------------------------------------------------------------
// Tuple
// ----------------------------------------------------------------------------

impl Tuple for DynamicTuple {
    fn field(&self, index: usize) -> Option<&dyn Reflect> {
        self.fields.get(index).map(|v| &**v)
    }

    fn field_mut(&mut self, index: usize) -> Option<&mut dyn Reflect> {
        self.fields.get_mut(index).map(|v| &mut **v)
    }

    fn field_len(&self) -> usize {
        self.fields.len()
    }

    fn iter_fields(&self) -> TupleFieldIter<'_> {
        TupleFieldIter::new(self)
    }

    fn unpack(self: Box<Self>) -> Vec<Box<dyn Reflect>> {
        self.fields
    }
}
