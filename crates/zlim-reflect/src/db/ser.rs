//! Serialization support for the reflection system.
//!
//! # Entry Points
//!
//! Consider a struct `A { x: 3, y: 4 }` registered under the type path
//! `"my_crate::A"`.  The two entry points produce different JSON:
//!
//! | Function | JSON output |
//! |---|---|
//! | [`reflect_serialize`] | `{"my_crate::A":{"x":3,"y":4}}` |
//! | [`serialize`] | `{"x":3,"y":4}` |
//!
//! [`reflect_serialize`] wraps the payload in a single-entry map keyed by
//! the type path.  This is **self-describing** — the output can be
//! deserialized without knowing the target type in advance.
//! [`serialize`] produces only the payload, matching standard serde output.
//! It is used internally for nested fields during recursive serialization.
//!
//! # Serialization priority
//!
//! 1. **Registered Serializer First**: if the value's [`TypeDB`] has a hook
//!    (set via `insert_serializer`), use it directly — the **fast path** for
//!    types with a serde `Serialize` implementation. The hook writes through
//!    an erased serializer, and the caller recovers its own `Ok` afterwards.
//!
//! 2. **Reflection Fallback**: otherwise inspect [`Reflect::reflect_ref`]
//!    and delegate to the kind-specific function:
//!
//!    | ReflectRef kind | Function | Serde call |
//!    |---|---|---|
//!    | `Opaque` | `serialize_opaque` | `serialize_str` (via [`Opaque::stringify`]) |
//!    | `Struct` | `serialize_struct` | `serialize_struct` |
//!    | `Tuple`  | `serialize_tuple`  | `serialize_tuple` / `serialize_tuple_struct` |
//!    | `Array`  | `serialize_array`  | `serialize_tuple` |
//!    | `List`   | `serialize_list`   | `serialize_seq` |
//!    | `Map`    | `serialize_map`    | `serialize_map` |
//!    | `Set`    | `serialize_set`    | `serialize_seq` |
//!    | `Enum`   | `serialize_enum`   | `serialize_enum` |
//!
//! # Opaque fallback
//!
//! When an opaque type has no registered hook, the serializer falls
//! back to calling [`Opaque::stringify`] and serializing the result as a
//! string.  This ensures all reflected types are serializable even without
//! explicit serde support.
//!
//! [`serialize`]: TypeDB::serialize()
//! [`reflect_serialize`]: TypeDB::reflect_serialize
//! [`TypeDB`]: super::TypeDB
//! [`insert_serializer`]: TypeDB::insert_serializer
//! [`ReflectRef`]: crate::ops::ReflectRef
//! [`Reflect::reflect_ref`]: crate::Reflect::reflect_ref
//! [`Opaque::stringify`]: crate::ops::Opaque::stringify

use core::any::TypeId;
use core::fmt::Display;
use core::mem::size_of;

use serde_core::ser::Error;
use serde_core::ser::{SerializeMap, SerializeSeq, SerializeStruct};
use serde_core::ser::{SerializeStructVariant, SerializeTuple};
use serde_core::ser::{SerializeTupleStruct, SerializeTupleVariant};
use serde_core::{Serialize, Serializer};
use zlim_log as log;
use zlim_utils::debug::DebugLocation;

use super::{TypeDB, TypeDatabase};
use crate::Reflect;
use crate::info::ReflectKindError;
use crate::ops::Opaque;
use crate::ops::ReflectRef;
use crate::serde::{EMPTY_CONTEXT, ReflectContext, ReflectSerialize};

// -----------------------------------------------------------------------------
// Register
// -----------------------------------------------------------------------------

/// Logs a message when the same serializer is registered more than once.
#[cold]
#[inline(never)]
fn warn_serializer_dup(ty: &'static str, l: DebugLocation) {
    log::trace!("{l}: `{ty}`'s serializer registered repeatedly; ignored.");
}

impl TypeDB {
    /// Returns `true` if a serializer has been registered for this type.
    #[inline]
    pub fn contains_serializer(&self) -> bool {
        self.serialize.get().is_some()
    }

