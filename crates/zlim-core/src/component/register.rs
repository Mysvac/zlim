//! Component registration entry points.

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
use super::db::{ComponentDB, ID_REGISTRY, PATH_REGISTRY, TYPE_REGISTRY};
use super::{Component, ComponentId};
use crate::entity::EntityMapper;

// -----------------------------------------------------------------------------
// Registration
// -----------------------------------------------------------------------------

/// Registers a component type **without** serialization support.
///
/// This is the registration used by [`Component::register`]'s default
/// implementation and by `#[derive(Component)]` unless
/// `#[component(serialize)]` is present.  The returned [`ComponentDB`]
/// has its serialization function pointers set to `None`.
///
/// Registration is idempotent: if `C` is already registered, the existing
/// entry is returned without creating a duplicate.
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::component::register_base;
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Position;
///
/// let db = register_base::<Position>();
/// assert!(db.serialize.is_none());
/// ```
#[cold]
#[inline(never)]
pub fn register_base<C: Component>() -> &'static ComponentDB {
    register_impl::<C>(None, None)
}

/// Registers a component type **with** serialization support.
///
/// In addition to the base metadata, this fills the
/// [`ComponentDB::serialize`] / [`ComponentDB::deserialize`] function
/// pointers so the component can be serialized into scenes.  The derive
/// macro uses this when the type is annotated with
/// `#[component(serialize)]`.
///
/// Registration is idempotent: if `C` is already registered, the existing
/// entry is returned without creating a duplicate.
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::clone::ComponentCloner;
/// use zlim_core::component::register_serializable;
/// use serde::{Deserialize, Serialize};
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Clone, Serialize, Deserialize)]
/// struct Position;
///
/// // Manual `Component` implementation that opts into serialization support:
/// impl Component for Position {
///     const SERIALIZE: bool = true;
///
///     fn register() -> &'static ComponentDB {
///         register_serializable::<Self>()
///     }
///
///     const CLONER: ComponentCloner = ComponentCloner::clonable::<Self>();
/// }
///
/// let db = register_serializable::<Position>();
/// assert!(db.serialize.is_some());
/// ```
#[cold]
#[inline(never)]
pub fn register_serializable<C: Component + Serialize + for<'de> Deserialize<'de>>()
-> &'static ComponentDB {
    register_impl::<C>(Some(serialize_fn::<C>), Some(deserialize_fn::<C>))
}

/// Shared registration core: builds and stores the [`ComponentDB`] for `C`.
#[cold]
#[inline]
fn register_impl<C: Component>(
    serialize: Option<SerializeFunc>,
    deserialize: Option<DeserializeFunc>,
) -> &'static ComponentDB {
    let type_id = TypeId::of::<C>();

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
        C::SERIALIZE,
        "`<{} as Component>::SERIALIZE` does not match the register used internally.\
        If the type needs serialization, `register_serializable` should be used.",
        C::type_path(),
    );

    let mut db = ComponentDB {
        id: ComponentId::without_provenance(0),
        type_id: TypeId::of::<C>(),
        type_path: C::type_path(),
        type_name: C::type_name(),
        module_path: C::MODULE.unwrap_or(""),
        required: C::REQUIRED,
        on_add: C::ON_ADD,
        on_clone: C::ON_CLONE,
        on_insert: C::ON_INSERT,
        on_remove: C::ON_REMOVE,
        on_discard: C::ON_DISCARD,
        on_despawn: C::ON_DESPAWN,
        getter: C::GETTER,
        setter: C::SETTER,
        get_field_func: get_field::<C>,
        set_field_func: set_field::<C>,
        layout: Layout::new::<C>(),
        dropper: C::DROPPER,
        cloner: C::CLONER,
        map_entities: map_entities_fn::<C>,
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

    db.id = ComponentId::without_provenance(id_guard.len());
    let db: &'static ComponentDB = unsafe { Global::alloc_unchecked(db) };

    type_guard.insert(type_id, db);
    id_guard.push(db);

    ::core::mem::drop(id_guard);
    ::core::mem::drop(type_guard);

    PATH_REGISTRY
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(db.type_path, db);

    // Register required components (recursively).
    if let Some(required) = C::REQUIRED {
        required.register();
    }

    db
}

// -----------------------------------------------------------------------------
// Type-erased helpers
// -----------------------------------------------------------------------------

/// Type-erased field getter for `C::get_field`.
fn get_field<'a, C: Component>(ptr: Ptr<'a>, name: &str) -> Option<&'a dyn Reflect> {
    ptr.debug_assert_aligned::<C>();
    unsafe { ptr.deref::<C>().get_field(name) }
}

/// Type-erased field setter for `C::set_field`.
fn set_field<'a, C: Component>(
    ptr: PtrMut<'a>,
    name: &str,
    value: &dyn Reflect,
) -> Result<(), String> {
    ptr.debug_assert_aligned::<C>();
    unsafe { ptr.deref::<C>().set_field(name, value) }
}

/// Type-erased entity mapper for `C::map_entities`.
fn map_entities_fn<'a, C: Component>(ptr: PtrMut<'a>, mut mapper: &mut dyn EntityMapper) {
    ptr.debug_assert_aligned::<C>();
    unsafe {
        ptr.deref::<C>()
            .map_entities::<&mut dyn EntityMapper>(&mut mapper);
    }
}

/// Type-erased serializer for `C`.
fn serialize_fn<C: Component + Serialize>(ptr: Ptr<'_>) -> &dyn ErasedSerialize {
    ptr.debug_assert_aligned::<C>();
    unsafe { ptr.deref::<C>() as &dyn ErasedSerialize }
}

/// Type-erased deserializer for `C`.
fn deserialize_fn<'b, C: Component + for<'de> Deserialize<'de>>(
    deserializer: &mut dyn ErasedDeserializer<'_>,
    bump: &'b Bump,
) -> Result<OwningPtr<'b>, erased_serde::Error> {
    let value: C = erased_serde::deserialize(deserializer)?;
    let ptr = unsafe { bump.alloc_unchecked(value) };
    let inner = NonNull::from_mut(ptr).cast::<u8>();
    Ok(unsafe { OwningPtr::new(inner) })
}

// -----------------------------------------------------------------------------
