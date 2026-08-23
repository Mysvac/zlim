//! Deserialization support for the reflection system.
//!
//! # Entry Points
//!
//! Consider a struct `A { x: 3, y: 4 }` registered as `"my_crate::A"`.
//! The two entry points expect different JSON:
//!
//! | Function | Expected JSON |
//! |---|---|
//! | [`reflect_deserialize`] | `{"my_crate::A": {"x": 3, "y": 4}}` |
//! | [`deserialize`] | `{"x": 3, "y": 4}` |
//!
//! [`reflect_deserialize`] reads the type path from the input, resolves it
//! to a `&TypeDB` via [`TypeDB::get_by_path`], then deserializes the
//! payload.  This is **self-describing** — the input carries its own type
//! information.  [`deserialize`] expects only the payload; the caller must
//! already hold a `&TypeDB` for the target type.
//!
//! # Deserialization priority
//!
//! 1. **Registered Deserializer First**: if the [`TypeDB`] has a registered
//!    `DeseFunc` (set via [`insert_deserializer`]), use it directly — the
//!    **fast path** for types with a serde `Deserialize` implementation.
//!
//! 2. **Reflection Fallback**: otherwise inspect [`TypeInfo`] and dispatch
//!    to the kind-specific visitor:
//!
//!    | TypeInfo kind | Visitor | Serde entry |
//!    |---|---|---|
//!    | `Opaque` | `OpaqueVisitor` | `deserialize_any` |
//!    | `Struct` | `StructVisitor` | `deserialize_struct` |
//!    | `Tuple`  | `TupleVisitor`  | `deserialize_tuple` / `deserialize_tuple_struct` |
//!    | `Array`  | `ArrayVisitor`  | `deserialize_tuple` |
//!    | `List`   | `ListVisitor`   | `deserialize_seq` |
//!    | `Map`    | `MapVisitor`    | `deserialize_map` |
//!    | `Set`    | `SetVisitor`    | `deserialize_seq` |
//!    | `Enum`   | `EnumVisitor`   | `deserialize_enum` / `deserialize_option` |
//!
//! Each visitor follows a **two-phase** strategy: if the type has a default
//! constructor, create an empty native value and mutate fields in-place
//! (fast); otherwise build a [`Dynamic*`](crate::dynamic) value and convert
//! via [`TypeDB::from_reflect`] (always works, but slower).
//!
//! [`reflect_deserialize`]: TypeDB::reflect_deserialize
//! [`deserialize`]: TypeDB::deserialize()
//! [`TypeDB::get_by_path`]: TypeDB::get_by_path
//! [`insert_deserializer`]: TypeDB::insert_deserializer
//! [`TypeDB`]: super::TypeDB
//! [`TypeInfo`]: crate::info::TypeInfo

use core::any::TypeId;
use core::fmt::{self, Display, Formatter};
use core::panic::Location;
use std::borrow::Cow;

use erased_serde::Deserializer as ErasedDeserializer;
use serde_core::de::{DeserializeSeed, IgnoredAny, MapAccess, Visitor};
use serde_core::de::{EnumAccess, Error, SeqAccess, VariantAccess};
use serde_core::{Deserialize, Deserializer};
use zlim_log as log;
use zlim_utils::format_smol;

use super::{TypeDB, TypeDatabase};
use crate::Reflect;
use crate::info::{ReflectKind, ReflectKindError, TypeInfo};

// -----------------------------------------------------------------------------
// Register
// -----------------------------------------------------------------------------

/// Logs a message when the same deserializer is registered more than once.
///
/// Uses `debug!` in release mode and `info!` in debug mode.  The original
/// registration is kept; this is purely informational.
#[cold]
#[inline(never)]
fn warn_deserializer_dup(ty: &'static str, l: &'static Location<'static>) {
    #[cfg(not(feature = "debug"))]
    log::debug!("{l}: `{ty}`'s deserializer registered repeatedly; ignored.");

    // Upgrade the message level in debug mode.
    #[cfg(feature = "debug")]
    log::info!("{l}: `{ty}`'s deserializer registered repeatedly; ignored.");
}

impl TypeDB {
    /// Registers a `DeseFunc` wrapper for type `T` into this `TypeDB`.
    ///
    /// The wrapper calls `T::deserialize` through an
    /// [`erased_serde::Deserializer`].  Once registered, `ReflectDeser`
    /// uses this function directly (the **fast path**), bypassing the
    /// kind-specific visitor.
    ///
    /// # Returns
    ///
    /// `true` on first registration, `false` if a deserializer was already
    /// registered (a warning is logged and the original is kept).
    ///
    /// # Panics
    ///
    /// Panics if `self` does not belong to type `T`.
    #[cold]
    #[track_caller]
    #[inline(never)]
    pub fn insert_deserializer<T>(&self) -> bool
    where
        T: TypeDatabase + for<'de> Deserialize<'de>,
    {
        #[cold]
        #[inline(never)]
        fn panicked(e: &'static str, a: &'static str, l: &'static Location<'static>) -> ! {
            panic!(
                "{l}: `insert_deserializer` type mismatch — \
                TypeDB is for `{e}`, but the Deserialize need `{a}`."
            )
        }

        if self.id != TypeId::of::<T>() {
            panicked(self.type_path, T::type_path(), Location::caller());
        }

        fn func<T>(de: &mut dyn ErasedDeserializer) -> Result<Box<dyn Reflect>, erased_serde::Error>
        where
            T: TypeDatabase + for<'de> Deserialize<'de>,
        {
            Ok(Box::new(T::deserialize(de)?))
        }

        if self.deserialize.set(func::<T>).is_err() {
            warn_deserializer_dup(T::type_path(), Location::caller());
            false
        } else {
            true
        }
    }

    /// Convenience wrapper: resolves `T`'s [`TypeDB`] via
    /// [`TypeDB::of`](TypeDB::of) then calls
    /// [`insert_deserializer`](Self::insert_deserializer).
    #[cold]
    #[track_caller]
    pub fn register_deserializer<T>() -> bool
    where
        T: TypeDatabase + for<'de> Deserialize<'de>,
    {
        let db = TypeDB::of::<T>();
        db.insert_deserializer::<T>()
    }
}

// -----------------------------------------------------------------------------
// deserialize
// -----------------------------------------------------------------------------

impl TypeDB {
    /// Self-describing deserializer for reflected types.
    ///
    /// # Example
    ///
    /// For a struct `A { x: 3, y: 4 }` registered as `"my_crate::A"`,
    /// the expected JSON is:
    ///
    /// ```text
    /// {"my_crate::A": {"x": 3, "y": 4}}
    /// ```
    ///
    /// The outer key `"my_crate::A"` is resolved to a `&TypeDB` via
    /// [`TypeDB::get_by_path`]; the inner `{"x": 3, "y": 4}` is the payload
    /// deserialized by [`deserialize`].
    ///
    /// # Deserialization Rules
    ///
    /// Internally delegates to `TypePathReflectDeser`, which:
    ///
    /// 1. Reads the type path key via `TypePathDeser` → `&'static TypeDB`.
    /// 2. Reads the value via `ReflectDeser`, following the two-step
    ///    priority order described in [`deserialize`].
    ///
    /// This is the counterpart to [`reflect_serialize`].
    ///
    /// # Use when
    ///
    /// The input is **self-describing** — the type is embedded in the data.
    ///
    /// If the target type is already known, use [`deserialize`] instead.
    ///
    /// [`deserialize`]: TypeDB::deserialize()
    /// [`reflect_serialize`]: TypeDB::reflect_serialize
    #[inline]
    pub fn reflect_deserialize<'de, D>(deserializer: D) -> Result<Box<dyn Reflect>, D::Error>
    where
        D: Deserializer<'de>,
    {
        TypePathReflectDeser.deserialize(deserializer)
    }

