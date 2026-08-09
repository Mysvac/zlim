use core::alloc::Layout;
use core::any::TypeId;
use core::ptr::NonNull;
use std::sync::{PoisonError, RwLock};

use erased_serde::Deserializer as ErasedDeserializer;
use erased_serde::Serialize as ErasedSerialize;
use serde::{Deserialize, Serialize};
use zlim_ptr::{OwningPtr, Ptr, PtrMut};
use zlim_reflect::{Reflect, TypePath};
use zlim_utils::ext::TypeMap;
use zlim_utils::hash::HashMap;
use zlim_utils::mem::{Bump, Global};

use crate::clone::ComponentCloner;
use crate::entity::{EntityId, EntityMapper};
use crate::utils::{DebugLocation, Dropper};
use crate::world::DeferredWorld;

// -----------------------------------------------------------------------------
// ComponentId
// -----------------------------------------------------------------------------

crate::utils::define_ident!(
    /// A unique identifier for a `Component` type.
    ComponentId
);

// -----------------------------------------------------------------------------
// ComponentId
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct HookContext {
    pub id: ComponentId,
    pub entity: EntityId,
    pub caller: DebugLocation,
}

pub type ComponentHook = fn(DeferredWorld, HookContext);

// -----------------------------------------------------------------------------
// Component
// -----------------------------------------------------------------------------

#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a `Component`",
    label = "invalid `Component`",
    note = "consider annotating `{Self}` with `#[derive(Component)]`"
)]
pub trait Component: TypePath + Send + Sync + Sized + Serialize + for<'d> Deserialize<'d> {
    const NO_ENTITY: bool = false;

    const DROPPER: Option<Dropper> = Dropper::of::<Self>();

    const ON_ADD: Option<ComponentHook> = None;
    const ON_CLONE: Option<ComponentHook> = None;
    const ON_INSERT: Option<ComponentHook> = None;
    const ON_REMOVE: Option<ComponentHook> = None;
    const ON_DISCARD: Option<ComponentHook> = None;
    const ON_DESPAWN: Option<ComponentHook> = None;

    const CLONER: ComponentCloner;

    const FIELDS: &'static [&'static str] = &[];
    const MUTABLE_FIELDS: &'static [&'static str] = &[];
    const READONLY_FIELDS: &'static [&'static str] = &[];

    fn field(&self, name: &str) -> Option<&dyn Reflect>;

    fn field_mut(&mut self, name: &str) -> Option<&mut dyn Reflect>;

    #[inline(always)]
    fn map_entities<M: EntityMapper>(&mut self, _: &mut M) {}
}

// -----------------------------------------------------------------------------
// alias
// -----------------------------------------------------------------------------

pub mod alias {
    use crate::entity::EntityMapper;
    use core::ptr::NonNull;
    use erased_serde::{Deserializer, Error, Serialize};
    use zlim_ptr::{OwningPtr, Ptr, PtrMut};
    use zlim_reflect::Reflect;
    use zlim_utils::mem::Bump;

    pub type FieldRefFunc = for<'a> unsafe fn(Ptr<'a>, &str) -> Option<&'a dyn Reflect>;
    pub type FieldMutFunc = for<'a> unsafe fn(PtrMut<'a>, &str) -> Option<&'a mut dyn Reflect>;

    pub type WritterFunc = unsafe fn(NonNull<u8>, NonNull<u8>);
    pub type MapEntitiesFunc = unsafe fn(PtrMut<'_>, &mut dyn EntityMapper);

    pub type SeriailizeFunc = for<'a> fn(Ptr<'a>) -> &'a dyn Serialize;
    pub type DeserializeFunc =
        for<'a, 'b> fn(&'a mut dyn Deserializer, &'b Bump) -> Result<OwningPtr<'b>, Error>;
}

use alias::*;

// -----------------------------------------------------------------------------
// ComponentDB
// -----------------------------------------------------------------------------

pub struct ComponentDB {
    // --------------------------------
    // ident
    pub id: ComponentId,
    pub type_id: TypeId,

    pub typa_path: &'static str,
    pub typa_name: &'static str,
    pub module_path: &'static str,
    // --------------------------------
    // hook
    pub on_add: Option<ComponentHook>,
    pub on_clone: Option<ComponentHook>,
    pub on_insert: Option<ComponentHook>,
    pub on_remove: Option<ComponentHook>,
    pub on_discard: Option<ComponentHook>,
    pub on_despawn: Option<ComponentHook>,
    // --------------------------------
    // editor accessor
    pub fields: &'static [&'static str],
    pub mutable_fields: &'static [&'static str],
    pub readonly_fields: &'static [&'static str],
    pub field_ref_func: FieldRefFunc,
    pub field_mut_func: FieldMutFunc,
    // --------------------------------
    // Data
    pub layout: Layout,
    pub cloner: ComponentCloner,
    pub dropper: Option<Dropper>,
    pub map_entities: MapEntitiesFunc,
    // --------------------------------
    // serialize
    pub serialize: SeriailizeFunc,
    pub deserialize: DeserializeFunc,
}

