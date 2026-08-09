use core::error::Error;
use core::fmt::{self, Display};

use super::{ArrayInfo, ListInfo, TupleInfo};
use super::{Attributes, Generics, Type};
use super::{EnumInfo, StructInfo};
use super::{MapInfo, OpaqueInfo, SetInfo};
use crate::path::TypePath;

// -----------------------------------------------------------------------------
// ReflectKind

/// An enumeration of the "kinds" of a reflected type.
///
/// Each kind corresponds to a specific reflection trait,
/// such as `Struct` or `List`, which itself corresponds
/// to the kind or structure of a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(PartialOrd, Ord, Hash)]
pub enum ReflectKind {
    Opaque,
    Struct,
    Tuple,
    Array,
    List,
    Map,
    Set,
    Enum,
}

macro_rules! impl_kind_is {
    ($ident:ident, $var:ident) => {
        /// Checks the metadata kind.
        #[inline(always)]
        pub const fn $ident(self) -> bool {
            matches!(self, Self::$var)
        }
    };
}

impl ReflectKind {
    impl_kind_is!(is_opaque, Opaque);
    impl_kind_is!(is_enum, Enum);
    impl_kind_is!(is_struct, Struct);
    impl_kind_is!(is_tuple, Tuple);
    impl_kind_is!(is_array, Array);
    impl_kind_is!(is_list, List);
    impl_kind_is!(is_map, Map);
    impl_kind_is!(is_set, Set);
}

impl Display for ReflectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Opaque => f.pad("Opaque"),
            Self::Struct => f.pad("Struct"),
            Self::Tuple => f.pad("Tuple"),
            Self::Array => f.pad("Array"),
            Self::List => f.pad("List"),
            Self::Map => f.pad("Map"),
            Self::Set => f.pad("Set"),
            Self::Enum => f.pad("Enum"),
        }
    }
}

/// Error returned when a `TypeInfo` value is not the expected `ReflectKind`.
#[derive(Clone, Copy, Debug)]
pub struct ReflectKindError {
    pub expected: ReflectKind,
    pub received: ReflectKind,
}

impl Display for ReflectKindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "reflect kind mismatch: expected {}, received {}",
            self.expected, self.received
        )
    }
}

impl Error for ReflectKindError {}

// -----------------------------------------------------------------------------
// TypeInfo

/// Compile-time type information for various reflected types.
#[derive(Debug)]
pub enum TypeInfo {
    Opaque(OpaqueInfo),
    Struct(StructInfo),
    Tuple(TupleInfo),
    Array(ArrayInfo),
    List(ListInfo),
    Map(MapInfo),
    Set(SetInfo),
    Enum(EnumInfo),
}

// Helper macro that implements type-safe accessor methods like `as_struct`.
macro_rules! impl_cast_method {
    ($name:ident : $kind:ident => $info:ident) => {
        /// Convert [`TypeInfo`] to specific type information.
        pub const fn $name(&self) -> Result<&$info, ReflectKindError> {
            match self {
                Self::$kind(info) => Ok(info),
                _ => Err(ReflectKindError {
                    expected: ReflectKind::$kind,
                    received: self.kind(),
                }),
            }
        }
    };
}

impl TypeInfo {
    impl_cast_method!(as_opaque: Opaque => OpaqueInfo);
    impl_cast_method!(as_struct: Struct => StructInfo);
    impl_cast_method!(as_tuple: Tuple => TupleInfo);
    impl_cast_method!(as_array: Array => ArrayInfo);
    impl_cast_method!(as_list: List => ListInfo);
    impl_cast_method!(as_map: Map => MapInfo);
    impl_cast_method!(as_set: Set => SetInfo);
    impl_cast_method!(as_enum: Enum => EnumInfo);

    /// Returns the [`ReflectKind`] for this `TypeInfo` (a fast discriminator).
    pub const fn kind(&self) -> ReflectKind {
        match self {
            Self::Opaque(_) => ReflectKind::Opaque,
            Self::Struct(_) => ReflectKind::Struct,
            Self::Tuple(_) => ReflectKind::Tuple,
            Self::Array(_) => ReflectKind::Array,
            Self::List(_) => ReflectKind::List,
            Self::Map(_) => ReflectKind::Map,
            Self::Set(_) => ReflectKind::Set,
            Self::Enum(_) => ReflectKind::Enum,
        }
    }

    /// Returns the underlying [`Type`] metadata for this `TypeInfo`.
    pub const fn ty(&self) -> &Type {
        match self {
            Self::Opaque(info) => info.ty(),
            Self::Struct(info) => info.ty(),
            Self::Tuple(info) => info.ty(),
            Self::Array(info) => info.ty(),
            Self::List(info) => info.ty(),
            Self::Map(info) => info.ty(),
            Self::Set(info) => info.ty(),
            Self::Enum(info) => info.ty(),
        }
    }
    super::impl_type_fn!();