    /// Type-known deserializer, **without** type path resolution.
    ///
    /// # Example
    ///
    /// For a struct `A { x: 3, y: 4 }` (regardless of its registered path),
    /// the expected JSON is:
    ///
    /// ```text
    /// {"x": 3, "y": 4}
    /// ```
    ///
    /// This matches standard serde input.  Compare with [`reflect_deserialize`],
    /// which would expect `{"my_crate::A": {"x": 3, "y": 4}}`.
    ///
    /// # Deserialization Rules
    ///
    /// Delegates to `ReflectDeser`, which follows a two-step priority
    /// order:
    ///
    /// 1. **Registered Deserializer First**: checks whether this `TypeDB`
    ///    has a registered `DeseFunc` (set via
    ///    [`insert_deserializer`]).  If present, the function is called
    ///    directly — this is the **fast path** for types with a serde
    ///    `Deserialize` implementation (e.g. via
    ///    `#[derive(Deserialize)]`).
    ///
    /// 2. **Reflection Fallback**: if no `DeseFunc` is registered, inspects
    ///    [`TypeInfo`] and dispatches to the appropriate kind-specific
    ///    visitor (`OpaqueVisitor`, `StructVisitor`, …,
    ///    `EnumVisitor`).  Each visitor first tries to create a native
    ///    default value and mutate it in-place; if no default constructor
    ///    exists, builds a [`Dynamic*`](crate::dynamic) value and converts
    ///    via [`TypeDB::from_reflect`].
    ///
    /// # Use when
    ///
    /// The target type is **already known** from context (e.g. a stored
    /// `&TypeDB` reference, or a known component field in an ECS world).
    /// The input contains only the payload — no type path key.
    ///
    /// For a self-describing variant that reads the type from the input, see
    /// [`reflect_deserialize`].
    ///
    /// [`reflect_deserialize`]: TypeDB::reflect_deserialize
    /// [`insert_deserializer`]: TypeDB::insert_deserializer
    #[inline]
    pub fn deserialize<'de, D>(&self, deserializer: D) -> Result<Box<dyn Reflect>, D::Error>
    where
        D: Deserializer<'de>,
    {
        ReflectDeser(self).deserialize(deserializer)
    }
}

// -----------------------------------------------------------------------------
// Helper
// -----------------------------------------------------------------------------

/// A [`DeserializeSeed`] that produces [`IgnoredAny`].
///
/// Used to skip unknown keys in map-based deserialization without allocating
/// or validating the value.
struct IgnoreSeed;

impl<'de> DeserializeSeed<'de> for IgnoreSeed {
    type Value = IgnoredAny;

    #[inline]
    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(IgnoredAny)
    }
}

/// A map key deserialized as a string identifier.
///
/// Used as the key type when deserializing maps with named fields (struct
/// key-value format, enum variant names).
struct Ident(pub String);

impl<'de> Deserialize<'de> for Ident {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct IdentVisitor;

        impl<'de> Visitor<'de> for IdentVisitor {
            type Value = Ident;

            fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
                formatter.write_str("identifier")
            }

            #[inline]
            fn visit_str<E: Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(Ident(value.to_owned()))
            }

            #[inline]
            fn visit_string<E: Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(Ident(value))
            }
        }

        deserializer.deserialize_identifier(IdentVisitor)
    }
}

crate::cfg::debug! {
    std::thread_local! {
        static TYPE_INFO_STACK: ::core::cell::RefCell<super::TypeInfoStack> =
            const { ::core::cell::RefCell::new(super::TypeInfoStack::new()) };
    }
}

/// Centralized error constructor.
#[cold]
fn make_error<E: Error>(msg: impl Display) -> E {
    crate::cfg::debug! {
        if {
            TYPE_INFO_STACK.with_borrow(|stack| {
                E::custom(format_args!("{msg} (stack:\n{stack:?})"))
            })
        } else {
            E::custom(msg)
        }
    }
}

#[cold]
#[cfg_attr(any(debug_assertions, feature = "debug"), inline(never))]
#[cfg_attr(not(any(debug_assertions, feature = "debug")), inline(always))]
fn maperr<E: Error>(e: E) -> E {
    crate::cfg::debug! {
        if {
            TYPE_INFO_STACK.with_borrow(|stack| {
                E::custom(format_args!("{e} (stack:\n{stack:?})"))
            })
        } else {
            e
        }
    }
}

#[cold]
#[inline(never)]
fn invalid_info<E: Error>(ty: &'static str, error: ReflectKindError) -> E {
    make_error(format_args!(
        "Invalid type info for `{ty}`, `{error}`. \
        There may be a dynamic type that cannot be deserialized."
    ))
}

#[cold]
#[inline(never)]
fn invalid_convert<E: Error>(to: &'static str, from: &dyn Reflect) -> E {
    make_error(format_args!(
        "Try convert a `{}` to `{}` failed, internal values: `{:?}`.",
        from.reflect_type_path(),
        to,
        from,
    ))
}

#[cold]
#[inline(never)]
fn invalid_key_value<E: Error>(map: &'static str, k: &dyn Reflect, v: &dyn Reflect) -> E {
    make_error(format_args!(
        "Invalid key-value pair for `{}` container, expected key: `{}`, value: `{}`, got key: `{:?}`, value: `{:?}`.",
        map,
        k.reflect_type_path(),
        v.reflect_type_path(),
        k,
        v,
    ))
}

#[cold]
#[inline(never)]
fn missing_type_db<E: Error>(name: &'static str) -> E {
    make_error(format_args!("no TypeDB found for type `{name}`"))
}

// -----------------------------------------------------------------------------
// TypePathDeser
// -----------------------------------------------------------------------------

/// Resolves a type-path string (e.g. `"my_crate::components::Health"`) to a
/// `&'static TypeDB` via [`TypeDB::get_by_path`].
struct TypePathDeser;

impl<'de> Visitor<'de> for TypePathDeser {
    type Value = &'static TypeDB;

    fn expecting(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str("string containing `type` entry for the reflected value")
    }

    #[inline]
    fn visit_str<E: Error>(self, type_path: &str) -> Result<Self::Value, E> {
        TypeDB::get_by_path(type_path).ok_or_else(|| {
            ::core::hint::cold_path();
            Error::custom(format_args!("no type database found for `{type_path}`"))
        })
    }
}

impl<'de> DeserializeSeed<'de> for TypePathDeser {
    type Value = &'static TypeDB;

    #[inline]
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_str(TypePathDeser)
    }
}

// -----------------------------------------------------------------------------
// TypePathReflectDeser
// -----------------------------------------------------------------------------

