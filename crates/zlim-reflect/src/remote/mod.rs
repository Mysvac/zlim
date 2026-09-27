//! Remote reflection: describing a type that lives in another crate.
//!
//! A type can only be given reflection where it is defined — the derive needs its fields, and the
//! orphan rule needs a local type — so a type from a library the engine does not own cannot be
//! reflected directly. What *can* be done is to describe a **wrapper** of that type in a crate that
//! is allowed to reflect: the wrapper carries the field layout, and the two types are converted
//! into each other where a reflected value is read or written.
//!
//! [`ReflectRemote`] is that pair. Its conversions are what a wrapper adds beyond [`Reflect`]:
//! reflection reaches the *remote* type through the wrapper, so the `&dyn Reflect` a field accessor
//! hands out is really a `&dyn Reflect` of the type behind it.
//!
//! ```rust, no_run
//! # use std::fmt::{Debug, Formatter};
//! # use zlim_reflect::db::{TypeDB, TypeDatabase};
//! # use zlim_reflect::info::{OpaqueInfo, ReflectKind, TypeInfo, Typed};
//! # use zlim_reflect::ops::{ApplyError, CloneError, Opaque, ReflectMut};
//! # use zlim_reflect::ops::{ReflectOwned, ReflectRef};
//! # use zlim_reflect::remote::ReflectRemote;
//! # use zlim_reflect::{Reflect, TypePath};
//! #
//! /// The type this crate cannot reflect: it belongs to someone else.
//! mod some_lib {
//!     #[derive(Debug, Default, Clone)]
//!     pub struct TheirType {
//!         pub value: u32,
//!     }
//! }
//!
//! /// Its description: a newtype over the remote value, whose conversion is the reinterpretation
//! /// `#[repr(transparent)]` allows.
//! ///
//! /// `Reflect` is written by hand rather than derived, because the derive would read the newtype's
//! /// field — the remote value — and require reflection of it, which is exactly what it has not got.
//! #[derive(Default, Clone, TypePath)]
//! #[repr(transparent)]
//! struct TheirTypeRemote(some_lib::TheirType);
//!
//! impl Typed for TheirTypeRemote {
//! #    fn type_info() -> &'static TypeInfo { todo!() }
//!     /* .. */
//! }
//!
//! impl Reflect for TheirTypeRemote {
//! #    fn reflect_kind(&self) -> ReflectKind { todo!() }
//! #    fn reflect_ref(&self) -> ReflectRef<'_> { todo!() }
//! #    fn reflect_mut(&mut self) -> ReflectMut<'_> { todo!() }
//! #    fn reflect_owned(self: Box<Self>) -> ReflectOwned { todo!() }
//! #    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> { todo!() }
//! #    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> { todo!() }
//! #    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>> { todo!() }
//!     /* .. */
//! }
//!
//! impl TypeDatabase for TheirTypeRemote {
//! #    fn on_register(_: &'static TypeDB) {}
//! #    fn register_dependencies() {}
//!     /* .. */
//! }
//!
//! impl ReflectRemote for TheirTypeRemote {
//!     type Remote = some_lib::TheirType;
//!
//!     fn as_remote(&self) -> &Self::Remote { &self.0 }
//!     fn as_remote_mut(&mut self) -> &mut Self::Remote { &mut self.0 }
//!     fn into_remote(self) -> Self::Remote { self.0 }
//!     fn as_wrapper(remote: &Self::Remote) -> &Self { unsafe { core::mem::transmute(remote) } }
//!     fn as_wrapper_mut(remote: &mut Self::Remote) -> &mut Self { unsafe { core::mem::transmute(remote) } }
//!     fn into_wrapper(remote: Self::Remote) -> Self { Self(remote) }
//! }
//!
//! /// A description of a type of *this* crate, which holds the remote type in a field.
//! #[derive(Reflect, TypePath)]
//! struct Holder {
//!     #[reflect(remote = TheirTypeRemote)]
//!     data: some_lib::TheirType,
//! }
//! ```

use crate::Reflect;

// -----------------------------------------------------------------------------
// ReflectRemote
// -----------------------------------------------------------------------------

/// A reflected wrapper for a type that lives in another crate.
///
/// The wrapper is what the engine reflects and stores; the [`Remote`](Self::Remote) type is what
/// the library's own code sees. The conversions between them are what the generated reflection
/// uses, so a field marked `#[reflect(remote = Wrapper)]` is *reached through* the wrapper while the
/// value the entity holds stays the remote type.
///
/// # Coherence
///
/// The wrapper must be `#[repr(transparent)]` over its remote type for
/// [`as_wrapper`](Self::as_wrapper) and [`as_wrapper_mut`](Self::as_wrapper_mut) to be sound: the
/// two then have the same layout, and the borrow can be reinterpreted. A manual implementation may
/// choose another representation, but it then has to convert for real — the other direction,
/// [`as_remote`](Self::as_remote), needs no such promise.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a remote wrapper for another type",
    label = "not a remote wrapper",
    note = "a field is marked `#[reflect(remote = ...)]`, \
            so the type named there must implement `ReflectRemote`"
)]
pub trait ReflectRemote: Reflect {
    /// The type this wrapper describes.
    ///
    /// It is the type the library hands out, and the one a field marked
    /// `#[reflect(remote = ...)]` actually holds.
    type Remote;

    /// Borrows the remote value behind this wrapper.
    fn as_remote(&self) -> &Self::Remote;

    /// Borrows the remote value behind this wrapper mutably.
    fn as_remote_mut(&mut self) -> &mut Self::Remote;

    /// Takes the remote value out of this wrapper.
    fn into_remote(self) -> Self::Remote;

    /// Borrows a remote value as this wrapper.
    ///
    /// This is the direction that needs the two to share a representation — see the trait's note on
    /// coherence.
    fn as_wrapper(remote: &Self::Remote) -> &Self;

    /// Borrows a remote value as this wrapper, mutably.
    ///
    /// Like [`as_wrapper`](Self::as_wrapper), this needs the two to share a representation.
    fn as_wrapper_mut(remote: &mut Self::Remote) -> &mut Self;

    /// Wraps a remote value.
    fn into_wrapper(remote: Self::Remote) -> Self;
}

// -----------------------------------------------------------------------------
