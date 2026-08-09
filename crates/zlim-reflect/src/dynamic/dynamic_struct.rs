use core::fmt::{self, Debug, Formatter};
use std::borrow::Cow;

use crate::Reflect;
use crate::info::ReflectKind;
use crate::ops::ApplyError;
use crate::ops::CloneError;
use crate::ops::Struct;
use crate::ops::StructFieldIter;

use super::{impl_dynamic_reflect_cast, impl_dynamic_type_info, impl_dynamic_type_path};

// -----------------------------------------------------------------------------
// DynamicStruct
// -----------------------------------------------------------------------------

/// A dynamic container representing a struct with named fields.
///
/// Fields are stored as `(name, value)` pairs in insertion order.
/// Lookup by name is O(n).
///
/// # Conversion
///
/// `DynamicStruct` can be constructed from any type that implements
/// [`Struct`](crate::ops::Struct) via [`from_ref`](Self::from_ref), and
/// can be converted back to a concrete type via
/// [`from_reflect`](crate::Reflect::from_reflect).
///
/// # Examples
///
/// ```no_run
/// use zlim_reflect::dynamic::DynamicStruct;
/// use zlim_reflect::ops::Struct;
///
/// // Construct from standard reflected types.
/// let mut dynamic = DynamicStruct::new();
/// dynamic.push("x".into(), Box::new(42i32));
/// dynamic.push("name".into(), Box::new("hello"));
///
/// assert_eq!(dynamic.field_len(), 2);
/// assert!(dynamic.field("x").is_some());
///
/// // Convert from any Struct-implementing type.
/// // let s: &dyn Struct = &my_custom_struct;
/// // let cloned = DynamicStruct::from_ref(s).unwrap();
/// ```
#[derive(Default)]
pub struct DynamicStruct {
    pub(super) fields: Vec<(Cow<'static, str>, Box<dyn Reflect>)>,
}

impl_dynamic_type_path!(DynamicStruct);
impl_dynamic_type_info!(DynamicStruct);

impl DynamicStruct {
    /// Creates an empty `DynamicStruct`.
    #[inline]
    pub const fn new() -> Self {
        Self { fields: Vec::new() }
    }

    /// Creates an empty `DynamicStruct` with at least the specified capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            fields: Vec::with_capacity(capacity),
        }
    }

    /// Inserts or replaces a field.
    ///
    /// If a field with the given `name` already exists, its value is
    /// replaced. Otherwise the field is appended.
    #[inline]
    pub fn insert(&mut self, name: Cow<'static, str>, value: Box<dyn Reflect>) {
        if let Some((_, existing)) = self.fields.iter_mut().find(|(n, _)| *n == name) {
            *existing = value;
        } else {
            self.fields.push((name, value));
        }
    }

    /// Inserts a field.
    ///
    /// This does not check for repeatability.
    #[inline]
    pub fn push(&mut self, name: Cow<'static, str>, value: Box<dyn Reflect>) {
        self.fields.push((name, value));
    }

    /// Constructs a `DynamicStruct` by cloning each field from a [`Struct`].
    ///
    /// This is the fallible version of [`Reflect::reflect_clone`]; it
    /// propagates any [`CloneError`] from inner fields.
    #[inline(never)]
    pub fn from_ref(value: &dyn Struct) -> Result<Self, CloneError> {
        let hint = value.field_len();
        let mut fields = Vec::with_capacity(hint);
        for (i, f) in value.iter_fields().enumerate() {
            let name = value.name_at(i).unwrap();
            fields.push((Cow::Owned(name.to_owned()), f.reflect_clone()?));
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
        for (name, value) in self.fields.iter() {
            let val = value.reflect_clone()?;
            fields.push((name.clone(), val));
        }
        Ok(Self { fields })
    }
}

// -----------------------------------------------------------------------------
// Reflect
// -----------------------------------------------------------------------------

impl Reflect for DynamicStruct {
    impl_dynamic_reflect_cast!(Struct);

    #[inline]
    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        self.try_clone().map(|x| Box::new(x) as Box<dyn Reflect>)
    }

    #[inline]
    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        crate::impls::struct_apply(self, value)
    }

    #[inline]
    fn reflect_hash(&self) -> u64 {
        crate::impls::struct_hash(self)
    }

    #[inline]
    fn reflect_eq(&self, other: &dyn Reflect) -> bool {
        crate::impls::struct_eq(self, other)
    }

    #[inline]
    fn reflect_debug(&self, f: &mut Formatter) -> fmt::Result {
        crate::impls::struct_debug(self, f)
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>>
    where
        Self: Sized,
    {
        // Fast path: already a DynamicStruct.
        if value.is::<Self>() {
            return value.downcast::<Self>();
        }

        if value.reflect_kind() != ReflectKind::Struct {
            return Err(value);
        }

        let value = value.reflect_owned().into_struct().unwrap();

        Ok(Box::new(Self {
            fields: value.unpack(),
        }))
    }
}

impl Debug for DynamicStruct {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        crate::impls::struct_debug(self, f)
    }
}

// -----------------------------------------------------------------------------
// Struct
// -----------------------------------------------------------------------------

impl Struct for DynamicStruct {
    fn field(&self, name: &str) -> Option<&dyn Reflect> {
        self.fields
            .iter()
            .find(|(n, _)| n.as_ref() == name)
            .map(|(_, v)| &**v)
    }

    fn field_mut(&mut self, name: &str) -> Option<&mut dyn Reflect> {
        self.fields
            .iter_mut()
            .find(|(n, _)| n.as_ref() == name)
            .map(|(_, v)| &mut **v)
    }

    fn field_at(&self, index: usize) -> Option<&dyn Reflect> {
        self.fields.get(index).map(|(_, v)| &**v)
    }

    fn field_at_mut(&mut self, index: usize) -> Option<&mut dyn Reflect> {
        self.fields.get_mut(index).map(|(_, v)| &mut **v)
    }

    fn name_at(&self, index: usize) -> Option<&str> {
        self.fields.get(index).map(|(n, _)| n.as_ref())
    }

    fn index_of(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|(n, _)| n.as_ref() == name)
    }

    fn field_len(&self) -> usize {
        self.fields.len()
    }

    fn iter_fields(&self) -> StructFieldIter<'_> {
        StructFieldIter::new(self)
    }

    fn unpack(self: Box<Self>) -> Vec<(Cow<'static, str>, Box<dyn Reflect>)> {
        self.fields
    }
}