/// Top-level entry for self-describing formats.
///
/// Deserializes a single-entry map `{type_path: payload}` where `type_path`
/// is resolved to a `&TypeDB` via [`TypePathDeser`] and the payload is
/// deserialized via [`ReflectDeser`].
struct TypePathReflectDeser;

impl<'de> Visitor<'de> for TypePathReflectDeser {
    type Value = Box<dyn Reflect>;

    fn expecting(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str("a single-entry map keyed by type path, e.g. {\"my_crate::Foo\": ...}")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        // Resolve the target type metadata from the registry.
        let db = map
            .next_key_seed(TypePathDeser)?
            .ok_or_else(|| Error::invalid_length(0, &"a single entry"))?;

        let value = map.next_value_seed(ReflectDeser(db))?;

        if map.next_key::<IgnoredAny>()?.is_some() {
            return Err(Error::invalid_length(2, &"a single entry"));
        }

        Ok(value)
    }
}

impl<'de> DeserializeSeed<'de> for TypePathReflectDeser {
    type Value = Box<dyn Reflect>;

    #[inline]
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(TypePathReflectDeser)
    }
}

// -----------------------------------------------------------------------------
// ReflectDeser
// -----------------------------------------------------------------------------

/// Central deserialization dispatcher.
///
/// Given a `&TypeDB`, decides how to deserialize:
///
/// - **Fast path:** if the `TypeDB` has a registered `DeseFunc`, call it
///   directly through an erased serde deserializer.
/// - **Slow path:** inspect [`TypeInfo`] and delegate to the kind-specific
///   visitor (`OpaqueVisitor`, `StructVisitor`, …, `EnumVisitor`).
struct ReflectDeser<'a>(&'a TypeDB);

impl<'de> DeserializeSeed<'de> for ReflectDeser<'_> {
    type Value = Box<dyn Reflect>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        if let Some(f) = self.0.deserialize.get() {
            let mut erased = <dyn erased_serde::Deserializer>::erase(deserializer);
            return f(&mut erased).map_err(make_error);
        }

        crate::cfg::debug! {
            TYPE_INFO_STACK.with_borrow_mut(|stack| stack.push(self.0.type_info))
        }

        let returne_value: Result<Box<dyn Reflect>, D::Error> = match self.0.type_info {
            TypeInfo::Opaque(_) => OpaqueVisitor(self.0).vis(deserializer),
            TypeInfo::Struct(_) => StructVisitor(self.0).vis(deserializer),
            TypeInfo::Tuple(_) => TupleVisitor(self.0).vis(deserializer),
            TypeInfo::Array(_) => ArrayVisitor(self.0).vis(deserializer),
            TypeInfo::List(_) => ListVisitor(self.0).vis(deserializer),
            TypeInfo::Map(_) => MapVisitor(self.0).vis(deserializer),
            TypeInfo::Set(_) => SetVisitor(self.0).vis(deserializer),
            TypeInfo::Enum(_) => EnumVisitor(self.0).vis(deserializer),
        };

        crate::cfg::debug! {
            TYPE_INFO_STACK.with_borrow_mut(|stack|stack.pop());
        }

        returne_value
    }
}

// -----------------------------------------------------------------------------
// OpaqueVisitor
// -----------------------------------------------------------------------------

/// Deserializes opaque types (primitives: `i32`, `f64`, `bool`, `String`,
/// `char`, etc.).
///
/// These types have no internal structure exposed to reflection.
///
/// # Fallback strategy
///
/// When no registered `DeseFunc` is found, the visitor:
///
/// 1. Gets a default instance via [`TypeDB::default`].
/// 2. Casts it to `Box<dyn Opaque>`.
/// 3. Calls [`Opaque::apply_str`] with the string-serialized value
///    (or `""` for `None`, `"()"` for unit).
///
/// This only works for types whose `apply_str` can parse the given string
/// format.  If no default constructor exists, returns an error via
/// [`failed`](OpaqueVisitor::failed).
struct OpaqueVisitor<'a>(&'a TypeDB);

impl OpaqueVisitor<'_> {
    #[cold] // Opaque type should provide deserializer, Visitor is usually unused.
    #[inline(always)]
    fn vis<'de, D>(self, deserializer: D) -> Result<Box<dyn Reflect>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }

    #[cold]
    fn failed<E: Error>(&self) -> E {
        make_error(format!(
            "Opaque type `{}` has no deserializer and no defaultor; \
            deserialization is not possible.",
            self.0.type_path(),
        ))
    }
}

macro_rules! impl_visit {
    ($func:ident, $ty:ty, $input:ident, $e:expr) => {
        fn $func<E: Error>(self, $input: $ty) -> Result<Self::Value, E> {
            let Some(default) = self.0.default() else {
                return Err(self.failed());
            };
            let mut boxed = default.reflect_owned().into_opaque().unwrap();
            if let Err(e) = boxed.apply_str($e) {
                ::core::hint::cold_path();
                return Err(Error::custom(e));
            }
            Ok(boxed)
        }
    };
}

impl<'de> Visitor<'de> for OpaqueVisitor<'_> {
    type Value = Box<dyn Reflect>;

    fn expecting(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str(self.0.type_path)?;
        f.write_str(" value")
    }

    impl_visit!(visit_i8, i8, v, &format_smol!("{v}"));
    impl_visit!(visit_i16, i16, v, &format_smol!("{v}"));
    impl_visit!(visit_i32, i32, v, &format_smol!("{v}"));
    impl_visit!(visit_i64, i64, v, &format_smol!("{v}"));
    impl_visit!(visit_i128, i128, v, &format_smol!("{v}"));

    impl_visit!(visit_u8, u8, v, &format_smol!("{v}"));
    impl_visit!(visit_u16, u16, v, &format_smol!("{v}"));
    impl_visit!(visit_u32, u32, v, &format_smol!("{v}"));
    impl_visit!(visit_u64, u64, v, &format_smol!("{v}"));
    impl_visit!(visit_u128, u128, v, &format_smol!("{v}"));

    impl_visit!(visit_f32, f32, v, &format_smol!("{v}"));
    impl_visit!(visit_f64, f64, v, &format_smol!("{v}"));

    impl_visit!(visit_bool, bool, v, &format_smol!("{v}"));
    impl_visit!(visit_char, char, v, &format_smol!("{v}"));

    impl_visit!(visit_str, &str, v, v);
    impl_visit!(visit_string, String, v, &v);

    fn visit_none<E: Error>(self) -> Result<Self::Value, E> {
        let Some(default) = self.0.default() else {
            return Err(self.failed());
        };
        let mut boxed = default.reflect_owned().into_opaque().unwrap();
        if let Err(e) = boxed.apply_str("None") {
            ::core::hint::cold_path();
            return Err(Error::custom(e));
        }
        Ok(boxed)
    }

    fn visit_unit<E: Error>(self) -> Result<Self::Value, E> {
        let Some(default) = self.0.default() else {
            return Err(self.failed());
        };
        let mut boxed = default.reflect_owned().into_opaque().unwrap();
        if let Err(e) = boxed.apply_str("()") {
            ::core::hint::cold_path();
            return Err(Error::custom(e));
        }
        Ok(boxed)
    }
}

