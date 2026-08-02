use core::fmt::{self, Debug, Formatter};
use std::borrow::Cow;

use zlim_utils::format_smol;

use crate::Reflect;
use crate::info::ReflectKind;
use crate::info::VariantKind;
use crate::ops::ApplyError;
use crate::ops::CloneError;
use crate::ops::Enum;
use crate::ops::Struct;
use crate::ops::Tuple;
use crate::ops::VariantFieldIter;
use crate::path::TypePath;

use super::dynamic_struct::DynamicStruct;
use super::dynamic_tuple::DynamicTuple;
use super::{impl_dynamic_reflect_cast, impl_dynamic_type_info, impl_dynamic_type_path};

// ----------------------------------------------------------------------------
// DynamicVariant
// ----------------------------------------------------------------------------

/// The data carried by an enum variant in a [`DynamicEnum`].
///
/// Corresponds to the three [`VariantKind`] possibilities:
///
/// | Variant | Kind | Data |
/// |---------|------|------|
/// | `Unit` | [`VariantKind::Unit`] | None |
/// | `Tuple(DynamicTuple)` | [`VariantKind::Tuple`] | Unnamed fields |
/// | `Struct(DynamicStruct)` | [`VariantKind::Struct`] | Named fields |
#[derive(Debug)]
pub enum DynamicVariant {
    /// A unit variant with no data.
    Unit,
    /// A tuple variant with unnamed, indexed fields.
    Tuple(DynamicTuple),
    /// A struct variant with named fields.
    Struct(DynamicStruct),
}

impl From<()> for DynamicVariant {
    #[inline]
    fn from(_: ()) -> Self {
        Self::Unit
    }
}

impl From<DynamicTuple> for DynamicVariant {
    #[inline]
    fn from(value: DynamicTuple) -> Self {
        Self::Tuple(value)
    }
}

impl From<DynamicStruct> for DynamicVariant {
    #[inline]
    fn from(value: DynamicStruct) -> Self {
        Self::Struct(value)
    }
}

// ----------------------------------------------------------------------------
// DynamicEnum
// ----------------------------------------------------------------------------

/// A dynamic representation of an enum variant.
///
/// Just as a Rust enum can only be one variant at a time, `DynamicEnum`
/// stores exactly one variant's data.
///
/// # Conversion
///
/// `DynamicEnum` can be constructed from any type that implements
/// [`Enum`](crate::ops::Enum) via [`from_ref`](Self::from_ref), and
/// can be converted back to a concrete enum type via
/// [`from_reflect`](crate::Reflect::from_reflect).
///
/// # Examples
///
/// ```no_run
/// use zlim_reflect::dynamic::{DynamicEnum, DynamicTuple, DynamicVariant};
/// use zlim_reflect::ops::Enum;
///
/// // Create a unit variant.
/// let dyn_enum = DynamicEnum::new(0, "None", DynamicVariant::Unit);
/// assert_eq!(dyn_enum.variant_name(), "None");
///
/// // Create a tuple variant with concrete values.
/// let mut tuple = DynamicTuple::new();
/// tuple.push(Box::new(42i32));
/// let dyn_enum = DynamicEnum::new(0, "Some", DynamicVariant::Tuple(tuple));
/// assert_eq!(dyn_enum.field_len(), 1);
/// ```
pub struct DynamicEnum {
    variant_index: usize,
    variant_name: Cow<'static, str>,
    variant: DynamicVariant,
}

impl_dynamic_type_path!(DynamicEnum);
impl_dynamic_type_info!(DynamicEnum);

impl DynamicEnum {
    /// Creates a new `DynamicEnum` representing a single variant.
    #[inline]
    pub fn new<I, V>(index: usize, name: I, variant: V) -> Self
    where
        I: Into<Cow<'static, str>>,
        V: Into<DynamicVariant>,
    {
        Self {
            variant_index: index,
            variant_name: name.into(),
            variant: variant.into(),
        }
    }

    /// Replaces the current variant with new data.
    #[inline]
    pub fn reset<I, V>(&mut self, index: usize, name: I, variant: V)
    where
        I: Into<Cow<'static, str>>,
        V: Into<DynamicVariant>,
    {
        self.variant_index = index;
        self.variant_name = name.into();
        self.variant = variant.into();
    }

    /// Returns a reference to the current variant's data.
    #[inline]
    pub fn variant(&self) -> &DynamicVariant {
        &self.variant
    }

