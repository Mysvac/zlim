//! Resource registration entry points.

use core::alloc::Layout;
use core::any::TypeId;
use core::ptr::NonNull;
use std::sync::PoisonError;

use erased_serde::Deserializer as ErasedDeserializer;
use erased_serde::Serialize as ErasedSerialize;
use serde::{Deserialize, Serialize};
use zlim_ptr::{OwningPtr, Ptr, PtrMut};
use zlim_reflect::Reflect;
use zlim_utils::mem::{Bump, Global};

use super::alias::{DeserializeFunc, SerializeFunc};
use super::db::{ID_REGISTRY, PATH_REGISTRY, ResourceDB, TYPE_REGISTRY};
use super::id::ResourceId;
use super::resource::Resource;

// -----------------------------------------------------------------------------
// Registration
// -----------------------------------------------------------------------------

/// Registers a [`Resource`] type `R` **without** serialization support.
///
/// This is the registration used by [`Resource::register`]'s default
/// implementation and by `#[derive(Resource)]` unless
/// `#[resource(serialize)]` is present.  The returned [`ResourceDB`] has its
/// serialization function pointers set to `None`.
///
/// Registration is idempotent: if `R` is already registered, the existing
/// entry is returned without creating a duplicate. This function is marked
/// `#[cold]` because it should only execute once per type during the
/// application lifetime.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::resource::register_base;
///
/// #[derive(TypePath, Resource)]
/// struct Score(u32);
///
/// // Usually reached through `ResourceDB::of` / `Resource::register`
/// // instead of being called directly:
/// let db = register_base::<Score>();
/// assert_eq!(db.type_name, "Score");
/// assert!(db.serialize.is_none());
/// ```
///
/// [`Resource`]: crate::resource::Resource
/// [`ResourceDB`]: crate::resource::ResourceDB
/// [`Resource::register`]: crate::resource::Resource::register
#[cold]
#[inline(never)]
pub fn register_base<R: Resource>() -> &'static ResourceDB {
    register_impl::<R>(None, None)
}

/// Registers a [`Resource`] type `R` **with** serialization support.
///
/// In addition to the base metadata, this fills the
/// [`ResourceDB::serialize`] / [`ResourceDB::deserialize`] function
/// pointers so the resource can be serialized into scenes.  The derive
/// macro uses this when the type is annotated with
/// `#[resource(serialize)]`.
///
/// Registration is idempotent: if `R` is already registered, the existing
/// entry is returned without creating a duplicate.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::resource::register_serializable;
/// use serde::{Deserialize, Serialize};
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Clone, Serialize, Deserialize)]
/// struct Score(u32);
///
/// // Manual `Resource` implementation that opts into serialization support:
/// impl Resource for Score {
///     const SERIALIZE: bool = true;
///
///     fn register() -> &'static ResourceDB {
///         register_serializable::<Self>()
///     }
/// }
///
/// let db = register_serializable::<Score>();
/// assert!(db.serialize.is_some());
/// ```
#[cold]
#[inline(never)]
pub fn register_serializable<R: Resource + Serialize + for<'de> Deserialize<'de>>()
-> &'static ResourceDB {
    register_impl::<R>(Some(serialize_fn::<R>), Some(deserialize_fn::<R>))
}

/// Shared registration core: builds and stores the [`ResourceDB`] for `R`.
#[cold]
#[inline]
fn register_impl<R: Resource>(
    serialize: Option<SerializeFunc>,
    deserialize: Option<DeserializeFunc>,
) -> &'static ResourceDB {
    let type_id = TypeId::of::<R>();

    // Quick read-check first — hot path when already registered.
    let type_guard = TYPE_REGISTRY.read().unwrap_or_else(PoisonError::into_inner);
    if let Some(existing) = type_guard.get(type_id) {
        return existing;
    }
    ::core::mem::drop(type_guard);

    ::core::hint::cold_path();

    debug_assert_eq!(serialize.is_some(), deserialize.is_some());

    assert_eq!(
        serialize.is_some(),
        R::SERIALIZE,
        "`<{} as Resource>::SERIALIZE` does not match the register used internally.\
        If the type needs serialization, `register_serializable` should be used.",
        R::type_path(),
    );

    let mut db = ResourceDB {
        id: ResourceId::without_provenance(0),
        type_id: TypeId::of::<R>(),
        type_path: R::type_path(),
        type_name: R::type_name(),
        module_path: R::MODULE.unwrap_or(""),
        getter: R::GETTER,
        setter: R::SETTER,
        get_field_func: get_field::<R>,
        set_field_func: set_field::<R>,
        layout: Layout::new::<R>(),
        dropper: R::DROPPER,
        serialize,
        deserialize,
    };

    let mut type_guard = TYPE_REGISTRY
        .write()
        .unwrap_or_else(PoisonError::into_inner);

    if let Some(existing) = type_guard.get(type_id) {
        return existing;
    }

    let mut id_guard = ID_REGISTRY.write().unwrap_or_else(PoisonError::into_inner);

    db.id = ResourceId::without_provenance(id_guard.len());
    let db: &'static ResourceDB = unsafe { Global::alloc_unchecked(db) };

    type_guard.insert(type_id, db);
    id_guard.push(db);

    ::core::mem::drop(id_guard);
    ::core::mem::drop(type_guard);

    PATH_REGISTRY
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(db.type_path, db);

    db
}

// -----------------------------------------------------------------------------
// Type-erased helpers
// -----------------------------------------------------------------------------

/// Type-erased field getter for `R::get_field`.
fn get_field<'a, R: Resource>(ptr: Ptr<'a>, name: &str) -> Option<&'a dyn Reflect> {
    ptr.debug_assert_aligned::<R>();
    unsafe { ptr.deref::<R>().get_field(name) }
}

/// Type-erased field setter for `R::set_field`.
fn set_field<'a, R: Resource>(
    ptr: PtrMut<'a>,
    name: &str,
    value: &dyn Reflect,
) -> Result<(), String> {
    ptr.debug_assert_aligned::<R>();
    unsafe { ptr.deref::<R>().set_field(name, value) }
}

/// Type-erased serializer for `R`.
fn serialize_fn<R: Resource + Serialize>(ptr: Ptr<'_>) -> &dyn ErasedSerialize {
    ptr.debug_assert_aligned::<R>();
    unsafe { ptr.deref::<R>() as &dyn ErasedSerialize }
}

/// Type-erased deserializer for `R`.
fn deserialize_fn<'b, R: Resource + for<'de> Deserialize<'de>>(
    deserializer: &mut dyn ErasedDeserializer<'_>,
    bump: &'b Bump,
) -> Result<OwningPtr<'b>, erased_serde::Error> {
    let value: R = erased_serde::deserialize(deserializer)?;
    let ptr = unsafe { bump.alloc_unchecked(value) };
    let inner = NonNull::from_mut(ptr).cast::<u8>();
    Ok(unsafe { OwningPtr::new(inner) })
}