// -----------------------------------------------------------------------------
// ArrayVisitor
// -----------------------------------------------------------------------------

/// Deserializes fixed-size arrays (`[T; N]`).
///
/// Enters via `deserialize_tuple(len, self)` → `visit_seq`.
///
/// # Two-phase strategy
///
/// | Phase | Condition | Strategy |
/// |-------|-----------|----------|
/// | Native | `TypeDB::default()` exists | Create empty native array, assign each element by index via `reflect_assign` (with `reflect_apply` fallback). |
/// | Dynamic | No default | Build a [`DynamicArray`], push deserialized elements, convert via [`TypeDB::from_reflect`]. |
///
/// Length is validated against [`ArrayInfo::len`] — error on underflow or
/// overflow.
struct ArrayVisitor<'a>(&'a TypeDB);

impl ArrayVisitor<'_> {
    #[cold] // Fixed Array is rare.
    #[inline(always)]
    fn vis<'de, D>(self, deserializer: D) -> Result<Box<dyn Reflect>, D::Error>
    where
        D: Deserializer<'de>,
    {
        use crate::info::ArrayInfo;
        let info: &ArrayInfo = self
            .0
            .type_info
            .as_array()
            .map_err(|e| invalid_info(self.0.type_path, e))?;

        deserializer.deserialize_tuple(info.len(), self)
    }
}

impl<'de> Visitor<'de> for ArrayVisitor<'_> {
    type Value = Box<dyn Reflect>;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("reflected array value")
    }

    fn visit_seq<V: SeqAccess<'de>>(self, mut seq: V) -> Result<Self::Value, V::Error> {
        use crate::dynamic::DynamicArray;
        use crate::info::ArrayInfo;
        use crate::ops::Array;

        let info: &ArrayInfo = self
            .0
            .type_info
            .as_array()
            .map_err(|e| invalid_info(self.0.type_path, e))?;

        let Some(db) = TypeDB::get_by_type(info.item_id()) else {
            return Err(missing_type_db(info.item_info().type_path()));
        };
        let len: usize = info.len();

        // If the container supports Default, construct it directly and mutate in-place (faster).
        if let Some(default) = self.0.default() {
            let mut boxed: Box<dyn Array> = default.reflect_owned().into_array().unwrap();
            debug_assert_eq!(
                boxed.item_len(),
                len,
                "array info's length mismatch `{}`",
                self.0.type_path
            );

            for index in 0..len {
                let Some(v) = seq.next_element_seed(ReflectDeser(db))? else {
                    return Err(make_error(format!(
                        "invalid length, expected: `{len}`, actual: `{}`.",
                        index - 1
                    )));
                };
                let item = boxed.item_mut(index).expect("valid_index");
                if let Err(e) = item.reflect_assign(v) {
                    ::core::hint::cold_path();
                    // In theory, the types should match, no need `reflect_apply`.
                    if let Err(e) = item.reflect_apply(&*e) {
                        return Err(make_error(e));
                    }
                }
            }

            if seq.next_element_seed(IgnoreSeed).map_err(maperr)?.is_some() {
                return Err(make_error(format!(
                    "invalid length, expected: `{len}`, actual: `>{len}`."
                )));
            }

            Ok(boxed)
        } else {
            // Fallback: build a dynamic type and convert via from_reflect.
            let mut dynamic = DynamicArray::with_capacity(len);

            for index in 0..len {
                let Some(v) = seq.next_element_seed(ReflectDeser(db))? else {
                    return Err(make_error(format!(
                        "invalid length, expected: `{len}`, actual: `{}`.",
                        index - 1
                    )));
                };
                dynamic.push(v);
            }

            if seq.next_element_seed(IgnoreSeed).map_err(maperr)?.is_some() {
                return Err(make_error(format!(
                    "invalid length, expected: `{len}`, actual: `>{len}`."
                )));
            }

            match self.0.from_reflect(Box::new(dynamic)) {
                Ok(boxed) => Ok(boxed),
                Err(e) => Err(invalid_convert(self.0.type_path, &*e)),
            }
        }
    }
}

// -----------------------------------------------------------------------------
// ListVisitor
// -----------------------------------------------------------------------------

/// Deserializes growable list-like types (`Vec<T>`, `VecDeque<T>`, …).
///
/// Enters via `deserialize_seq` → `visit_seq`.
///
/// # Two-phase strategy
///
/// | Phase | Condition | Strategy |
/// |-------|-----------|----------|
/// | Native | `TypeDB::default()` exists | Create empty native list, `push_back` each element. |
/// | Dynamic | No default | Build a [`DynamicList`], push elements, convert via [`TypeDB::from_reflect`]. |
///
/// No length validation — lists are variable-length.
struct ListVisitor<'a>(&'a TypeDB);

impl ListVisitor<'_> {
    #[inline(always)]
    fn vis<'de, D>(self, deserializer: D) -> Result<Box<dyn Reflect>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for ListVisitor<'_> {
    type Value = Box<dyn Reflect>;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("reflected list value")
    }

    fn visit_seq<V: SeqAccess<'de>>(self, mut seq: V) -> Result<Self::Value, V::Error> {
        use crate::dynamic::DynamicList;
        use crate::info::ListInfo;
        use crate::ops::List;

        let info: &ListInfo = self
            .0
            .type_info
            .as_list()
            .map_err(|e| invalid_info(self.0.type_path, e))?;

        let Some(db) = TypeDB::get_by_type(info.item_id()) else {
            return Err(missing_type_db(info.item_info().type_path()));
        };

        // If the container supports Default, construct it directly and mutate in-place (faster).
        if let Some(default) = self.0.default() {
            let mut boxed: Box<dyn List> = default.reflect_owned().into_list().unwrap();
            while let Some(v) = seq.next_element_seed(ReflectDeser(db))? {
                if let Err(e) = boxed.push_back(v) {
                    return Err(invalid_convert(self.0.type_path, &*e));
                }
            }
            Ok(boxed)
        } else {
            // Almost all List containers support default.
            ::core::hint::cold_path();
            let hint = seq.size_hint().unwrap_or(0);

            let mut dynamic = DynamicList::with_capacity(hint);
            while let Some(v) = seq.next_element_seed(ReflectDeser(self.0))? {
                dynamic.push(v);
            }

            match self.0.from_reflect(Box::new(dynamic)) {
                Ok(boxed) => Ok(boxed),
                Err(e) => Err(invalid_convert(self.0.type_path, &*e)),
            }
        }
    }
}

// -----------------------------------------------------------------------------
// SetVisitor
// -----------------------------------------------------------------------------

/// Deserializes set-like types (`HashSet<T>`, `BTreeSet<T>`, …).
///
/// Enters via `deserialize_seq` → `visit_seq`.
///
/// # Two-phase strategy
///
/// | Phase | Condition | Strategy |
/// |-------|-----------|----------|
/// | Native | `TypeDB::default()` exists | Create empty native set, `insert_value` each element. |
/// | Dynamic | No default | Build a [`DynamicSet`], insert elements, convert via [`TypeDB::from_reflect`]. |
struct SetVisitor<'a>(&'a TypeDB);

