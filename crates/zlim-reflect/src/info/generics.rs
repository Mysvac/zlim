use core::any::{Any, TypeId};
use core::fmt::{Debug, Display};

use zlim_utils::mem::Global;

use super::Type;
use crate::path::TypePath;

// -----------------------------------------------------------------------------
// ConstParam — const-generic value storage

/// Stores the value of a const-generic parameter.
///
/// Wraps the limited set of primitive types that Rust allows in
/// `const` generic positions — integers, `char`, and `bool`.
///
/// See: <https://doc.rust-lang.org/reference/items/generics.html>
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ConstParam {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    Usize(usize),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    Isize(isize),
    Char(char),
    Bool(bool),
}

impl Display for ConstParam {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::U8(a) => Debug::fmt(a, f),
            Self::U16(a) => Debug::fmt(a, f),
            Self::U32(a) => Debug::fmt(a, f),
            Self::U64(a) => Debug::fmt(a, f),
            Self::U128(a) => Debug::fmt(a, f),
            Self::Usize(a) => Debug::fmt(a, f),
            Self::I8(a) => Debug::fmt(a, f),
            Self::I16(a) => Debug::fmt(a, f),
            Self::I32(a) => Debug::fmt(a, f),
            Self::I64(a) => Debug::fmt(a, f),
            Self::I128(a) => Debug::fmt(a, f),
            Self::Isize(a) => Debug::fmt(a, f),
            Self::Char(a) => Debug::fmt(a, f),
            Self::Bool(a) => Debug::fmt(a, f),
        }
    }
}

impl Debug for ConstParam {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self, f)
    }
}

macro_rules! impl_from_fn {
    ($ty:ident, $kind:ident) => {
        impl From<$ty> for ConstParam {
            #[inline(always)]
            fn from(value: $ty) -> Self {
                Self::$kind(value)
            }
        }

        impl TryFrom<ConstParam> for $ty {
            type Error = ConstParam;
            #[inline]
            fn try_from(value: ConstParam) -> Result<Self, Self::Error> {
                match value {
                    ConstParam::$kind(v) => Ok(v),
                    _ => Err(value),
                }
            }
        }
    };
}

impl_from_fn!(u8, U8);
impl_from_fn!(u16, U16);
impl_from_fn!(u32, U32);
impl_from_fn!(u64, U64);
impl_from_fn!(u128, U128);
impl_from_fn!(usize, Usize);
impl_from_fn!(i8, I8);
impl_from_fn!(i16, I16);
impl_from_fn!(i32, I32);
impl_from_fn!(i64, I64);
impl_from_fn!(i128, I128);
impl_from_fn!(isize, Isize);
impl_from_fn!(char, Char);
impl_from_fn!(bool, Bool);

// -----------------------------------------------------------------------------
// ConstParamInfo — const-generic parameter metadata

/// Compile-time metadata for a single const-generic parameter.
///
/// Captures the parameter name, its Rust type (as a [`TypePath`] ident), and
/// the concrete const value supplied at instantiation.
#[derive(Clone, Copy)]
pub struct ConstParamInfo {
    id: TypeId,
    type_ident: &'static str,
    param_ident: &'static str,
    const_value: ConstParam,
}

impl Debug for ConstParamInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "ConstParam<{}: {}>({:?})",
            self.param_ident, self.type_ident, self.const_value
        )
    }
}

impl ConstParamInfo {
    /// Creates a new [`ConstParamInfo`] for the given parameter `name`.
    ///
    /// The const value is initialized to a placeholder (`0i32`); call
    /// [`with_value`](Self::with_value) to set the actual value.
    #[inline]
    pub const fn new<T: TypePath>(name: &'static str) -> Self {
        Self {
            id: TypeId::of::<T>(),
            type_ident: T::IDENT,
            param_ident: name,
            const_value: ConstParam::I32(0),
        }
    }

    /// Sets the concrete const value for this parameter.
    #[inline]
    pub const fn with_value(mut self, value: ConstParam) -> Self {
        self.const_value = value;
        self
    }

    /// Returns the generic parameter name (e.g. `"N"`, `"SIZE"`).
    #[inline]
    pub const fn name(&self) -> &'static str {
        self.param_ident
    }

    /// Returns the [`TypeId`] of the parameter's Rust type.
    #[inline]
    pub const fn type_id(&self) -> TypeId {
        self.id
    }

    /// Returns the concrete const value.
    #[inline]
    pub const fn const_value(&self) -> ConstParam {
        self.const_value
    }

    /// Returns `true` if the parameter type is `T`.
    #[inline]
    pub fn type_is<T: Any>(&self) -> bool {
        self.id == TypeId::of::<T>()
    }

    /// Returns the short name of the parameter's Rust type.
    ///
    /// For a `const N: usize` parameter this returns `"usize"`.
    #[inline]
    pub fn type_path(&self) -> &'static str {
        self.type_ident
    }

    /// Returns the short name of the parameter's Rust type.
    #[inline]
    pub fn type_name(&self) -> &'static str {
        self.type_ident
    }

    /// Returns `None` — primitive types have no module path.
    #[inline]
    pub fn module_path(&self) -> Option<&'static str> {
        None
    }
}

