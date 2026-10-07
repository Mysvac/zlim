//! Resource registration entry points.

use core::alloc::Layout;
use core::any::TypeId;
use std::sync::PoisonError;

use zlim_reflect::TypeDB;
use zlim_reflect::db::TypeDatabase;
use zlim_utils::mem::Global;

use crate::utils::Dropper;

use super::db::{ID_REGISTRY, PATH_REGISTRY, ResourceDB, TYPE_REGISTRY};
use super::id::ResourceId;
use super::reflect::ReflectResource;
use super::resource::Resource;

// -----------------------------------------------------------------------------
// Registration
// -----------------------------------------------------------------------------

/// Quick read-check — the hot path once the type is registered.
#[inline(never)]
fn fast_path(id: TypeId) -> Option<&'static ResourceDB> {
    TYPE_REGISTRY
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .get(id)
        .copied()
}

/// Registers a [`Resource`] type `R` **without** reflection support.
///
/// This is the registration behind [`Resource::REGISTER`] and the default for
/// `#[derive(Resource)]`.
///
/// Registration is idempotent: if `R` is already registered, the existing
/// entry is returned without creating a duplicate.
///
/// Without reflection there is no [`TypeDB`] to take the names from, so
/// `type_path` / `type_name` / `module_path` are parsed out of [`core::any::type_name`].
/// That name is not stable, which is why a resource registered this way is kept
/// out of the path registry and cannot be found by [`ResourceDB::get_by_path`].
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::resource::register_base;
///
/// #[derive(Resource)]
/// struct Score(u32);
///
/// // Usually reached through `ResourceDB::of` / `Resource::REGISTER`
/// // instead of being called directly:
/// let db = register_base::<Score>();
/// assert_eq!(db.type_name, "Score");
/// ```
///
/// [`Resource`]: crate::resource::Resource
/// [`ResourceDB`]: crate::resource::ResourceDB
/// [`ResourceDB::get_by_path`]: crate::resource::ResourceDB::get_by_path
/// [`Resource::REGISTER`]: crate::resource::Resource::REGISTER
#[inline]
pub fn register_base<R: Resource>() -> &'static ResourceDB {
    if let Some(db) = fast_path(TypeId::of::<R>()) {
        return db;
    }

    register_impl::<R>(None, None)
}

/// Registers a [`Resource`] type `R` **with** reflection support.
///
/// Registration is idempotent: if `R` is already registered, the existing
/// entry is returned without creating a duplicate.
///
/// In addition to the base metadata this caches the type's [`TypeDB`] and the
/// [`ReflectResource`] function pointers, which is what lets the resource be
/// read, written and removed through `&dyn Reflect`.
///
/// The names then come from the [`TypeDB`] instead of [`core::any::type_name`],
/// so the resource is also filed in the path registry and can be found by
/// [`ResourceDB::get_by_path`].
#[inline]
pub fn register_reflect<R: Resource + TypeDatabase>() -> &'static ResourceDB {
    if let Some(db) = fast_path(TypeId::of::<R>()) {
        return db;
    }
    let type_db = Some(TypeDB::of::<R>());
    let reflect = Some(ReflectResource::new::<R>());
    register_impl::<R>(type_db, reflect)
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
fn register_impl<R: Resource>(
    type_db: Option<&'static TypeDB>,
    reflect: Option<ReflectResource>,
) -> &'static ResourceDB {
    debug_assert_eq!(type_db.is_some(), reflect.is_some());

    let type_id = TypeId::of::<R>();

    ::core::hint::cold_path();

    let mut db = ResourceDB {
        id: ResourceId::without_provenance(0),
        type_id,
        layout: Layout::new::<R>(),
        dropper: Dropper::of::<R>(),
        type_db,
        reflect: None,
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

    db.id = ResourceId::without_provenance(ident_guard.len());
    if let Some(type_db) = type_db {
        db.type_path = type_db.type_path();
        db.type_name = type_db.type_info().type_name();
        db.module_path = type_db.type_info().module_path().unwrap_or("");
    } else {
        let paths = crate::utils::split_path(::core::any::type_name::<R>());
        db.type_path = paths.0;
        db.type_name = paths.1;
        db.module_path = paths.2;
    }
    if let Some(reflect) = reflect {
        db.reflect = Some(Global::alloc_static(reflect));
    }

    let db: &'static ResourceDB = Global::alloc_static(db);
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