impl SetVisitor<'_> {
    #[inline(always)]
    fn vis<'de, D>(self, deserializer: D) -> Result<Box<dyn Reflect>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for SetVisitor<'_> {
    type Value = Box<dyn Reflect>;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("reflected set value")
    }

    fn visit_seq<V: SeqAccess<'de>>(self, mut seq: V) -> Result<Self::Value, V::Error> {
        use crate::dynamic::DynamicSet;
        use crate::info::SetInfo;
        use crate::ops::Set;

        let info: &SetInfo = self
            .0
            .type_info
            .as_set()
            .map_err(|e| invalid_info(self.0.type_path, e))?;

        let Some(db) = TypeDB::get_by_type(info.value_id()) else {
            return Err(missing_type_db(info.value_info().type_path()));
        };

        // If the container supports Default, construct it directly and mutate in-place (faster).
        if let Some(default) = self.0.default() {
            let mut boxed: Box<dyn Set> = default.reflect_owned().into_set().unwrap();
            while let Some(v) = seq.next_element_seed(ReflectDeser(db))? {
                if let Err(e) = boxed.insert_value(v) {
                    return Err(invalid_convert(self.0.type_path, &*e));
                }
            }
            Ok(boxed)
        } else {
            // Almost all Set containers support default.
            ::core::hint::cold_path();
            let hint = seq.size_hint().unwrap_or(0);
            let mut dynamic = DynamicSet::with_capacity(hint);
            while let Some(v) = seq.next_element_seed(ReflectDeser(db))? {
                dynamic.insert(v);
            }

            match self.0.from_reflect(Box::new(dynamic)) {
                Ok(boxed) => Ok(boxed),
                Err(e) => Err(invalid_convert(self.0.type_path, &*e)),
            }
        }
    }
}

// -----------------------------------------------------------------------------
// MapVisitor
// -----------------------------------------------------------------------------

/// Deserializes map-like types (`HashMap<K, V>`, `BTreeMap<K, V>`, …).
///
/// Enters via `deserialize_map` → `visit_map`.
///
/// # Two-phase strategy
///
/// | Phase | Condition | Strategy |
/// |-------|-----------|----------|
/// | Native | `TypeDB::default()` exists | Create empty native map. For each key-value pair: deserialize key via `ReflectDeser(key_db)`, value via `ReflectDeser(val_db)`, insert with `Map::insert_entry`. |
/// | Dynamic | No default | Build a [`DynamicMap`], insert entries, convert via [`TypeDB::from_reflect`]. |
///
/// Key/value `TypeDB` lookups use [`MapInfo::key_id`] / [`MapInfo::value_id`].
struct MapVisitor<'a>(&'a TypeDB);

impl MapVisitor<'_> {
    #[inline(always)]
    fn vis<'de, D>(self, deserializer: D) -> Result<Box<dyn Reflect>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for MapVisitor<'_> {
    type Value = Box<dyn Reflect>;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("reflected map value")
    }

    fn visit_map<V: MapAccess<'de>>(self, mut map: V) -> Result<Self::Value, V::Error> {
        use crate::dynamic::DynamicMap;
        use crate::info::MapInfo;
        use crate::ops::Map;

        let info: &MapInfo = self
            .0
            .type_info
            .as_map()
            .map_err(|e| invalid_info(self.0.type_path, e))?;

        let Some(key_db) = TypeDB::get_by_type(info.key_id()) else {
            return Err(missing_type_db(info.key_info().type_path()));
        };
        let Some(val_db) = TypeDB::get_by_type(info.value_id()) else {
            return Err(missing_type_db(info.value_info().type_path()));
        };

        // If the container supports Default, construct it directly and mutate in-place (faster).
        if let Some(default) = self.0.default() {
            let mut boxed: Box<dyn Map> = default.reflect_owned().into_map().unwrap();

            while let Some(key) = map.next_key_seed(ReflectDeser(key_db))? {
                let value = map.next_value_seed(ReflectDeser(val_db))?;
                if let Err((k, v)) = boxed.insert_entry(key, value) {
                    ::core::hint::cold_path();
                    return Err(invalid_key_value(self.0.type_path, &*k, &*v));
                }
            }

            Ok(boxed)
        } else {
            // Almost all Map containers support default.
            ::core::hint::cold_path();
            let hint = map.size_hint().unwrap_or(0);
            let mut dynamic = DynamicMap::with_capacity(hint);
            while let Some(key) = map.next_key_seed(ReflectDeser(key_db))? {
                let value = map.next_value_seed(ReflectDeser(val_db))?;
                dynamic.insert(key, value);
            }

            match self.0.from_reflect(Box::new(dynamic)) {
                Ok(boxed) => Ok(boxed),
                Err(e) => Err(invalid_convert(self.0.type_path, &*e)),
            }
        }
    }
}

// -----------------------------------------------------------------------------
// TupleVisitor
// -----------------------------------------------------------------------------

/// Deserializes tuples and tuple-structs (`(A, B)`, `Foo(A, B)`, newtype
/// `Bar(T)`).
///
/// Dispatches the serde entry by ident prefix:
///
/// | Condition | Serde call |
/// |-----------|------------|
/// | `name.starts_with('(')` (basic tuple) | `deserialize_tuple(len, self)` |
/// | `len == 1` (newtype struct) | `deserialize_newtype_struct(name, self)` |
/// | Otherwise (tuple struct) | `deserialize_tuple_struct(name, len, self)` |
///
/// All paths converge to `visit_seq`.  Fixed-length — per-index field type
/// resolved via [`TupleInfo::field`] → `TypeDB` lookup.
struct TupleVisitor<'a>(&'a TypeDB);

impl TupleVisitor<'_> {
    #[inline(always)]
    fn vis<'de, D>(self, deserializer: D) -> Result<Box<dyn Reflect>, D::Error>
    where
        D: Deserializer<'de>,
    {
        use crate::info::TupleInfo;
        let info: &TupleInfo = self
            .0
            .type_info
            .as_tuple()
            .map_err(|e| invalid_info(self.0.type_path, e))?;
        let length = info.field_len();
        let name = info.type_ident();

        if name.starts_with('(') {
            // basic tuple
            deserializer.deserialize_tuple(length, self)
        } else if length == 1 {
            // newtype struct
            deserializer.deserialize_newtype_struct(name, self)
        } else {
            // normal tuple struct
            deserializer.deserialize_tuple_struct(name, length, self)
        }
    }
}