    /// Returns a mutable reference to the current variant's data.
    ///
    /// Switching to a different variant via this reference does not update
    /// the variant name or index. Use [`reset`](Self::reset) instead.
    #[inline]
    pub fn variant_mut(&mut self) -> &mut DynamicVariant {
        &mut self.variant
    }

    /// Constructs a `DynamicEnum` by cloning from an [`Enum`] reference.
    ///
    /// This is the fallible version of [`Reflect::reflect_clone`]; it
    /// propagates any [`CloneError`] from inner fields.
    #[inline(never)]
    pub fn from_ref(value: &dyn Enum) -> Result<Self, CloneError> {
        let index = value.variant_index();
        let name = value.variant_name().to_owned();

        let dyn_enum = match value.variant_kind() {
            VariantKind::Unit => DynamicEnum::new(index, name, ()),
            VariantKind::Tuple => {
                let mut data = DynamicTuple::with_capacity(value.field_len());
                for field in value.iter_fields() {
                    data.push(field.reflect_clone()?);
                }
                DynamicEnum::new(index, name, data)
            }
            VariantKind::Struct => {
                let mut data = DynamicStruct::with_capacity(value.field_len());
                for (i, field) in value.iter_fields().enumerate() {
                    let n = value.field_name_at(i).expect("named variant");
                    let field_name: Cow<'static, str> = n.to_owned().into();
                    data.push(field_name, field.reflect_clone()?);
                }
                DynamicEnum::new(index, name, data)
            }
        };

        Ok(dyn_enum)
    }

    /// Fallible clone that propagates [`CloneError`] from inner fields.
    ///
    /// Unlike [`reflect_clone`](Reflect::reflect_clone) which panics on
    /// failure, this returns `Err` so callers can inspect the error.
    pub fn try_clone(&self) -> Result<Self, CloneError> {
        let index = self.variant_index;
        let name = self.variant_name.clone();
        match &self.variant {
            DynamicVariant::Unit => Ok(Self::new(index, name, ())),
            DynamicVariant::Tuple(d) => Ok(Self::new(index, name, d.try_clone()?)),
            DynamicVariant::Struct(d) => Ok(Self::new(index, name, d.try_clone()?)),
        }
    }
}

// ----------------------------------------------------------------------------
// Reflect
// ----------------------------------------------------------------------------

impl Reflect for DynamicEnum {
    impl_dynamic_reflect_cast!(Enum);

    #[inline]
    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        self.try_clone().map(|x| Box::new(x) as Box<dyn Reflect>)
    }

    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        if let Err(value) = crate::impls::enum_try_apply(self, value)? {
            let index = value.variant_index();
            let name = value.variant_name();

            // Variant name mismatch — switch to the new variant.
            let dyn_variant: DynamicVariant = match value.variant_kind() {
                VariantKind::Unit => DynamicVariant::Unit,
                VariantKind::Tuple => {
                    let mut t = DynamicTuple::with_capacity(value.field_len());
                    for (i, f) in value.iter_fields().enumerate() {
                        match f.reflect_clone() {
                            Ok(cloned) => t.push(cloned),
                            Err(e) => {
                                ::core::hint::cold_path();
                                let src = <Self as TypePath>::type_path();
                                let apply = value.reflect_type_path();
                                let item = format_smol!("{i}");
                                return Err(ApplyError::clone_error(src, apply, &item, e));
                            }
                        }
                    }
                    DynamicVariant::Tuple(t)
                }
                VariantKind::Struct => {
                    let mut s = DynamicStruct::with_capacity(value.field_len());
                    for (i, f) in value.iter_fields().enumerate() {
                        let n = value.field_name_at(i).expect("valid index");
                        let name: Cow<'static, str> = n.to_owned().into();
                        match f.reflect_clone() {
                            Ok(cloned) => s.push(name, cloned),
                            Err(e) => {
                                ::core::hint::cold_path();
                                let src = <Self as TypePath>::type_path();
                                let apply = value.reflect_type_path();
                                return Err(ApplyError::clone_error(src, apply, &name, e));
                            }
                        }
                    }
                    DynamicVariant::Struct(s)
                }
            };

            self.reset(index, name.to_owned(), dyn_variant);
        }
        Ok(())
    }

    #[inline]
    fn reflect_hash(&self) -> u64 {
        crate::impls::enum_hash(self)
    }

    #[inline]
    fn reflect_eq(&self, other: &dyn Reflect) -> bool {
        crate::impls::enum_eq(self, other)
    }

    #[inline]
    fn reflect_debug(&self, f: &mut Formatter) -> fmt::Result {
        crate::impls::enum_debug(self, f)
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>>
    where
        Self: Sized,
    {
        // Fast path: already a DynamicEnum.
        if value.is::<Self>() {
            return value.downcast::<Self>();
        }

        if value.reflect_kind() != ReflectKind::Enum {
            return Err(value);
        }

        let value = value.reflect_owned().into_enum().unwrap();
        let index = value.variant_index();
        let name = value.variant_name().to_owned();
        match value.variant_kind() {
            VariantKind::Unit => Ok(Box::new(Self::new(index, name, ()))),
            VariantKind::Tuple => {
                let vals = value.unpack();
                let fields: Vec<_> = vals.into_iter().map(|(_, v)| v).collect();
                let dynamic = DynamicTuple { fields };
                Ok(Box::new(Self::new(index, name, dynamic)))
            }
            VariantKind::Struct => {
                let vals = value.unpack();
                let fields: Vec<_> = vals
                    .into_iter()
                    .map(|(n, v)| (n.expect("struct variant should have field names"), v))
                    .collect();
                let dynamic = DynamicStruct { fields };
                Ok(Box::new(Self::new(index, name, dynamic)))
            }
        }
    }
}