    /// Registers the [`ReflectSerialize`] hook of `T` into this `TypeDB`.
    ///
    /// The hook is `T`'s own — the blanket implementation, or a hand-written one — so this only
    /// stores it. Once registered, serializing this type calls the hook directly (the **fast path**),
    /// bypassing the kind-specific dispatch.
    ///
    /// # Returns
    ///
    /// `true` on first registration, `false` if a serializer was already
    /// registered (a message is logged and the original is kept).
    ///
    /// # Panics
    ///
    /// Panics if `self` does not belong to type `T`.
    #[cold]
    #[inline(never)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn insert_serializer<T>(&self) -> bool
    where
        T: TypeDatabase + ReflectSerialize,
    {
        #[cold]
        #[inline(never)]
        fn panicked(e: &'static str, a: &'static str, l: DebugLocation) -> ! {
            panic!(
                "{l}: `insert_serializer` type mismatch — \
                TypeDB is for `{e}`, but the Serialize need `{a}`."
            )
        }

        if self.id != TypeId::of::<T>() {
            panicked(self.type_path, T::type_path(), DebugLocation::caller());
        }

        if self.serialize.set(T::reflect_serialize).is_err() {
            warn_serializer_dup(T::type_path(), DebugLocation::caller());
            false
        } else {
            true
        }
    }

    /// Convenience wrapper: resolves `T`'s [`TypeDB`] via
    /// [`TypeDB::of`](TypeDB::of) then calls
    /// `insert_serializer`(Self::insert_serializer).
    #[cold]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn register_serializer<T>() -> bool
    where
        T: TypeDatabase + ReflectSerialize,
    {
        let db = TypeDB::of::<T>();
        db.insert_serializer::<T>()
    }
}

// -----------------------------------------------------------------------------
// serialize
// -----------------------------------------------------------------------------

impl TypeDB {
    /// Self-describing serializer for reflected types.
    ///
    /// # Example
    ///
    /// For a struct `A { x: 3, y: 4 }` registered as `"my_crate::A"`:
    ///
    /// ```text
    /// {"my_crate::A": {"x": 3, "y": 4}}
    /// ```
    ///
    /// The type path wrapping is applied **only at the outermost level**.
    /// Inner values are serialized via [`serialize`] (no recursive wrapping).
    ///
    /// # Serialization Rules
    ///
    /// Internally delegates to `TypePathReflectSer`, which:
    ///
    /// 1. Wraps the value in a single-entry map keyed by type path.
    /// 2. Serializes the payload via `ReflectSer`, which follows the
    ///    two-step priority order described in [`serialize`].
    ///
    /// This is the counterpart to [`reflect_deserialize`].
    ///
    /// # Use when
    ///
    /// The format must be **self-describing** — the output carries its own
    /// type information so the deserializer can resolve the target type
    /// automatically.
    ///
    /// If the target type is already known, use [`serialize`] instead.
    ///
    /// [`serialize`]: TypeDB::serialize()
    /// [`reflect_deserialize`]: TypeDB::reflect_deserialize
    #[inline]
    pub fn reflect_serialize<S>(value: &dyn Reflect, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        TypePathReflectSer(value, EMPTY_CONTEXT).serialize(serializer)
    }

    /// Serializes a reflected value directly, **without** type path wrapping.
    ///
    /// See [`TypeDB::reflect_serialize`] for the shape of the output, and the
    /// [module documentation](self) for the dispatch order.
    #[inline]
    pub fn serialize<S>(value: &dyn Reflect, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ReflectSer(value, EMPTY_CONTEXT).serialize(serializer)
    }

    /// Self-describing serializer with a context for reflected types.
    ///
    /// The counterpart of [`reflect_deserialize_with`](TypeDB::reflect_deserialize_with); see
    /// [`TypeDB::reflect_serialize`] for the output.
    #[inline]
    pub fn reflect_serialize_with<S>(
        value: &dyn Reflect,
        serializer: S,
        ctx: &dyn ReflectContext,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        TypePathReflectSer(value, ctx).serialize(serializer)
    }

    /// Serializes a reflected value directly with a context, **without** type path wrapping.
    ///
    /// See [`TypeDB::serialize`] for the output.
    #[inline]
    pub fn serialize_with<S>(
        value: &dyn Reflect,
        serializer: S,
        ctx: &dyn ReflectContext,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ReflectSer(value, ctx).serialize(serializer)
    }
}

// -----------------------------------------------------------------------------
// Helper
// -----------------------------------------------------------------------------