impl<'de> Visitor<'de> for TupleVisitor<'_> {
    type Value = Box<dyn Reflect>;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("reflected tuple of tuple struct value")
    }

    fn visit_seq<V: SeqAccess<'de>>(self, mut seq: V) -> Result<Self::Value, V::Error> {
        use crate::dynamic::DynamicTuple;
        use crate::info::TupleInfo;
        use crate::ops::Tuple;

        let info: &TupleInfo = self
            .0
            .type_info
            .as_tuple()
            .map_err(|e| invalid_info(self.0.type_path, e))?;
        let len: usize = info.field_len();

        // If the container supports Default, construct it directly and mutate in-place (faster).
        if let Some(default) = self.0.default() {
            let mut boxed: Box<dyn Tuple> = default.reflect_owned().into_tuple().unwrap();
            debug_assert_eq!(
                boxed.field_len(),
                len,
                "tuple info's length mismatch `{}`",
                self.0.type_path
            );

            for index in 0..len {
                let field = info.field(index).unwrap();
                let Some(db) = TypeDB::get_by_type(field.type_id()) else {
                    return Err(missing_type_db(field.type_info().type_path()));
                };
                let Some(v) = seq.next_element_seed(ReflectDeser(db))? else {
                    return Err(make_error(format!(
                        "invalid length, expected: `{len}`, actual: `{}`.",
                        index - 1
                    )));
                };
                let item = boxed.field_mut(index).expect("valid_index");
                if let Err(e) = item.reflect_assign(v) {
                    ::core::hint::cold_path();
                    // In theory, the types should match, no need `reflect_apply`.
                    if let Err(e) = item.reflect_apply(&*e) {
                        return Err(make_error(e));
                    }
                }
            }

            if seq.next_element_seed(IgnoreSeed).map_err(maperr)?.is_some() {
                return Err(make_error(format!(
                    "invalid length, expected: `{len}`, actual: `>{len}`."
                )));
            }

            Ok(boxed)
        } else {
            // Fallback: build a dynamic type and convert via from_reflect.
            let mut dynamic = DynamicTuple::with_capacity(len);

            for index in 0..len {
                let field = info.field(index).unwrap();
                let Some(db) = TypeDB::get_by_type(field.type_id()) else {
                    return Err(missing_type_db(field.type_info().type_path()));
                };
                let Some(v) = seq.next_element_seed(ReflectDeser(db))? else {
                    return Err(make_error(format!(
                        "invalid length, expected: `{len}`, actual: `{}`.",
                        index - 1
                    )));
                };
                dynamic.push(v);
            }

            if seq.next_element_seed(IgnoreSeed).map_err(maperr)?.is_some() {
                return Err(make_error(format!(
                    "invalid length, expected: `{len}`, actual: `>{len}`."
                )));
            }

            match self.0.from_reflect(Box::new(dynamic)) {
                Ok(boxed) => Ok(boxed),
                Err(e) => Err(invalid_convert(self.0.type_path, &*e)),
            }
        }
    }
}

// -----------------------------------------------------------------------------
// StructVisitor
// -----------------------------------------------------------------------------

/// Deserializes named structs (`struct Foo { a: i32, b: String }`).
///
/// Enters via `deserialize_struct(name, fields, self)` and handles two
/// formats:
///
/// - **`visit_seq`** (positional): fields deserialized in declaration order.
///   Each field's `TypeDB` is resolved via [`StructInfo::field_at`].
///   Requires exact field count.
/// - **`visit_map`** (key-value): fields deserialized by name.  Unknown
///   keys are silently skipped.  If the type supports `Default`, the native
///   default is created and matching fields are assigned; missing fields
///   retain their default value (supporting `#[reflect(default)]`).
///   Otherwise a [`DynamicStruct`] is built and converted via
///   [`TypeDB::from_reflect`].
struct StructVisitor<'a>(&'a TypeDB);

impl StructVisitor<'_> {
    #[inline(always)]
    fn vis<'de, D>(self, deserializer: D) -> Result<Box<dyn Reflect>, D::Error>
    where
        D: Deserializer<'de>,
    {
        use crate::info::StructInfo;
        let info: &StructInfo = self
            .0
            .type_info
            .as_struct()
            .map_err(|e| invalid_info(self.0.type_path, e))?;
        let name = info.type_ident();
        let fields = info.field_names();
        deserializer.deserialize_struct(name, fields, self)
    }
}

impl<'de> Visitor<'de> for StructVisitor<'_> {
    type Value = Box<dyn Reflect>;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("reflected struct value")
    }

    fn visit_seq<V: SeqAccess<'de>>(self, mut seq: V) -> Result<Self::Value, V::Error> {
        use crate::dynamic::DynamicStruct;
        use crate::info::StructInfo;
        use crate::ops::Struct;

        let info: &StructInfo = self
            .0
            .type_info
            .as_struct()
            .map_err(|e| invalid_info(self.0.type_path, e))?;
        let len: usize = info.field_len();

        // If the container supports Default, construct it directly and mutate in-place (faster).
        if let Some(default) = self.0.default() {
            let mut boxed: Box<dyn Struct> = default.reflect_owned().into_struct().unwrap();
            debug_assert_eq!(
                boxed.field_len(),
                len,
                "struct info's length mismatch `{}`",
                self.0.type_path
            );

            for index in 0..len {
                let field = info.field_at(index).unwrap();
                let Some(db) = TypeDB::get_by_type(field.type_id()) else {
                    return Err(missing_type_db(field.type_info().type_path()));
                };
                let Some(v) = seq.next_element_seed(ReflectDeser(db))? else {
                    return Err(make_error(format!(
                        "invalid length, expected: `{len}`, actual: `{}`.",
                        index - 1
                    )));
                };
                let item = boxed.field_at_mut(index).expect("valid index");
                if let Err(e) = item.reflect_assign(v) {
                    ::core::hint::cold_path();
                    // In theory, the types should match, no need `reflect_apply`.
                    if let Err(e) = item.reflect_apply(&*e) {
                        return Err(make_error(e));
                    }
                }
            }

            if seq.next_element_seed(IgnoreSeed).map_err(maperr)?.is_some() {
                return Err(make_error(format!(
                    "invalid length, expected: `{len}`, actual: `>{len}`."
                )));
            }

            Ok(boxed)
        } else {
            // Fallback: build a dynamic type and convert via from_reflect.
            let mut dynamic = DynamicStruct::with_capacity(len);

            for index in 0..len {
                let field = info.field_at(index).unwrap();
                let Some(db) = TypeDB::get_by_type(field.type_id()) else {
                    return Err(missing_type_db(field.type_info().type_path()));
                };
                let Some(v) = seq.next_element_seed(ReflectDeser(db))? else {
                    return Err(make_error(format!(
                        "invalid length, expected: `{len}`, actual: `{}`.",
                        index - 1
                    )));
                };
                let name = field.name();
                dynamic.push(Cow::Borrowed(name), v);
            }

            if seq.next_element_seed(IgnoreSeed).map_err(maperr)?.is_some() {
                return Err(make_error(format!(
                    "invalid length, expected: `{len}`, actual: `>{len}`."
                )));
            }

            match self.0.from_reflect(Box::new(dynamic)) {
                Ok(boxed) => Ok(boxed),
                Err(e) => Err(invalid_convert(self.0.type_path, &*e)),
            }
        }
    }

    fn visit_map<V: MapAccess<'de>>(self, mut map: V) -> Result<Self::Value, V::Error> {
        use crate::dynamic::DynamicStruct;
        use crate::info::StructInfo;
        use crate::ops::Struct;

        let info: &StructInfo = self
            .0
            .type_info
            .as_struct()
            .map_err(|e| invalid_info(self.0.type_path, e))?;
        let len: usize = info.field_len();

        if let Some(default) = self.0.default() {
            let mut boxed: Box<dyn Struct> = default.reflect_owned().into_struct().unwrap();
            debug_assert_eq!(
                boxed.field_len(),
                len,
                "struct info's length mismatch `{}`",
                self.0.type_path
            );

            while let Some(Ident(key)) = map.next_key::<Ident>().map_err(maperr)? {
                let Some(index) = info.index_of(&key) else {
                    let _ = map.next_value_seed(IgnoreSeed).map_err(maperr)?;
                    continue;
                };
                let field = info.field_at(index).expect("valid index");

                let Some(db) = TypeDB::get_by_type(field.type_id()) else {
                    return Err(missing_type_db(field.type_info().type_path()));
                };
                let v = map.next_value_seed(ReflectDeser(db))?;

                let item = boxed.field_at_mut(index).expect("valid index");
                if let Err(e) = item.reflect_assign(v) {
                    ::core::hint::cold_path();
                    // In theory, the types should match, no need `reflect_apply`.
                    if let Err(e) = item.reflect_apply(&*e) {
                        return Err(make_error(e));
                    }
                }
            }

            Ok(boxed)
        } else {
            let mut dynamic = DynamicStruct::with_capacity(len);

            while let Some(Ident(key)) = map.next_key::<Ident>().map_err(maperr)? {
                let Some(field) = info.field(&key) else {
                    let _ = map.next_value_seed(IgnoreSeed).map_err(maperr)?;
                    continue;
                };

                let Some(db) = TypeDB::get_by_type(field.type_id()) else {
                    return Err(missing_type_db(field.type_info().type_path()));
                };
                let value = map.next_value_seed(ReflectDeser(db))?;

                dynamic.insert(Cow::Owned(key), value);
            }

            match self.0.from_reflect(Box::new(dynamic)) {
                Ok(boxed) => Ok(boxed),
                Err(e) => Err(invalid_convert(self.0.type_path, &*e)),
            }
        }
    }
}

