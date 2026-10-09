//! Component registration entry points.

use core::alloc::Layout;
use core::any::TypeId;
use std::sync::PoisonError;

use zlim_reflect::Reflect;
use zlim_reflect::TypeDB;
use zlim_reflect::db::TypeDatabase;
use zlim_utils::debug::DebugName;
use zlim_utils::mem::Global;

use super::db::{ID_REGISTRY, PATH_REGISTRY, TYPE_REGISTRY};
use super::{Component, ComponentDB, ComponentId};
use crate::component::ReflectComponent;
use crate::template::ComponentTemplate;
use crate::template::ReflectTemplate;
use crate::utils::Dropper;

// -----------------------------------------------------------------------------
// Registration
// -----------------------------------------------------------------------------

#[inline(never)]
fn fast_path(id: TypeId) -> Option<&'static ComponentDB> {
    TYPE_REGISTRY
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .get(id)
        .copied()
}

/// Registers a component type **without** reflect support.
///
/// Registration is idempotent: if `R` is already registered, the existing
/// entry is returned without creating a duplicate.
///
/// Without reflection there is no [`TypeDB`] to take the names from, so
/// `type_path` / `type_name` / `module_path` are parsed out of [`core::any::type_name`].
/// That name is not stable, which is why a resource registered this way is kept
/// out of the path registry and cannot be found by [`ComponentDB::get_by_path`].
#[inline]
pub fn register_base<C: Component>() -> &'static ComponentDB {
    if let Some(db) = fast_path(TypeId::of::<C>()) {
        return db;
    }

    register_impl::<C>(None, None, None)
}

/// Registers a [`Component`] type `R` **with** reflection support.
///
/// The names then come from the [`TypeDB`] instead of [`core::any::type_name`],
/// so the resource is also filed in the path registry and can be found by
/// [`ComponentDB::get_by_path`].
#[inline]
pub fn register_reflect<C: Component + TypeDatabase>() -> &'static ComponentDB {
    if let Some(db) = fast_path(TypeId::of::<C>()) {
        return db;
    }
    let type_db = Some(TypeDB::of::<C>());
    let reflect = Some(ReflectComponent::new::<C>());
    register_impl::<C>(None, type_db, reflect)
}

/// Registers a [`Component`] type `R` **with** serialization support.
///
/// The names then come from the [`TypeDB`] instead of [`core::any::type_name`],
/// so the resource is also filed in the path registry and can be found by
/// [`ComponentDB::get_by_path`].
///
/// A serialized component is also [`Clone`], because a scene document describes a value that can be
/// applied more than once, and the template of a value is built by cloning it — see
/// [`ComponentDB::into_template`].
#[inline]
pub fn register_serialize<C: Component + TypeDatabase + Clone>() -> &'static ComponentDB {
    if let Some(db) = fast_path(TypeId::of::<C>()) {
        return db;
    }
    let type_db = Some(TypeDB::of::<C>());
    let reflect = Some(ReflectComponent::new::<C>());
    register_impl::<C>(Some(into_template::<C>), type_db, reflect)
}

/// Wraps a deserialized component value in the template that describes it.
///
/// The value arrives as a reflected value of the component's own type, so the template is that very
/// value reinterpreted: it is already allocated, and copying it into a fresh
/// [`ComponentTemplate`] would buy nothing.
#[expect(
    unsafe_code,
    reason = "`ComponentTemplate<C>` is `repr(transparent)` over `C`"
)]
fn into_template<C: Reflect + Component + Clone>(
    value: Box<dyn Reflect>,
) -> Box<dyn ReflectTemplate> {
    #[cold]
    #[inline(never)]
    fn failed(name: DebugName) -> ! {
        panic!("a reflected value of `{name}`")
    }

    let value: Box<C> = value
        .downcast::<C>()
        .unwrap_or_else(|_| failed(DebugName::type_name::<C>()));

    let ptr: *mut C = Box::leak(value);

    // SAFETY: the two types have the same layout, and the pointer came from `Box<C>`,
    // which is the allocation `Box<ComponentTemplate<C>>` would have used.
    unsafe { Box::from_raw(ptr as *mut ComponentTemplate<C>) }
}

#[cold]
#[inline(never)]
#[expect(unsafe_code, reason = "specify sections to accelerate registeration")]
#[cfg_attr(target_family = "windows", unsafe(link_section = ".ZINIT"))]
#[cfg_attr(target_family = "wasm", unsafe(link_section = ".text.zliminit"))]
#[cfg_attr(target_os = "linux", unsafe(link_section = ".text.zliminit"))]
#[cfg_attr(target_os = "android", unsafe(link_section = ".text.zliminit"))]
#[cfg_attr(target_os = "macos", unsafe(link_section = "__TEXT,__zlim_init"))]
#[cfg_attr(target_os = "ios", unsafe(link_section = "__TEXT,__zlim_init"))]
fn register_impl<C: Component>(
    into_template: Option<fn(Box<dyn Reflect>) -> Box<dyn ReflectTemplate>>,
    type_db: Option<&'static TypeDB>,
    reflect: Option<ReflectComponent>,
) -> &'static ComponentDB {
    debug_assert_eq!(type_db.is_some(), reflect.is_some());

    let type_id = TypeId::of::<C>();

    ::core::hint::cold_path();

    let mut db = ComponentDB {
        id: ComponentId::without_provenance(0),
        type_id,
        layout: Layout::new::<C>(),
        dropper: Dropper::of::<C>(),
        cloner: C::CLONER,
        required: C::REQUIRED,
        summary_tick: C::SUMMARY_TICK,
        no_entity: C::NO_ENTITY,
        serialize: into_template.is_some(),
        type_db,
        reflect: None,
        into_template,
        on_add: C::ON_ADD,
        on_clone: C::ON_CLONE,
        on_insert: C::ON_INSERT,
        on_remove: C::ON_REMOVE,
        on_discard: C::ON_DISCARD,
        on_despawn: C::ON_DESPAWN,
        type_path: "",
        type_name: "",
        module_path: "",
    };

    let mut type_guard = TYPE_REGISTRY
        .write()
        .unwrap_or_else(PoisonError::into_inner);

    if let Some(&existing) = type_guard.get(type_id) {
        return existing;
    }

    let mut ident_guard = ID_REGISTRY.write().unwrap_or_else(PoisonError::into_inner);

    db.id = ComponentId::without_provenance(ident_guard.len());
    if let Some(type_db) = type_db {
        db.type_path = type_db.type_path();
        db.type_name = type_db.type_info().type_name();
        db.module_path = type_db.type_info().module_path().unwrap_or("");
    } else {
        let paths = crate::utils::split_path(::core::any::type_name::<C>());
        db.type_path = paths.0;
        db.type_name = paths.1;
        db.module_path = paths.2;
    }
    if let Some(reflect) = reflect {
        db.reflect = Some(Global::alloc_static(reflect));
    }

    let db: &'static ComponentDB = Global::alloc_static(db);
    type_guard.insert(type_id, db);
    ident_guard.push(db);

    ::core::mem::drop(ident_guard);
    ::core::mem::drop(type_guard);

    if let Some(type_db) = type_db {
        PATH_REGISTRY
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(type_db.type_path(), db);
    }

    db
}

// -----------------------------------------------------------------------------