    /// Returns the generics metadata (type/const parameters) for this type.
    pub const fn generics(&self) -> Generics {
        match self {
            Self::Opaque(info) => info.generics(),
            Self::Struct(info) => info.generics(),
            Self::Tuple(info) => info.generics(),
            Self::Array(info) => info.generics(),
            Self::List(info) => info.generics(),
            Self::Map(info) => info.generics(),
            Self::Set(info) => info.generics(),
            Self::Enum(info) => info.generics(),
        }
    }
    super::impl_generics_fn!();

    /// Returns the custom attributes attached to this type, if any.
    pub fn attributes(&self) -> Attributes {
        match self {
            Self::Opaque(info) => info.attributes(),
            Self::Struct(info) => info.attributes(),
            Self::Tuple(info) => info.attributes(),
            Self::Array(info) => info.attributes(),
            Self::List(info) => info.attributes(),
            Self::Map(info) => info.attributes(),
            Self::Set(info) => info.attributes(),
            Self::Enum(info) => info.attributes(),
        }
    }
    super::impl_attributes_fn!();
}

// -----------------------------------------------------------------------------
// Typed

/// A static accessor to compile-time type information.
///
/// Automatically implemented by the derive macro,
/// allowing access to type information without an instance of the type.
pub trait Typed: TypePath {
    /// A static accessor to compile-time type information.
    ///
    /// Note: Use [`DynamicTyped`] for dynamic dispatch.
    fn type_info() -> &'static TypeInfo;
}

// -----------------------------------------------------------------------------
// DynamicTyped

/// Provide dynamic dispatch for types that implement [`Typed`].
///
/// Auto impl for all types that implemented [`Typed`].
pub trait DynamicTyped {
    /// Provide dynamic dispatch for types that implement [`Typed`].
    ///
    /// When you hold a `dyn Reflect` object,
    /// you can use this method to get type information.
    fn reflect_type_info(&self) -> &'static TypeInfo;
}

impl<T: Typed> DynamicTyped for T {
    #[inline]
    fn reflect_type_info(&self) -> &'static TypeInfo {
        Self::type_info()
    }
}

// -----------------------------------------------------------------------------
// InfoCell

use core::any::TypeId;
use std::sync::{PoisonError, RwLock};
use zlim_utils::ext::TypeMap;
use zlim_utils::mem::Global;

/// A cache for generic type information.
///
/// Stores [`TypeInfo`] keyed by [`TypeId`], allowing each monomorphized
/// instantiation of a generic type to carry its own metadata.
///
/// For non-generic types prefer [`OnceLock`] / [`LazyLock`] — they are
/// cheaper than the `RwLock` used here.  For types that can be constructed
/// as compile-time constants a plain `static` with [`TypeInfo`] is the best
/// choice.
///
/// [`OnceLock`]: std::sync::OnceLock
/// [`LazyLock`]: std::sync::LazyLock
pub struct InfoCell(RwLock<TypeMap<&'static TypeInfo>>);

impl InfoCell {
    /// Creates an empty cell.
    #[expect(clippy::new_without_default, reason = "need `const`")]
    pub const fn new() -> Self {
        Self(RwLock::new(TypeMap::new()))
    }

    /// Returns a reference to the [`TypeInfo`] stored in the cell.
    ///
    /// This method will then return the correct `Info` reference for the given type `T`.
    /// If there is no entry found, a new one will be generated from the given function.
    #[inline(always)]
    pub fn get_or_init<T: 'static>(&self, f: impl FnOnce() -> TypeInfo) -> &'static TypeInfo {
        match self.get_by_type_id(TypeId::of::<T>()) {
            Some(info) => info,
            None => self.insert_by_type_id(TypeId::of::<T>(), f()),
        }
    }

    // Separate to reduce code compilation times
    #[inline(never)]
    fn get_by_type_id(&self, type_id: TypeId) -> Option<&'static TypeInfo> {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(type_id)
            .copied()
    }

    // Separate to reduce code compilation times
    #[cold]
    #[inline(never)]
    fn insert_by_type_id(&self, type_id: TypeId, s: TypeInfo) -> &'static TypeInfo {
        // SAFETY: TypeInfo does not implement `Drop`.
        #[expect(unsafe_code, reason = "TypeInfo does not impl `Copy`")]
        let value = unsafe { Global::alloc_unchecked(s) };

        // Concurrent allocations of the same type are rare and acceptable —
        // we avoid holding the write lock during allocation.
        self.0
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(type_id, value);

        value
    }
}

// -----------------------------------------------------------------------------