// -----------------------------------------------------------------------------
// Register
// -----------------------------------------------------------------------------

static ID_REGISTRY: RwLock<Vec<&'static ComponentDB>> = RwLock::new(Vec::new());
static TYPE_REGISTRY: RwLock<TypeMap<&'static ComponentDB>> = RwLock::new(TypeMap::new());
static PATH_REGISTRY: RwLock<HashMap<&'static str, &'static ComponentDB>> =
    RwLock::new(HashMap::new());

impl ComponentDB {
    /// Returns the [`ComponentDB`] for `T`, registering it if this is
    /// the first access.
    #[inline(always)]
    pub fn of<T: Component>() -> &'static ComponentDB {
        if let Some(db) = ComponentDB::get_by_type(TypeId::of::<T>()) {
            return db;
        }

        ComponentDB::register::<T>()
    }

    pub fn get_by_type(id: TypeId) -> Option<&'static ComponentDB> {
        TYPE_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .copied()
    }

    pub fn get_by_path(path: &str) -> Option<&'static ComponentDB> {
        PATH_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(path)
            .copied()
    }

    #[cold]
    #[inline(never)]
    pub fn register<C: Component>() -> &'static ComponentDB {
        let type_id = TypeId::of::<C>();

        // Quick read-check first — hot path when already registered.
        let type_guard = TYPE_REGISTRY.read().unwrap_or_else(PoisonError::into_inner);
        if let Some(existing) = type_guard.get(type_id) {
            return existing;
        }
        ::core::mem::drop(type_guard);

        ::core::hint::cold_path();

        let mut db = ComponentDB {
            id: ComponentId::without_provenance(0),
            type_id: TypeId::of::<C>(),
            typa_path: C::type_path(),
            typa_name: C::type_name(),
            module_path: C::MODULE.unwrap_or(""),
            on_add: C::ON_ADD,
            on_clone: C::ON_CLONE,
            on_insert: C::ON_INSERT,
            on_remove: C::ON_REMOVE,
            on_discard: C::ON_DISCARD,
            on_despawn: C::ON_DESPAWN,
            fields: C::FIELDS,
            mutable_fields: C::MUTABLE_FIELDS,
            readonly_fields: C::READONLY_FIELDS,
            field_ref_func: field_ref::<C>,
            field_mut_func: field_mut::<C>,
            layout: Layout::new::<C>(),
            dropper: C::DROPPER,
            cloner: C::CLONER,
            map_entities: map_entities_fn::<C>,
            serialize: serialize_fn::<C>,
            deserialize: deserialize_fn::<C>,
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
            .insert(db.typa_path, db);

        db
    }
}

// -----------------------------------------------------------------------------
// Collect
// -----------------------------------------------------------------------------

impl ComponentDB {
    pub fn collect() {
        #[cold]
        #[inline(never)]
        fn collect_internal() {
            use __internal__::__ComponentReg__ as Reg;
            use zlim_os::time::Instant;
            const PRE: usize = 100;

            let start = Instant::now();
            log::info!("Collecting ComponentDB registrations...");

            {
                // pre-reserve, for better register speed.
                ID_REGISTRY
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .reserve(PRE);
                TYPE_REGISTRY
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .reserve(PRE);
                PATH_REGISTRY
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .reserve(PRE);
            }

            zlim_reg::iter::<Reg>().for_each(|r| {
                (r.0)();
            });

            {
                // post-reserve, for better hash performance.
                let len: usize = TYPE_REGISTRY
                    .read()
                    .unwrap_or_else(PoisonError::into_inner)
                    .len();
                let add: usize = len >> 1;
                TYPE_REGISTRY
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .reserve(add);
                PATH_REGISTRY
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .reserve(add);
            }

            log::info!("ComponentDB collection finished in {:?}", start.elapsed());
        }

        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(collect_internal);
    }
}

#[doc(hidden)]
pub mod __internal__ {
    pub use zlim_reg::submit;

    use super::{Component, ComponentDB};