crate::cfg::debug! {
    std::thread_local! {
        static TYPE_INFO_STACK: ::core::cell::RefCell<super::TypeInfoStack> =
            const { ::core::cell::RefCell::new(super::TypeInfoStack::new()) };
    }
}

/// Centralized error constructor.
///
/// In debug mode, appends the current [`TypeInfoStack`] to the error message
/// for diagnostic context.
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

/// Error-mapping helper for compound serializer entry points.
///
/// In debug mode, enriches the error with the current [`TypeInfoStack`].
/// In release mode, passes the error through unchanged.
///
/// This is applied at each `serializer.serialize_*()` call (the boundary
/// where a compound serializer is created) so that every error path
/// benefits from the type stack context.
#[cold]
#[cfg_attr(any(debug_assertions, feature = "debug"), inline(never))]
#[cfg_attr(not(any(debug_assertions, feature = "debug")), inline(always))]
fn maperr<E: Error>(e: E) -> E {
    crate::cfg::debug! {
        if {
            TYPE_INFO_STACK.with_borrow(|stack| {
                E::custom(format_args!("{e} (stack:\n{stack:?}\n)"))
            })
        } else {
            e
        }
    }
}

/// Produces an error for a [`ReflectKind`] mismatch on a [`TypeInfo`] lookup.
///
/// This indicates the value's runtime kind does not match its compile-time
/// metadata — typically a bug in the type registration or a dynamic type
/// being used where a static type is expected.
#[cold]
#[inline(never)]
fn invalid_info<E: Error>(ty: &'static str, error: ReflectKindError) -> E {
    make_error(format_args!(
        "Invalid type info for `{ty}`, `{error}`. \
        There may be a dynamic type that cannot be deserialized."
    ))
}

// -----------------------------------------------------------------------------
// TypePathReflectSer — top-level wrapper with type path
// -----------------------------------------------------------------------------

/// Wraps a `&dyn Reflect` for self-describing serialization.
///
/// Produces `{ "type_path": <payload> }` by delegating the payload to
/// `ReflectSer`.  This is the counterpart to
/// [`TypePathReflectDeser`](super::des::TypePathReflectDeser).
struct TypePathReflectSer<'a>(&'a dyn Reflect, &'a dyn ReflectContext);

impl Serialize for TypePathReflectSer<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_map(Some(1))?;

        let type_path = self.0.reflect_type_path();
        s.serialize_entry(type_path, &ReflectSer(self.0, self.1))?;

        s.end()
    }
}

// -----------------------------------------------------------------------------
// UnitOk
// -----------------------------------------------------------------------------

const _: () = const {
    assert!(size_of::<()>() == 0);
};

/// Returns the `Ok` of a serializer that wrote a value whose type has a registered hook.
///
/// Such a value is written through its hook, and a hook takes an *erased* serializer — whose `Ok` is
/// `()` by the nature of the erasure, so the `Ok` the caller asked for is gone by the time the hook
/// returns. `Serialize::serialize` is shared by every serializer and cannot carry a bound that says
/// "your `Ok` is `()`", so the two are compared here instead.
///
/// A type with no size *is* `()`: nothing else can be held in zero bytes, which is why measuring is
/// enough. If the caller's `Ok` is that, it is `()`. If it is anything else, this is a serializer
/// that reflective serialization cannot use, and that is reported rather than returned as a value.
///
/// # Errors
///
/// Returns `Err` if `S::Ok` is not `()`.
#[inline]
fn unit_ok<S: Serializer>(_: ()) -> Result<S::Ok, S::Error> {
    if const { size_of::<S::Ok>() != 0 } {
        #[cold]
        #[inline(never)]
        fn wrong_ok<E: Error>() -> E {
            E::custom(
                "a reflected value with a registered serializer was written through an erased \
                 serializer, whose `Ok` is `()`, but this serializer's `Ok` is not `()`. Use a \
                 serializer that produces `()` — `TypeDB::serialize` requires one.",
            )
        }

        return Err(wrong_ok());
    }

    #[expect(unsafe_code, reason = "initialize ZST")]
    #[expect(clippy::uninit_assumed_init, reason = "ZST")]
    Ok(unsafe { ::core::mem::MaybeUninit::<S::Ok>::uninit().assume_init() })
}

