use super::{Array, Enum, List, Map, Opaque, Set, Struct, Tuple};
use crate::info::{ReflectKind, ReflectKindError};

// -----------------------------------------------------------------------------
// ReflectRef

/// An immutable enumeration of ["kinds"](ReflectKind) of a reflected type.
///
/// Each variant contains a trait object with methods specific to a kind of
/// type.
///
/// A [`ReflectRef`] is obtained via [`Reflect::reflect_ref`],
/// its kind must be consistent with [`Reflect::reflect_kind`].
///
/// [`Reflect::reflect_ref`]: crate::Reflect::reflect_mut
/// [`Reflect::reflect_kind`]: crate::Reflect::reflect_kind
pub enum ReflectRef<'a> {
    Opaque(&'a dyn Opaque),
    Struct(&'a dyn Struct),
    Tuple(&'a dyn Tuple),
    Array(&'a dyn Array),
    List(&'a dyn List),
    Map(&'a dyn Map),
    Set(&'a dyn Set),
    Enum(&'a dyn Enum),
}

// -----------------------------------------------------------------------------
// ReflectMut

/// A mutable enumeration of ["kinds"](ReflectKind) of a reflected type.
///
/// Each variant contains a trait object with methods specific to a kind of
/// type.
///
/// A [`ReflectMut`] is obtained via [`Reflect::reflect_mut`],
/// its kind must be consistent with [`Reflect::reflect_kind`].
///
/// [`Reflect::reflect_mut`]: crate::Reflect::reflect_mut
/// [`Reflect::reflect_kind`]: crate::Reflect::reflect_kind
pub enum ReflectMut<'a> {
    Opaque(&'a mut dyn Opaque),
    Struct(&'a mut dyn Struct),
    Tuple(&'a mut dyn Tuple),
    Array(&'a mut dyn Array),
    List(&'a mut dyn List),
    Map(&'a mut dyn Map),
    Set(&'a mut dyn Set),
    Enum(&'a mut dyn Enum),
}

// -----------------------------------------------------------------------------
// ReflectOwned

/// An owned enumeration of ["kinds"](ReflectKind) of a reflected type.
///
/// Each variant contains a trait object with methods specific to a kind of
/// type.
///
/// A [`ReflectOwned`] is obtained via [`Reflect::reflect_owned`],
/// its kind must be consistent with [`Reflect::reflect_kind`].
///
/// [`Reflect::reflect_owned`]: crate::Reflect::reflect_mut
/// [`Reflect::reflect_kind`]: crate::Reflect::reflect_kind
pub enum ReflectOwned {
    Opaque(Box<dyn Opaque>),
    Struct(Box<dyn Struct>),
    Tuple(Box<dyn Tuple>),
    Array(Box<dyn Array>),
    List(Box<dyn List>),
    Map(Box<dyn Map>),
    Set(Box<dyn Set>),
    Enum(Box<dyn Enum>),
}

// -----------------------------------------------------------------------------
// Implementation

macro_rules! impl_kind_fn {
    () => {
        /// Returns the [`ReflectKind`] discriminant without the associated
        /// type-specific data.
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

        // Internal Helper, see `impl_cast_fn` macro below.
        #[cold]
        fn kind_error(&self, expected: ReflectKind) -> ReflectKindError {
            ReflectKindError {
                expected,
                received: self.kind(),
            }
        }
    };
}

macro_rules! impl_cast_fn {
    ($name:ident : $kind:ident => $retval:ty) => {
        #[doc = concat!("Attempts a cast to a `", stringify!($kind), "` trait object.")]
        pub fn $name(self) -> Result<$retval, ReflectKindError> {
            match self {
                Self::$kind(value) => Ok(value),
                this => Err(this.kind_error(ReflectKind::$kind)),
            }
        }
    };
}

impl<'a> ReflectRef<'a> {
    impl_kind_fn!();
    impl_cast_fn!(as_opaque: Opaque => &'a dyn Opaque);
    impl_cast_fn!(as_struct: Struct => &'a dyn Struct);
    impl_cast_fn!(as_tuple: Tuple => &'a dyn Tuple);
    impl_cast_fn!(as_array: Array => &'a dyn Array);
    impl_cast_fn!(as_list: List => &'a dyn List);
    impl_cast_fn!(as_map: Map => &'a dyn Map);
    impl_cast_fn!(as_set: Set => &'a dyn Set);
    impl_cast_fn!(as_enum: Enum => &'a dyn Enum);
}

impl<'a> ReflectMut<'a> {
    impl_kind_fn!();
    impl_cast_fn!(as_opaque: Opaque => &'a mut dyn Opaque);
    impl_cast_fn!(as_struct: Struct => &'a mut dyn Struct);
    impl_cast_fn!(as_tuple: Tuple => &'a mut dyn Tuple);
    impl_cast_fn!(as_array: Array => &'a mut dyn Array);
    impl_cast_fn!(as_list: List => &'a mut dyn List);
    impl_cast_fn!(as_map: Map => &'a mut dyn Map);
    impl_cast_fn!(as_set: Set => &'a mut dyn Set);
    impl_cast_fn!(as_enum: Enum => &'a mut dyn Enum);
}

impl ReflectOwned {
    impl_kind_fn!();
    impl_cast_fn!(into_opaque: Opaque => Box<dyn Opaque>);
    impl_cast_fn!(into_struct: Struct => Box<dyn Struct>);
    impl_cast_fn!(into_tuple: Tuple => Box<dyn Tuple>);
    impl_cast_fn!(into_array: Array => Box<dyn Array>);
    impl_cast_fn!(into_list: List => Box<dyn List>);
    impl_cast_fn!(into_map: Map => Box<dyn Map>);
    impl_cast_fn!(into_set: Set => Box<dyn Set>);
    impl_cast_fn!(into_enum: Enum => Box<dyn Enum>);
}

// -----------------------------------------------------------------------------
