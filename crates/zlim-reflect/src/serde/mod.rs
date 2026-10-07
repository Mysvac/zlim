//! Context-carrying serde support for reflected values.
//!
//! Reflection can serialize and deserialize a value whose *shape* is all that is known, but some
//! types need more than their own fields to be read back: a handle needs the asset system to resolve a
//! path, a component needs a registry to look up a type by name. [`ReflectContext`] is how such a type
//! reaches that ambient state.
//!
//! # The three pieces
//!
//! - [`ReflectContext`] — an `Any + Sync` value that is handed to a serialization or deserialization,
//!   and that a type can ask questions of by type. `()` implements it, and is the context of
//!   everything that needs none.
//! - [`ReflectSerialize`] / [`ReflectDeserialize`] — the per-type hooks that do the work. Both are
//!   implemented for every `Reflect` type that is `Serialize` / `Deserialize`, which covers what
//!   [`TypeDB`] registers through [`insert_serializer`] and [`insert_deserializer`].
//! - [`EMPTY_CONTEXT`] — the shared empty context, for the entry points that take none.
//!
//! # Which way the context flows
//!
//! A context is *read* by the hook, never written to: it is handed down by shared reference, so one
//! context serves every nested value of a whole tree. A type that has to accumulate something across
//! values keeps that in interior mutability of its own rather than in the context.
//!
//! # Writing a hook
//!
//! The blanket implementations cover the serde-derived case, so a hand-written hook is only needed
//! for a type that the derived one cannot describe — one that has to consult the context, which also
//! means it must not be `Serialize` or `Deserialize`, since the two implementations would overlap:
//!
//! ```rust,ignore
//! impl ReflectSerialize for Tagged {
//!     fn reflect_serialize(
//!         value: &dyn Reflect,
//!         serializer: &mut dyn ErasedSerializer,
//!         ctx: &dyn ReflectContext,
//!     ) -> Result<(), ErasedError> {
//!         let value = value.downcast_ref::<Tagged>().expect("this hook is `Tagged`'s");
//!         let registry = ctx.get::<Registry>().expect("a registry is required");
//!         serializer.serialize_str(registry.name_of(value))
//!     }
//! }
//! ```
//!
//! [`TypeDB`]: crate::db::TypeDB
//! [`insert_serializer`]: crate::db::TypeDB::insert_serializer
//! [`insert_deserializer`]: crate::db::TypeDB::insert_deserializer

use core::any::Any;
use core::any::TypeId;

use erased_serde::Deserializer as ErasedDeserializer;
use erased_serde::Error as ErasedError;
use erased_serde::Serializer as ErasedSerializer;
use serde_core::de::Deserialize;
use serde_core::ser::Serialize;
use zlim_ptr::Ptr;

use crate::Reflect;

// -----------------------------------------------------------------------------
// ReflectContext
// -----------------------------------------------------------------------------

/// Ambient state that a serialization or deserialization is given, and that a type may consult.
///
/// A hook receives `&dyn ReflectContext` and asks for what it needs by type, so the caller decides
/// what is available and the type decides what it uses:
///
/// ```
/// use zlim_reflect::serde::ReflectContext;
///
/// struct Registry(u32);
///
/// # #[expect(unsafe_code, reason = "`ReflectContext` is an unsafe trait")]
/// unsafe impl ReflectContext for Registry {}
///
/// fn takes_context(ctx: &dyn ReflectContext) {
///     assert_eq!(ctx.get::<Registry>().map(|r| r.0), Some(7));
///     assert!(ctx.get::<String>().is_none());
/// }
///
/// takes_context(&Registry(7));
/// ```
///
/// # Implementing
///
/// The default implementation is correct for any type that only holds itself: it answers for its own
/// `TypeId` and `None` for every other, so `unsafe impl ReflectContext for MyType {}` is enough.
///
/// Note the bound: `Any` requires `'static`, so a context cannot borrow from a caller's stack.
///
/// # Safety
///
/// `dyn ReflectContext::get` turns what [`get_ptr`](ReflectContext::get_ptr) returns into a
/// reference without checking it, so an implementation must keep the contract documented on
/// `get_ptr`: the pointer it produces for a `TypeId` must be a valid reference to a value of that
/// type, and must stay valid for as long as the context is borrowed. The default implementation
/// satisfies this by construction.
#[expect(unsafe_code, reason = "the trait is unsafe by contract, for `get`")]
pub unsafe trait ReflectContext: Any + Sync {
    /// Names this context, for diagnostics.
    fn debug(&self) -> &'static str {
        core::any::type_name::<Self>()
    }

    /// Returns a pointer to the value this context holds for `ty`, if it has one.
    ///
    /// # Contract
    ///
    /// The returned pointer must be a valid reference to a value of the type `ty` names, and must
    /// stay valid for as long as the context is borrowed.
    fn get_ptr(&self, ty: TypeId) -> Option<Ptr<'_>> {
        if ty == TypeId::of::<Self>() {
            Some(Ptr::from_ref(self))
        } else {
            None
        }
    }
}

