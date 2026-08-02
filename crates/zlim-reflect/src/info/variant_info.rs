use core::error::Error;
use core::fmt::{self, Display};

use zlim_utils::mem::Global;

use super::{Attributes, NamedField, UnnamedField, impl_docs_fn};
use super::{impl_attributes_fn, impl_with_attributes};

// ----------------------------------------------------------------------------
// Enum Variant Kind

/// Represents the kind/form of an enum variant.
///
/// # Kinds
///
/// - `A` -> Unit
/// - `A()` and `A(..)` -> Tuple
/// - `A{}` and `A{..}` -> Struct
#[derive(Copy, Clone, PartialEq, Eq)]
#[derive(Debug, PartialOrd, Ord, Hash)]
pub enum VariantKind {
    Unit,
    Tuple,
    Struct,
}

impl Display for VariantKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unit => f.pad("Unit"),
            Self::Tuple => f.pad("Tuple"),
            Self::Struct => f.pad("Struct"),
        }
    }
}

/// A [`VariantKind`]-specific error.
#[derive(Clone, Copy, Debug)]
pub struct VariantKindError {
    /// Expected variant kind.
    pub expected: VariantKind,
    /// Received variant kind.
    pub received: VariantKind,
}

impl Display for VariantKindError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "variant kind mismatch: expected {}, received {}",
            self.expected, self.received
        )
    }
}

impl Error for VariantKindError {}

// ----------------------------------------------------------------------------
// Struct-like variant

/// Information for struct-style enum variants.
#[derive(Clone, Copy, Debug)]
pub struct StructVariantInfo {
    name: &'static str,
    fields: &'static [NamedField],
    // Needed for deserialization.
    field_names: &'static [&'static str],
    attributes: Attributes,
    #[cfg(feature = "reflect_docs")]
    docs: Option<&'static str>,
}

impl StructVariantInfo {
    impl_docs_fn!(docs);
    impl_attributes_fn!(attributes);
    impl_with_attributes!(attributes);

    /// Create a new [`StructVariantInfo`].
    ///
    /// The order of internal fields is fixed, depends on the input order.
    pub fn new(name: &'static str, fields: &[NamedField]) -> Self {
        let field_names: Vec<&'static str> = fields.iter().map(|f| f.name()).collect();

        Self {
            name,
            fields: Global::alloc_slice(fields),
            field_names: Global::alloc_slice(field_names.as_slice()),
            attributes: Attributes::EMPTY,
            #[cfg(feature = "reflect_docs")]
            docs: None,
        }
    }

    /// Returns the name of this variant.
    #[inline]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the underlying fields slice.
    #[inline]
    pub fn fields(&self) -> &'static [NamedField] {
        self.fields
    }

    /// Returns the underlying field names slice.
    #[inline]
    pub fn field_names(&self) -> &'static [&'static str] {
        self.field_names
    }

    /// Returns the number of fields.
    #[inline]
    pub fn field_len(&self) -> usize {
        self.fields.len()
    }

    /// Returns the [`NamedField`] for the given `name`, if present.
    ///
    /// Complexity: O(n) in the number of fields.
    #[inline]
    pub fn field(&self, name: &str) -> Option<&'static NamedField> {
        self.fields.iter().find(|f| f.name() == name)
    }

    /// Returns the [`NamedField`] at the given index, if present.
    ///
    /// Complexity: O(1).
    #[inline]
    pub fn field_at(&self, index: usize) -> Option<&'static NamedField> {
        self.fields.get(index)
    }

    /// Returns the index for the given field `name`, if present.
    ///
    /// Complexity: O(n) in the number of fields.
    #[inline]
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.field_names.iter().position(|n| *n == name)
    }

    /// Returns the name for the given `index`, if present.
    ///
    /// Complexity: O(1).
    #[inline]
    pub fn name_at(&self, index: usize) -> Option<&'static str> {
        self.field_names.get(index).copied()
    }
}

// ----------------------------------------------------------------------------
// Tuple-like variant

/// Information for tuple-style enum variants.
#[derive(Clone, Copy, Debug)]
pub struct TupleVariantInfo {
    name: &'static str,
    fields: &'static [UnnamedField],
    attributes: Attributes,
    #[cfg(feature = "reflect_docs")]
    docs: Option<&'static str>,
}

impl TupleVariantInfo {
    impl_docs_fn!(docs);
    impl_attributes_fn!(attributes);
    impl_with_attributes!(attributes);

    /// Create a new [`TupleVariantInfo`].
    pub fn new(name: &'static str, fields: &[UnnamedField]) -> Self {
        Self {
            name,
            fields: Global::alloc_slice(fields),
            attributes: Attributes::EMPTY,
            #[cfg(feature = "reflect_docs")]
            docs: None,
        }
    }