impl Debug for DynamicEnum {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        crate::impls::enum_debug(self, f)
    }
}

// ----------------------------------------------------------------------------
// Enum
// ----------------------------------------------------------------------------

impl Enum for DynamicEnum {
    #[inline]
    fn field(&self, name: &str) -> Option<&dyn Reflect> {
        match &self.variant {
            DynamicVariant::Struct(data) => data.field(name),
            _ => None,
        }
    }

    #[inline]
    fn field_at(&self, index: usize) -> Option<&dyn Reflect> {
        match &self.variant {
            DynamicVariant::Tuple(data) => data.field(index),
            DynamicVariant::Struct(data) => data.field_at(index),
            DynamicVariant::Unit => None,
        }
    }

    #[inline]
    fn field_mut(&mut self, name: &str) -> Option<&mut dyn Reflect> {
        match &mut self.variant {
            DynamicVariant::Struct(data) => data.field_mut(name),
            _ => None,
        }
    }

    #[inline]
    fn field_at_mut(&mut self, index: usize) -> Option<&mut dyn Reflect> {
        match &mut self.variant {
            DynamicVariant::Tuple(data) => data.field_mut(index),
            DynamicVariant::Struct(data) => data.field_at_mut(index),
            DynamicVariant::Unit => None,
        }
    }

    #[inline]
    fn field_name_at(&self, index: usize) -> Option<&str> {
        match &self.variant {
            DynamicVariant::Struct(data) => data.name_at(index),
            _ => None,
        }
    }

    #[inline]
    fn field_index_of(&self, name: &str) -> Option<usize> {
        match &self.variant {
            DynamicVariant::Struct(data) => data.index_of(name),
            _ => None,
        }
    }

    #[inline]
    fn field_len(&self) -> usize {
        match &self.variant {
            DynamicVariant::Unit => 0,
            DynamicVariant::Tuple(data) => data.field_len(),
            DynamicVariant::Struct(data) => data.field_len(),
        }
    }

    #[inline]
    fn iter_fields(&self) -> VariantFieldIter<'_> {
        VariantFieldIter::new(self)
    }

    #[inline]
    fn variant_kind(&self) -> VariantKind {
        match &self.variant {
            DynamicVariant::Unit => VariantKind::Unit,
            DynamicVariant::Tuple(..) => VariantKind::Tuple,
            DynamicVariant::Struct(..) => VariantKind::Struct,
        }
    }

    #[inline]
    fn variant_index(&self) -> usize {
        self.variant_index
    }

    #[inline]
    fn variant_name(&self) -> &str {
        &self.variant_name
    }

    fn unpack(self: Box<Self>) -> Vec<(Option<Cow<'static, str>>, Box<dyn Reflect>)> {
        match self.variant {
            DynamicVariant::Unit => Vec::new(),
            DynamicVariant::Tuple(t) => t.fields.into_iter().map(|v| (None, v)).collect(),
            DynamicVariant::Struct(s) => s
                .fields
                .into_iter()
                .map(|(name, val)| (Some(name), val))
                .collect(),
        }
    }
}