// -----------------------------------------------------------------------------
// OptionVisitor
// -----------------------------------------------------------------------------

/// Specialized visitor for `Option<T>`, invoked via
/// [`Deserializer::deserialize_option`].
///
/// - **`visit_none`**: Creates the `None` variant (index 1).  If the native
///   type has a default constructor, uses it directly.
/// - **`visit_some`**: Looks up the `Some` variant in [`EnumInfo`], resolves
///   the inner type's `TypeDB`, deserializes the inner value via
///   `ReflectDeser`, wraps in a [`DynamicTuple`], and converts to the
///   native type via [`TypeDB::from_reflect`].
struct OptionVisitor<'a>(&'a TypeDB);

impl<'de> Visitor<'de> for OptionVisitor<'_> {
    type Value = Box<dyn Reflect>;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("reflected option value of type ")?;
        formatter.write_str(self.0.type_path)
    }

    fn visit_none<E: Error>(self) -> Result<Self::Value, E> {
        use crate::dynamic::DynamicEnum;

        if let Some(default) = self.0.default() {
            debug_assert_eq!(default.reflect_kind(), ReflectKind::Enum);
            Ok(default)
        } else {
            ::core::hint::cold_path();
            match self
                .0
                .from_reflect(Box::new(DynamicEnum::new(1, "None", ())))
            {
                Ok(ret) => Ok(ret),
                Err(e) => Err(invalid_convert(self.0.type_path, &*e)),
            }
        }
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        use crate::dynamic::{DynamicEnum, DynamicTuple};
        use crate::info::{EnumInfo, VariantInfo};

        let info: &EnumInfo = self
            .0
            .type_info
            .as_enum()
            .map_err(|e| invalid_info(self.0.type_path, e))?;

        let Some(variant_info) = info.variant("Some") else {
            return Err(make_error(format!(
                "invalid variant, expected `Some(_)` but got: {info:?}"
            )));
        };

        match variant_info {
            VariantInfo::Tuple(tuple_info) if tuple_info.field_len() == 1 => {
                let field = tuple_info.field(0).unwrap();

                let Some(db) = TypeDB::get_by_type(field.type_id()) else {
                    let path = field.type_info().type_path();
                    return Err(make_error(format!("no TypeDB found for type `{path}`")));
                };

                let field = ReflectDeser(db).deserialize(deserializer)?;
                let mut variant = DynamicTuple::with_capacity(1);
                variant.push(field);
                let dynamic = DynamicEnum::new(0, "Some", variant);

                match self.0.from_reflect(Box::new(dynamic)) {
                    Ok(ret) => Ok(ret),
                    Err(e) => Err(invalid_convert(self.0.type_path, &*e)),
                }
            }
            _ => Err(make_error(format!(
                "invalid variant, expected `Some(_)` but got: {info:?}"
            ))),
        }
    }
}

// -----------------------------------------------------------------------------
// VariantVisitor
// -----------------------------------------------------------------------------

/// Deserializes an enum variant identifier.
///
/// Accepts a variant index (`u32` / `u64`) or variant name (`&str`),
/// resolved against the given [`EnumInfo`].
///
/// Used by [`EnumVisitor::visit_enum`] via [`EnumAccess::variant_seed`].
struct VariantVisitor(&'static crate::info::EnumInfo);

impl<'de> Visitor<'de> for VariantVisitor {
    type Value = &'static crate::info::VariantInfo;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("expected either a variant index or variant name")
    }

    fn visit_u32<E: Error>(self, index: u32) -> Result<Self::Value, E> {
        match self.0.variant_at(index as usize) {
            Some(val) => Ok(val),
            None => {
                ::core::hint::cold_path();
                let type_path = self.0.type_path();
                Err(make_error(format!(
                    "no variant found at index `{index}` on enum `{type_path}`"
                )))
            }
        }
    }

    fn visit_u64<E: Error>(self, index: u64) -> Result<Self::Value, E> {
        match self.0.variant_at(index as usize) {
            Some(val) => Ok(val),
            None => {
                ::core::hint::cold_path();
                let type_path = self.0.type_path();
                Err(make_error(format!(
                    "no variant found at index `{index}` on enum `{type_path}`"
                )))
            }
        }
    }

    fn visit_str<E: Error>(self, name: &str) -> Result<Self::Value, E> {
        match self.0.variant(name) {
            Some(val) => Ok(val),
            None => {
                ::core::hint::cold_path();
                let type_path = self.0.type_path();
                Err(make_error(format!(
                    "no variant found with name `{name}` on enum `{type_path}`"
                )))
            }
        }
    }
}

impl<'de> DeserializeSeed<'de> for VariantVisitor {
    type Value = &'static crate::info::VariantInfo;

    #[inline]
    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_identifier(self)
    }
}

// -----------------------------------------------------------------------------
// EnumVisitor
// -----------------------------------------------------------------------------