// -----------------------------------------------------------------------------
// TypeParamInfo — type-generic parameter metadata

/// Compile-time metadata for a single type-generic parameter.
///
/// Captures the parameter name, its [`TypePath`] information (via function
/// pointers to avoid monomorphization bloat), and an optional default type.
#[derive(Clone, Copy)]
pub struct TypeParamInfo {
    id: TypeId,
    type_path: fn() -> &'static str,
    type_name: fn() -> &'static str,
    module_path: Option<&'static str>,
    param_ident: &'static str,
    default_type: Option<fn() -> Type>,
}

impl Debug for TypeParamInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "TypeParam<{}>({})", self.param_ident, (self.type_path)())
    }
}

impl TypeParamInfo {
    /// Creates a new [`TypeParamInfo`] for the given parameter `name`.
    ///
    /// The default type is initially `None`; call
    /// [`with_default`](Self::with_default) to set one.
    #[inline]
    pub const fn new<T: TypePath + ?Sized>(name: &'static str) -> Self {
        Self {
            id: TypeId::of::<T>(),
            type_path: T::type_path,
            type_name: T::type_name,
            module_path: T::MODULE,
            param_ident: name,
            default_type: None,
        }
    }

    /// Sets the default type for this parameter.
    #[inline]
    pub const fn with_default<T: TypePath + ?Sized>(mut self) -> Self {
        self.default_type = Some(Type::of::<T>);
        self
    }

    /// Returns the generic parameter name (e.g. `"T"`).
    #[inline]
    pub const fn name(&self) -> &'static str {
        self.param_ident
    }

    /// Returns the [`TypeId`].
    #[inline]
    pub const fn type_id(&self) -> TypeId {
        self.id
    }

    /// Returns the default type for this parameter, if present.
    #[inline]
    pub fn default_type(&self) -> Option<Type> {
        self.default_type.map(|f| f())
    }

    /// Returns `true` if the parameter type is `T`.
    #[inline]
    pub fn type_is<T: Any>(&self) -> bool {
        self.id == TypeId::of::<T>()
    }

    /// Returns the fully-qualified type path (with generics).
    #[inline]
    pub fn type_path(&self) -> &'static str {
        (self.type_path)()
    }

    /// Returns the short type name (without module path).
    #[inline]
    pub fn type_name(&self) -> &'static str {
        (self.type_name)()
    }

    /// Returns the module path of the parameter's type, if applicable.
    #[inline]
    pub fn module_path(&self) -> Option<&'static str> {
        self.module_path
    }
}

// -----------------------------------------------------------------------------
// GenericInfo — type- vs const-generic discriminator

/// A single generic parameter — either a type or a const.
///
/// Obtained from [`Generics::get`] or by iterating over
/// [`Generics::as_slice`].  Use [`is_type`] / [`is_const`] (or the
/// [`as_type`] / [`as_const`] accessors) to inspect the inner data.
///
/// [`is_type`]: Self::is_type
/// [`is_const`]: Self::is_const
/// [`as_type`]: Self::as_type
/// [`as_const`]: Self::as_const
#[derive(Clone, Copy)]
pub enum GenericInfo {
    /// A type-generic parameter (e.g. `T` in `Foo<T>`).
    Type(TypeParamInfo),
    /// A const-generic parameter (e.g. `const N: usize`).
    Const(ConstParamInfo),
}

impl From<TypeParamInfo> for GenericInfo {
    #[inline(always)]
    fn from(value: TypeParamInfo) -> Self {
        Self::Type(value)
    }
}

impl From<ConstParamInfo> for GenericInfo {
    #[inline(always)]
    fn from(value: ConstParamInfo) -> Self {
        Self::Const(value)
    }
}

impl GenericInfo {
    /// Returns the inner [`TypeParamInfo`] if this is a type parameter.
    #[inline]
    pub const fn as_type(&self) -> Option<&TypeParamInfo> {
        match self {
            Self::Type(info) => Some(info),
            _ => None,
        }
    }

    /// Returns the inner [`ConstParamInfo`] if this is a const parameter.
    #[inline]
    pub const fn as_const(&self) -> Option<&ConstParamInfo> {
        match self {
            Self::Const(info) => Some(info),
            _ => None,
        }
    }

    /// Returns the parameter name, regardless of kind.
    #[inline]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Type(info) => info.name(),
            Self::Const(info) => info.name(),
        }
    }

    /// Returns `true` if this is a type parameter.
    #[inline]
    pub const fn is_type(&self) -> bool {
        matches!(self, Self::Type(_))
    }

    /// Returns `true` if this is a const parameter.
    #[inline]
    pub const fn is_const(&self) -> bool {
        matches!(self, Self::Const(_))
    }
}