// -----------------------------------------------------------------------------
// ReflectSer — central dispatch wrapper
// -----------------------------------------------------------------------------

/// Wraps a `&dyn Reflect` for direct serialization (no type path wrapping).
///
/// # Dispatch order
///
/// 1. **Fast path:** if the value's [`TypeDB`] has a registered hook, the value is written through an
///    erased serializer — a hook is a function pointer, so that is the only serializer it can name.
/// 2. **Slow path:** inspect [`Reflect::reflect_ref`] and dispatch to the
///    kind-specific function (`serialize_opaque`, `serialize_struct`,
///    …, `serialize_enum`).
///
/// Used internally for recursive serialization of nested values.
struct ReflectSer<'a>(&'a dyn Reflect, &'a dyn ReflectContext);

impl Serialize for ReflectSer<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let Some(db) = TypeDB::get_by_type(self.0.type_id())
            && let Some(f) = db.serialize.get()
        {
            let mut ser = <dyn erased_serde::Serializer>::erase(serializer);
            return f(self.0, &mut ser, self.1)
                .map_err(make_error)
                .and_then(unit_ok::<S>);
        }

        crate::cfg::debug! {
            TYPE_INFO_STACK.with_borrow_mut(|stack| stack.push(self.0.reflect_type_info()));
        }

        let returne_value: Result<S::Ok, S::Error> = match self.0.reflect_ref() {
            ReflectRef::Opaque(r) => serialize_opaque(r, serializer),
            ReflectRef::Struct(r) => serialize_struct(r, serializer, self.1),
            ReflectRef::Tuple(r) => serialize_tuple(r, serializer, self.1),
            ReflectRef::Array(r) => serialize_array(r, serializer, self.1),
            ReflectRef::List(r) => serialize_list(r, serializer, self.1),
            ReflectRef::Map(r) => serialize_map(r, serializer, self.1),
            ReflectRef::Set(r) => serialize_set(r, serializer, self.1),
            ReflectRef::Enum(r) => serialize_enum(r, serializer, self.1),
        };

        crate::cfg::debug! {
            TYPE_INFO_STACK.with_borrow_mut(|stack| stack.pop());
        }

        returne_value
    }
}

// -----------------------------------------------------------------------------
// Opaque
// -----------------------------------------------------------------------------

/// Serializes an opaque value.
///
/// Calls [`Opaque::stringify`] to convert the value to its compact string
/// representation, then serializes that string.
///
/// Types that need custom serialization (e.g. via serde derive) should
/// register a hook via `insert_serializer`(TypeDB::insert_serializer)
/// — those types bypass this path entirely when serialized through an
/// erased serde context.
#[cold] // Opaque type should provide serializer, this function is usually unused.
#[inline(always)]
fn serialize_opaque<S>(value: &dyn Opaque, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&value.stringify()).map_err(maperr)
}

// -----------------------------------------------------------------------------
// Array
// -----------------------------------------------------------------------------

/// Serializes a fixed-size array as a serde tuple.
#[inline(never)]
fn serialize_array<S>(
    value: &dyn crate::ops::Array,
    serializer: S,
    ctx: &dyn ReflectContext,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let len = value.item_len();
    let mut s = serializer.serialize_tuple(len).map_err(maperr)?;

    for i in 0..len {
        let item = value.item(i).expect("valid index");
        s.serialize_element(&ReflectSer(item, ctx))?;
    }

    s.end().map_err(maperr)
}

// -----------------------------------------------------------------------------
// List
// -----------------------------------------------------------------------------

/// Serializes a growable list as a serde sequence.
#[inline(never)]
fn serialize_list<S>(
    value: &dyn crate::ops::List,
    serializer: S,
    ctx: &dyn ReflectContext,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let len = value.item_len();
    let mut s = serializer.serialize_seq(Some(len)).map_err(maperr)?;

    for i in 0..len {
        let item = value.item(i).expect("valid index");
        s.serialize_element(&ReflectSer(item, ctx))?;
    }

    s.end().map_err(maperr)
}

// -----------------------------------------------------------------------------
// Set
// -----------------------------------------------------------------------------