/// The context of a serialization that needs none.
#[expect(unsafe_code, reason = "`ReflectContext` is an unsafe trait")]
unsafe impl ReflectContext for () {}

/// The shared empty context, for entry points that take no context.
///
/// A `const` rather than a plain `&()` so that every use of it is the same reference, and so that a
/// caller who wants to pass nothing has a name to write.
pub const EMPTY_CONTEXT: &() = &();

impl dyn ReflectContext {
    /// Downcasts this context to `T`, if it is one.
    #[inline]
    pub fn downcast<T: Any>(&self) -> Option<&T> {
        <dyn Any>::downcast_ref(self)
    }

    /// Returns the value this context holds for `T`, if it has one.
    ///
    /// # Panics
    ///
    /// Panics if the pointer the context returned for `TypeId::of::<T>()` is not aligned for `T`,
    /// which means [`get_ptr`](ReflectContext::get_ptr) answered with something it should not have.
    #[inline]
    pub fn get<T: Any>(&self) -> Option<&T> {
        let ty = TypeId::of::<T>();
        let ptr = self.get_ptr(ty)?;
        ptr.debug_assert_aligned::<T>();
        #[expect(
            unsafe_code,
            reason = "`get_ptr` promises a reference of the type asked for"
        )]
        return Some(unsafe { ptr.deref::<T>() });
    }
}

// -----------------------------------------------------------------------------
// ReflectSerialize
// -----------------------------------------------------------------------------

/// Serializes a reflected value, with access to a [`ReflectContext`].
///
/// This is what a [`TypeDB`](crate::db::TypeDB) stores for a type that can be handed straight to
/// serde, and what a serializer calls when it finds a hook for the value in front of it. The blanket
/// implementation covers every `Serialize + Reflect` type — everything that derives both — and
/// ignores the context; a type that reads the context writes its own hook instead.
///
/// The value arrives as `&dyn Reflect` rather than as `Self` so that one hook can be stored for the
/// whole type: it downcasts, and reports a mismatch rather than misreading a value of another type.
///
/// A hook writes through an *erased* serializer, so it cannot return the caller's `Ok`: that is `()`
/// here, and the caller recovers its own where it is still known.
pub trait ReflectSerialize: Reflect {
    /// Writes `value`, which must be a `Self`, into `serializer`.
    ///
    /// # Errors
    ///
    /// Returns an error if `value` is not a `Self`, or if the write fails.
    fn reflect_serialize(
        value: &dyn Reflect,
        serializer: &mut dyn ErasedSerializer,
        ctx: &dyn ReflectContext,
    ) -> Result<(), ErasedError>;
}

impl<P: Serialize + Reflect> ReflectSerialize for P {
    fn reflect_serialize(
        value: &dyn Reflect,
        serializer: &mut dyn ErasedSerializer,
        _: &dyn ReflectContext,
    ) -> Result<(), ErasedError> {
        #[cold]
        #[inline(never)]
        fn missing_type(value: &dyn Reflect, expected: &str) -> ErasedError {
            <ErasedError as serde_core::ser::Error>::custom(format_args!(
                "Type mismatched, expected: `{expected}`, actual: `{}`",
                value.reflect_type_name(),
            ))
        }

        match value.downcast_ref::<P>() {
            Some(value) => erased_serde::Serialize::erased_serialize(value, serializer),
            None => Err(missing_type(value, ::core::any::type_name::<P>())),
        }
    }
}

// -----------------------------------------------------------------------------
// ReflectDeserialize
// -----------------------------------------------------------------------------

/// Deserializes a reflected value, with access to a [`ReflectContext`].
///
/// This is what a [`TypeDB`](crate::db::TypeDB) stores for a type that can be handed straight to
/// serde, and what a deserializer calls when it finds a hook for the type it is reading. The blanket
/// implementation covers every `Deserialize + Reflect` type — everything that derives both — and
/// ignores the context; a type that reads the context writes its own hook instead.
///
/// # Errors
///
/// Returns an error if the input does not describe a `Self`.
pub trait ReflectDeserialize: Reflect {
    /// Deserializes a `Self` through `deserializer`.
    ///
    /// The deserializer is erased, and `erased_serde::Deserializer` names the lifetime its input
    /// borrows for. That lifetime is left to the caller rather than fixed here: this hook is stored as
    /// a function pointer, so the pointer is higher-ranked over it and works for input of any
    /// lifetime.
    fn reflect_deserialize(
        deserializer: &mut dyn ErasedDeserializer<'_>,
        ctx: &dyn ReflectContext,
    ) -> Result<Box<dyn Reflect>, ErasedError>;
}

impl<P: for<'de> Deserialize<'de> + Reflect> ReflectDeserialize for P {
    fn reflect_deserialize(
        deserializer: &mut dyn ErasedDeserializer<'_>,
        _ctx: &dyn ReflectContext,
    ) -> Result<Box<dyn Reflect>, ErasedError> {
        Ok(Box::new(erased_serde::deserialize::<P>(deserializer)?))
    }
}