impl Debug for GenericInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Type(a) => {
                write!(f, "{} = {}", a.name(), a.type_name())
            }
            Self::Const(a) => {
                write!(f, "{}: {} = {}", a.name(), a.type_name(), a.const_value())
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Generics — frozen parameter list

/// A frozen, copyable collection of generic parameters.
///
/// Wraps a `'static` slice of [`GenericInfo`] values — each either a
/// [`TypeParamInfo`] or a [`ConstParamInfo`].  Lookups are by parameter name
/// (e.g. `"T"`, `"N"`).
///
/// # Creation
///
/// Use [`Generics::new`] to freeze a slice of [`GenericInfo`] items, or
/// [`Generics::EMPTY`] for the common case of a non-generic type.
///
/// # Performance
///
/// The struct is `Copy` and `#[repr(transparent)]` — it is a single pointer.
/// Lookups are linear scans, acceptable because the number of generic
/// parameters per type is typically very small (0–3).
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Generics(&'static [GenericInfo]);

impl Generics {
    /// An empty generics set.
    ///
    /// Use this instead of calling [`new(&[])`](Self::new) when
    /// there are no generics — it avoids both stack and heap
    /// allocations.
    pub const EMPTY: Self = Self(&[]);

    /// Creates a `Generics` from a slice of [`GenericInfo`] values.
    ///
    /// The slice is promoted to a `'static` allocation so the resulting
    /// [`Generics`] is cheaply [`Copy`].
    // #[inline(never)] // `alloc_slice` is `#[inline(never)]`
    pub fn new(generics: &[GenericInfo]) -> Self {
        Self(Global::alloc_slice(generics))
    }

    /// Returns the number of generic parameters stored.
    #[inline(always)]
    pub fn len(self) -> usize {
        self.0.len()
    }

    /// Returns `true` if no generic parameters are stored.
    #[inline(always)]
    pub fn is_empty(self) -> bool {
        self.0.is_empty()
    }

    /// Returns the underlying `'static` slice of all generic parameters.
    ///
    /// Useful for iteration or when callers need to inspect every parameter.
    #[inline(always)]
    pub fn as_slice(self) -> &'static [GenericInfo] {
        self.0
    }

    /// Returns the [`GenericInfo`] for the parameter with the given `name`,
    /// if present.
    ///
    /// Complexity: O(n) in the number of parameters.
    pub fn get(self, name: &str) -> Option<&'static GenericInfo> {
        self.0.iter().find(|info| info.name() == name)
    }

    /// Returns `true` if a generic parameter with the given `name` is
    /// present.
    ///
    /// Complexity: O(n) in the number of parameters.
    pub fn has(self, name: &str) -> bool {
        self.0.iter().any(|info| info.name() == name)
    }
}

impl Debug for Generics {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Generics").field(&self.0).finish()
    }
}

// -----------------------------------------------------------------------------
// Auxiliary macros
//
// These generate the `generics()` accessor, convenience lookup helpers, and
// the `with_generics()` builder-style setter on every info type that carries
// generic parameters. The `$field:ident` form is used when a type has a named
// field; the `()` form is used for enums (like `TypeInfo`) that delegate
// through a match.

/// Implements `generics()` plus the `has_generic` / `get_generic`
/// convenience methods.
///
/// # Forms
///
/// | Form | Use case |
/// |------|----------|
/// | `impl_generics_fn!(field_name)` | Struct with a named `Generics` field. |
/// | `impl_generics_fn!()` | Enum that dispatches `generics()` manually. |
macro_rules! impl_generics_fn {
    ($field:ident) => {
        /// Returns generic parameter metadata.
        ///
        /// See [`Generics`](crate::info::Generics).
        #[inline(always)]
        pub const fn generics(&self) -> $crate::info::Generics {
            self.$field
        }

        $crate::info::impl_generics_fn!();
    };
    () => {
        /// Returns `true` if a generic parameter with the given `name` is
        /// present.
        pub fn has_generic(&self, name: &str) -> bool {
            self.generics().has(name)
        }

        /// Returns the [`GenericInfo`] for the given parameter `name`, if
        /// present.
        ///
        /// [`GenericInfo`]: $crate::info::GenericInfo
        pub fn get_generic(&self, name: &str) -> Option<&'static $crate::info::GenericInfo> {
            self.generics().get(name)
        }
    };
}

/// Implements `with_generics()` — a builder-style setter that **replaces**
/// (does not merge) the generics field.
///
/// Primarily used by the proc-macro derive crate to attach generic parameter
/// metadata discovered at compile time.
macro_rules! impl_with_generics {
    ($field:ident) => {
        /// Replaces stored generics (overwrite, do not merge).
        ///
        /// Used by the proc-macro crate.
        pub fn with_generics(self, generics: $crate::info::Generics) -> Self {
            Self {
                $field: generics,
                ..self
            }
        }
    };
}

pub(crate) use impl_generics_fn;
pub(crate) use impl_with_generics;

// -----------------------------------------------------------------------------