/// Serializes a set as a serde sequence.
#[inline(never)]
fn serialize_set<S>(
    value: &dyn crate::ops::Set,
    serializer: S,
    ctx: &dyn ReflectContext,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let len = value.value_len();
    let mut s = serializer.serialize_seq(Some(len)).map_err(maperr)?;
    for v in value.iter_values() {
        s.serialize_element(&ReflectSer(v, ctx))?;
    }
    s.end().map_err(maperr)
}

// -----------------------------------------------------------------------------
// Map
// -----------------------------------------------------------------------------

/// Serializes a map as a serde map.
#[inline(never)]
fn serialize_map<S>(
    value: &dyn crate::ops::Map,
    serializer: S,
    ctx: &dyn ReflectContext,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let len = value.entry_len();
    let mut s = serializer.serialize_map(Some(len)).map_err(maperr)?;
    for (k, v) in value.iter_entries() {
        s.serialize_entry(&ReflectSer(k, ctx), &ReflectSer(v, ctx))?;
    }
    s.end().map_err(maperr)
}

// -----------------------------------------------------------------------------
// Tuple
// -----------------------------------------------------------------------------

/// Serializes a tuple / tuple-struct.
///
/// Dispatches to the appropriate serde method based on the type ident:
///
/// | Condition | Serde call |
/// |-----------|------------|
/// | `name.starts_with('(')` (basic tuple) | `serialize_tuple` |
/// | `len == 1` (newtype struct) | `serialize_newtype_struct` |
/// | Otherwise (tuple struct) | `serialize_tuple_struct` |
///
/// All paths use `ReflectSer` for recursive field serialization.
#[inline(never)]
fn serialize_tuple<S>(
    value: &dyn crate::ops::Tuple,
    serializer: S,
    ctx: &dyn ReflectContext,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let len = value.field_len();
    let name = value.reflect_type_ident();

    if name.starts_with('(') {
        let mut s = serializer.serialize_tuple(len).map_err(maperr)?;
        for i in 0..len {
            let field = value.field(i).expect("valid index");
            s.serialize_element(&ReflectSer(field, ctx))?;
        }
        s.end().map_err(maperr)
    } else if len == 1 {
        let field = value.field(0).expect("valid index");
        serializer.serialize_newtype_struct(name, &ReflectSer(field, ctx))
    } else {
        let mut s = serializer
            .serialize_tuple_struct(name, len)
            .map_err(maperr)?;
        for i in 0..len {
            let field = value.field(i).expect("valid index");
            s.serialize_field(&ReflectSer(field, ctx))?;
        }
        s.end().map_err(maperr)
    }
}

// -----------------------------------------------------------------------------
// Struct
// -----------------------------------------------------------------------------

/// Serializes a struct as a serde struct with named fields.
///
/// Uses the static [`StructInfo`] to obtain field names and validate the
/// field count.  If a field declared in `StructInfo` is missing from the
/// runtime value, an error is returned.  Each field value is recursively
/// serialized via `ReflectSer`.
///
/// [`StructInfo`]: crate::info::StructInfo
#[inline(never)]
fn serialize_struct<S>(
    value: &dyn crate::ops::Struct,
    serializer: S,
    ctx: &dyn ReflectContext,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    use crate::info::StructInfo;

    let struct_info: &StructInfo = value
        .reflect_type_info()
        .as_struct()
        .map_err(|e| invalid_info(value.reflect_type_path(), e))?;

    let len = value.field_len();
    let names = struct_info.field_names();
    let name = struct_info.type_ident();

    if names.len() != len {
        return Err(make_error(format!(
            "Field count mismatch: expect `{}` has {} fields, actual `{}` has {len} fields",
            struct_info.type_path(),
            names.len(),
            value.reflect_type_path(),
        )));
    }

    let mut s = serializer.serialize_struct(name, len).map_err(maperr)?;

    for name in names {
        // If fields match in type and count but a field is missing, panic directly.
        let Some(field) = value.field(name) else {
            return Err(make_error(format!(
                "Missing struct field `{name}` for type `{}`, actual data `{value:?}`",
                struct_info.type_path(),
            )));
        };
        s.serialize_field(name, &ReflectSer(field, ctx))?;
    }

    s.end().map_err(maperr)
}

// -----------------------------------------------------------------------------
// Enum
// -----------------------------------------------------------------------------