    /// The name of this variant.
    #[inline]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the underlying fields slice.
    #[inline]
    pub fn fields(&self) -> &'static [UnnamedField] {
        self.fields
    }

    /// Returns the number of fields.
    #[inline]
    pub fn field_len(&self) -> usize {
        self.fields.len()
    }

    /// Returns the [`UnnamedField`] at the given index, if present.
    ///
    /// Complexity: O(1).
    #[inline]
    pub fn field(&self, index: usize) -> Option<&'static UnnamedField> {
        self.fields.get(index)
    }
}

// ----------------------------------------------------------------------------
// Unit variant

/// Information for unit enum variants.
#[derive(Clone, Copy, Debug)]
pub struct UnitVariantInfo {
    name: &'static str,
    attributes: Attributes,
    #[cfg(feature = "reflect_docs")]
    docs: Option<&'static str>,
}

impl UnitVariantInfo {
    impl_docs_fn!(docs);
    impl_attributes_fn!(attributes);
    impl_with_attributes!(attributes);

    /// Create a new [`UnitVariantInfo`].
    #[inline]
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            attributes: Attributes::EMPTY,
            #[cfg(feature = "reflect_docs")]
            docs: None,
        }
    }

    /// The name of this variant.
    #[inline]
    pub const fn name(&self) -> &'static str {
        self.name
    }
}

// ----------------------------------------------------------------------------
// VariantInfo

/// Container for compile-time enum variant info.
#[derive(Clone, Copy, Debug)]
pub enum VariantInfo {
    /// See [`UnitVariantInfo`].
    Unit(UnitVariantInfo),
    /// See [`TupleVariantInfo`].
    Tuple(TupleVariantInfo),
    /// See [`StructVariantInfo`].
    Struct(StructVariantInfo),
}

impl VariantInfo {
    /// Returns the custom attributes for this variant.
    pub fn attributes(&self) -> Attributes {
        match self {
            Self::Struct(info) => info.attributes(),
            Self::Tuple(info) => info.attributes(),
            Self::Unit(info) => info.attributes(),
        }
    }

    impl_attributes_fn!();

    /// The name of the enum variant.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Struct(info) => info.name(),
            Self::Tuple(info) => info.name(),
            Self::Unit(info) => info.name(),
        }
    }

    /// Returns the [`VariantKind`] of this variant.
    ///
    /// # Kinds
    ///
    /// - `A` -> Unit
    /// - `A()` and `A(..)` -> Tuple
    /// - `A{}` and `A{..}` -> Struct
    pub const fn variant_kind(&self) -> VariantKind {
        match self {
            Self::Struct(_) => VariantKind::Struct,
            Self::Tuple(_) => VariantKind::Tuple,
            Self::Unit(_) => VariantKind::Unit,
        }
    }

    /// Returns the number of fields in this variant.
    pub fn field_len(&self) -> usize {
        match self {
            Self::Unit(_) => 0,
            Self::Tuple(info) => info.field_len(),
            Self::Struct(info) => info.field_len(),
        }
    }

    /// The docstring of the underlying variant, if any.
    ///
    /// If `reflect_docs` feature is not enabled, this function always returns `None`.
    pub const fn docs(&self) -> Option<&'static str> {
        #[cfg(not(feature = "reflect_docs"))]
        return None;

        #[cfg(feature = "reflect_docs")]
        match self {
            Self::Struct(info) => info.docs(),
            Self::Tuple(info) => info.docs(),
            Self::Unit(info) => info.docs(),
        }
    }
}

macro_rules! impl_from_fn {
    ($kind:ident => $info:ident) => {
        impl From<$info> for VariantInfo {
            #[inline(always)]
            fn from(value: $info) -> Self {
                Self::$kind(value)
            }
        }

        impl TryFrom<VariantInfo> for $info {
            type Error = VariantInfo;

            #[inline(always)]
            fn try_from(value: VariantInfo) -> Result<Self, Self::Error> {
                match value {
                    VariantInfo::$kind(info) => Ok(info),
                    _ => Err(value),
                }
            }
        }
    };
}

impl_from_fn!(Unit => UnitVariantInfo);
impl_from_fn!(Tuple => TupleVariantInfo);
impl_from_fn!(Struct => StructVariantInfo);

macro_rules! impl_cast_fn {
    ($name:ident : $kind:ident => $info:ident) => {
        #[doc = concat!("Attempts a cast to a [`", stringify!($info), "`].")]
        pub const fn $name(&self) -> Result<&$info, VariantKindError> {
            match self {
                Self::$kind(info) => Ok(info),
                _ => Err(VariantKindError {
                    expected: VariantKind::$kind,
                    received: self.variant_kind(),
                }),
            }
        }
    };
}

impl VariantInfo {
    impl_cast_fn!(as_unit: Unit => UnitVariantInfo);
    impl_cast_fn!(as_tuple: Tuple => TupleVariantInfo);
    impl_cast_fn!(as_struct: Struct => StructVariantInfo);
}

// ----------------------------------------------------------------------------