    #[repr(transparent)]
    pub struct __ComponentReg__(pub(super) fn() -> &'static ComponentDB);

    impl __ComponentReg__ {
        /// Creates a registration token for type `T`.
        #[inline(always)]
        pub const fn of<C: Component>() -> Self {
            Self(ComponentDB::register::<C>)
        }
    }

    zlim_reg::collect!(__ComponentReg__);
}

#[macro_export]
macro_rules! register_component {
    ($($ty:ty),* $(,)?) => {
        const _: () = {
            $(
                $crate::component::__internal__::submit!(
                    $crate::component::__internal__::__ComponentReg__::of::<$ty>()
                    => $crate::component::__internal__::__ComponentReg__
                );
            )*
        };
    };
}

// -----------------------------------------------------------------------------
// Components
// -----------------------------------------------------------------------------

pub struct Components {
    pub dbs: Vec<&'static ComponentDB>,
    pub type_map: TypeMap<&'static ComponentDB>,
    pub path_map: HashMap<&'static str, &'static ComponentDB>,
}

impl Default for Components {
    fn default() -> Self {
        let dbs: Vec<&'static ComponentDB> = ID_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();

        let hint = dbs.len() + (dbs.len() >> 1);
        let mut type_map = TypeMap::with_capacity(hint);
        let mut path_map = HashMap::with_capacity(hint);

        for &item in &dbs {
            type_map.insert(item.type_id, item);
            path_map.insert(item.typa_path, item);
        }

        Self {
            dbs,
            type_map,
            path_map,
        }
    }
}

impl Components {
    pub fn update(&mut self) {
        let r = ID_REGISTRY.read().unwrap_or_else(PoisonError::into_inner);
        let data = r.as_slice();
        let old_len = data.len();
        if old_len < self.dbs.len() {
            self.dbs.extend_from_slice(&data[old_len..]);
        }

        ::core::mem::drop(r);
        if old_len < self.dbs.len() {
            for &item in &self.dbs[old_len..] {
                self.type_map.insert(item.type_id, item);
                self.path_map.insert(item.typa_path, item);
            }
        }
    }

    pub fn get<C: Component>(&self) -> &'static ComponentDB {
        if let Some(&r) = self.type_map.get(TypeId::of::<C>()) {
            return r;
        }
        ::core::hint::cold_path();
        ComponentDB::register::<C>()
    }

    #[inline]
    pub fn get_by_id(&self, id: ComponentId) -> Option<&'static ComponentDB> {
        if let Some(info) = self.dbs.get(id.index()) {
            return Some(*info);
        }

        #[cold]
        #[inline(never)]
        fn slow_path(id: ComponentId) -> Option<&'static ComponentDB> {
            ID_REGISTRY
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .get(id.index())
                .copied()
        }

        slow_path(id)
    }

    #[inline]
    pub fn get_by_path(&self, path: &str) -> Option<&'static ComponentDB> {
        if let Some(info) = self.path_map.get(path) {
            return Some(*info);
        }

        #[cold]
        #[inline(never)]
        fn slow_path(path: &str) -> Option<&'static ComponentDB> {
            PATH_REGISTRY
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .get(path)
                .copied()
        }

        slow_path(path)
    }

    #[inline]
    pub fn get_by_type(&self, ty: TypeId) -> Option<&'static ComponentDB> {
        if let Some(info) = self.type_map.get(ty) {
            return Some(*info);
        }

        #[cold]
        #[inline(never)]
        fn slow_path(ty: TypeId) -> Option<&'static ComponentDB> {
            TYPE_REGISTRY
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .get(ty)
                .copied()
        }

        slow_path(ty)
    }
}

// -----------------------------------------------------------------------------
// Free helper functions for type-erased function pointers
// -----------------------------------------------------------------------------

/// Type-erased field accessor for `C::field`.
fn field_ref<'a, C: Component>(ptr: Ptr<'a>, name: &str) -> Option<&'a dyn Reflect> {
    ptr.debug_assert_aligned::<C>();
    unsafe { ptr.deref::<C>().field(name) }
}

/// Type-erased mutable field accessor for `C::field_mut`.
fn field_mut<'a, C: Component>(ptr: PtrMut<'a>, name: &str) -> Option<&'a mut dyn Reflect> {
    ptr.debug_assert_aligned::<C>();
    unsafe { ptr.deref::<C>().field_mut(name) }
}

/// Type-erased field accessor for `C::field`.
fn map_entities_fn<'a, C: Component>(ptr: PtrMut<'a>, mut mapper: &mut dyn EntityMapper) {
    ptr.debug_assert_aligned::<C>();
    unsafe {
        ptr.deref::<C>()
            .map_entities::<&mut dyn EntityMapper>(&mut mapper);
    }
}

/// Type-erased serializer for `C`.
fn serialize_fn<C: Component>(ptr: Ptr<'_>) -> &dyn ErasedSerialize {
    ptr.debug_assert_aligned::<C>();
    unsafe { ptr.deref::<C>() as &dyn ErasedSerialize }
}

/// Type-erased deserializer for `C`.
fn deserialize_fn<'b, C: Component>(
    deserializer: &mut dyn ErasedDeserializer<'_>,
    bump: &'b Bump,
) -> Result<OwningPtr<'b>, erased_serde::Error> {
    let value: C = erased_serde::deserialize(deserializer)?;
    let ptr = unsafe { bump.alloc_unchecked(value) };
    let inner = NonNull::from_mut(ptr).cast::<u8>();
    Ok(unsafe { OwningPtr::new(inner) })
}