/// Serializes an enum.
///
/// # Validation
///
/// Before serializing, validates that:
/// - The variant index exists in [`EnumInfo`].
/// - The variant name matches the info for that index.
/// - The field count and [`VariantKind`] are consistent.
///
/// # Dispatch
///
/// | Variant | Condition | Serde call |
/// |---------|-----------|------------|
/// | Unit | `Option` type | `serialize_none` |
/// | Unit | Other | `serialize_unit_variant` |
/// | Tuple (1 field) | `Option` type | `serialize_some` |
/// | Tuple (1 field) | Other (newtype) | `serialize_newtype_variant` |
/// | Tuple (N fields) | — | `serialize_tuple_variant` |
/// | Struct | — | `serialize_struct_variant` |
///
/// Each field is recursively serialized via `ReflectSer`.
///
/// [`EnumInfo`]: crate::info::EnumInfo
/// [`VariantKind`]: crate::info::VariantKind
#[inline(never)]
fn serialize_enum<S>(
    value: &dyn crate::ops::Enum,
    serializer: S,
    ctx: &dyn ReflectContext,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    use crate::info::{EnumInfo, VariantInfo};

    let enum_info: &EnumInfo = value
        .reflect_type_info()
        .as_enum()
        .map_err(|e| invalid_info(value.reflect_type_path(), e))?;

    let name = enum_info.type_ident();
    let module_path = enum_info.module_path();
    let is_option = name == "Option" && module_path == Some("core::option");

    let variant_index = value.variant_index();
    let Some(variant_info) = enum_info.variant_at(variant_index) else {
        return Err(make_error(format!(
            "variant index `{variant_index}` does not exist for `{}`",
            enum_info.type_path(),
        )));
    };
    let variant_index = variant_index as u32;
    let variant_name = variant_info.name();
    let field_len = variant_info.field_len();
    let is_new_type = field_len == 1;

    if variant_name != value.variant_name() {
        return Err(make_error(format!(
            "Variant name mismatched for same index, expect: `{}::{}`, actual data: `{}::{}`",
            enum_info.type_path(),
            variant_name,
            value.reflect_type_path(),
            value.variant_name(),
        )));
    }

    if field_len != value.field_len() {
        return Err(make_error(format!(
            "Field count mismatch: expect `{}::{}` has {} fields, actual `{}::{}` has {} fields",
            enum_info.type_path(),
            variant_name,
            variant_info.field_len(),
            value.reflect_type_path(),
            value.variant_name(),
            value.field_len(),
        )));
    }

    if value.variant_kind() != variant_info.variant_kind() {
        return Err(make_error(format!(
            "Variant kind mismatched for same index, expect: `{}`, actual data: `{}`",
            variant_info.variant_kind(),
            value.variant_kind(),
        )));
    }

    match variant_info {
        VariantInfo::Unit(_) if is_option => serializer.serialize_none(),
        VariantInfo::Unit(_) => {
            serializer.serialize_unit_variant(name, variant_index, variant_name)
        }
        VariantInfo::Tuple(_) if is_option => {
            let field = value.field_at(0).expect("valid index");
            debug_assert_eq!(field_len, 1, "Option + Tuple, must be Some(x)");
            serializer.serialize_some(&ReflectSer(field, ctx))
        }
        VariantInfo::Tuple(_) if is_new_type => {
            let field = value.field_at(0).expect("valid index");
            serializer.serialize_newtype_variant(
                name,
                variant_index,
                variant_name,
                &ReflectSer(field, ctx),
            )
        }
        VariantInfo::Tuple(_) => {
            let mut s = serializer
                .serialize_tuple_variant(name, variant_index, variant_name, field_len)
                .map_err(maperr)?;

            for i in 0..field_len {
                let field = value.field_at(i).expect("valid index");
                s.serialize_field(&ReflectSer(field, ctx))?;
            }

            s.end().map_err(maperr)
        }
        VariantInfo::Struct(info) => {
            let mut s = serializer
                .serialize_struct_variant(name, variant_index, variant_name, field_len)
                .map_err(maperr)?;

            for i in 0..field_len {
                let field = value.field_at(i).expect("valid index");
                let fname = info.name_at(i).unwrap();
                s.serialize_field(fname, &ReflectSer(field, ctx))?;
            }

            s.end().map_err(maperr)
        }
    }
}