/// Deserializes enum types.
///
/// Dispatches by type identity:
///
/// | Condition | Serde call |
/// |-----------|------------|
/// | `Option<T>` (type ident `"Option"` from `core::option`) | `deserialize_option(OptionVisitor)` |
/// | All other enums | `deserialize_enum(name, variants, self)` |
///
/// # `visit_enum` workflow
///
/// 1. Deserialize variant identifier via [`VariantVisitor`] (index or name).
/// 2. Deserialize variant data via [`EnumAccess`] / [`VariantAccess`]:
///    - **Unit** → [`DynamicVariant::Unit`].
///    - **Tuple (1 field)** → newtype → `newtype_variant_seed`.
///    - **Tuple (N fields)** → `tuple_variant(len, TupleVariantVisitor)`.
///    - **Struct** → `struct_variant(names, StructVariantVisitor)`.
/// 3. Construct a [`DynamicEnum`] with the resolved variant info.
/// 4. Convert to native type via [`TypeDB::from_reflect`].
struct EnumVisitor<'a>(&'a TypeDB);

impl EnumVisitor<'_> {
    #[inline(always)]
    fn vis<'de, D>(self, deserializer: D) -> Result<Box<dyn Reflect>, D::Error>
    where
        D: Deserializer<'de>,
    {
        use crate::info::EnumInfo;
        let info: &'static EnumInfo = self
            .0
            .type_info
            .as_enum()
            .map_err(|e| invalid_info(self.0.type_path, e))?;

        if info.type_ident() == "Option" && info.module_path() == Some("core::option") {
            deserializer.deserialize_option(OptionVisitor(self.0))
        } else {
            let name = info.type_ident();
            let variants = info.variant_names();
            deserializer.deserialize_enum(name, variants, EnumVisitor(self.0))
        }
    }
}

impl<'de> Visitor<'de> for EnumVisitor<'_> {
    type Value = Box<dyn Reflect>;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("reflected enum value")
    }

    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<Self::Value, A::Error> {
        use crate::dynamic::{DynamicEnum, DynamicTuple, DynamicVariant};
        use crate::info::{EnumInfo, VariantInfo};

        let info: &'static EnumInfo = self
            .0
            .type_info
            .as_enum()
            .map_err(|e| invalid_info(self.0.type_path, e))?;

        let (variant_info, variant) = data.variant_seed(VariantVisitor(info))?;

        let value: DynamicVariant = match variant_info {
            VariantInfo::Unit(_) => variant.unit_variant().map_err(maperr)?.into(),
            VariantInfo::Tuple(info) if info.field_len() == 1 => {
                let field_info = info.field(0).unwrap();
                let Some(db) = TypeDB::get_by_type(field_info.type_id()) else {
                    let path = field_info.type_info().type_path();
                    return Err(make_error(format!("no TypeDB found for type `{path}`")));
                };
                let value = variant.newtype_variant_seed(ReflectDeser(db))?;
                let mut dynamic = DynamicTuple::with_capacity(1);
                dynamic.push(value);
                DynamicVariant::Tuple(dynamic)
            }
            VariantInfo::Tuple(info) => {
                let field_len = info.field_len();
                variant
                    .tuple_variant(field_len, TupleVariantVisitor(info))?
                    .into()
            }
            VariantInfo::Struct(info) => {
                let field_names = info.field_names();
                variant
                    .struct_variant(field_names, StructVariantVisitor(info))?
                    .into()
            }
        };

        let variant_name = variant_info.name();
        let variant_index = info.index_of(variant_name).unwrap();
        let dynamic_enum = DynamicEnum::new(variant_index, variant_name, value);

        match self.0.from_reflect(Box::new(dynamic_enum)) {
            Ok(ret) => Ok(ret),
            Err(e) => Err(invalid_convert(self.0.type_path, &*e)),
        }
    }
}

// -----------------------------------------------------------------------------
// TupleVariantVisitor
// -----------------------------------------------------------------------------

/// Deserializes the fields of a tuple enum variant.
///
/// Driven by [`VariantAccess::tuple_variant`] → `visit_seq`.  Each field's
/// type is resolved via [`TupleVariantInfo::field`] → `TypeDB` lookup.
/// Produces a [`DynamicTuple`].
struct TupleVariantVisitor(&'static crate::info::TupleVariantInfo);

impl<'de> Visitor<'de> for TupleVariantVisitor {
    type Value = crate::dynamic::DynamicTuple;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("reflected tuple variant value")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        use crate::dynamic::DynamicTuple;

        let len: usize = self.0.field_len();

        let mut dynamic = DynamicTuple::with_capacity(len);

        for index in 0..len {
            let field = self.0.field(index).unwrap();
            let Some(db) = TypeDB::get_by_type(field.type_id()) else {
                return Err(missing_type_db(field.type_info().type_path()));
            };
            let Some(v) = seq.next_element_seed(ReflectDeser(db))? else {
                return Err(make_error(format!(
                    "invalid length, expected: `{len}`, actual: `{}`.",
                    index - 1
                )));
            };
            dynamic.push(v);
        }

        if seq.next_element_seed(IgnoreSeed).map_err(maperr)?.is_some() {
            return Err(make_error(format!(
                "invalid length, expected: `{len}`, actual: `>{len}`."
            )));
        }

        Ok(dynamic)
    }
}

// -----------------------------------------------------------------------------
// StructVariantVisitor
// -----------------------------------------------------------------------------

/// Deserializes the fields of a struct enum variant.
///
/// Driven by [`VariantAccess::struct_variant`].  Handles two formats:
///
/// - **`visit_seq`** (positional): fields deserialized in declaration order.
/// - **`visit_map`** (key-value): fields deserialized by name.  Unknown
///   keys are silently skipped.
///
/// Produces a [`DynamicStruct`].
struct StructVariantVisitor(&'static crate::info::StructVariantInfo);

impl<'de> Visitor<'de> for StructVariantVisitor {
    type Value = crate::dynamic::DynamicStruct;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("reflected struct variant value")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        use crate::dynamic::DynamicStruct;

        let len: usize = self.0.field_len();

        let mut dynamic = DynamicStruct::with_capacity(len);

        for index in 0..len {
            let field = self.0.field_at(index).unwrap();
            let Some(db) = TypeDB::get_by_type(field.type_id()) else {
                return Err(missing_type_db(field.type_info().type_path()));
            };
            let Some(v) = seq.next_element_seed(ReflectDeser(db))? else {
                return Err(make_error(format!(
                    "invalid length, expected: `{len}`, actual: `{}`.",
                    index - 1
                )));
            };
            let name = field.name();
            dynamic.push(Cow::Borrowed(name), v);
        }

        if seq.next_element_seed(IgnoreSeed).map_err(maperr)?.is_some() {
            return Err(make_error(format!(
                "invalid length, expected: `{len}`, actual: `>{len}`."
            )));
        }

        Ok(dynamic)
    }

    fn visit_map<V: MapAccess<'de>>(self, mut map: V) -> Result<Self::Value, V::Error> {
        use crate::dynamic::DynamicStruct;

        let len: usize = self.0.field_len();
        let mut dynamic = DynamicStruct::with_capacity(len);

        while let Some(Ident(key)) = map.next_key::<Ident>().map_err(maperr)? {
            let Some(field) = self.0.field(&key) else {
                let _ = map.next_value_seed(IgnoreSeed)?;
                continue;
            };

            let Some(db) = TypeDB::get_by_type(field.type_id()) else {
                return Err(missing_type_db(field.type_info().type_path()));
            };
            let value = map.next_value_seed(ReflectDeser(db))?;

            dynamic.insert(Cow::Owned(key), value);
        }

        Ok(dynamic)
    }
}
