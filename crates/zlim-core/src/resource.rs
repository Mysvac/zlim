use core::alloc::Layout;
use core::any::TypeId;
use std::sync::{PoisonError, RwLock};

use zlim_ptr::{Ptr, PtrMut};
use zlim_reflect::{Reflect, TypePath};
use zlim_utils::ext::TypeMap;
use zlim_utils::hash::HashMap;
use zlim_utils::mem::Global;

use crate::utils::Dropper;

// -----------------------------------------------------------------------------
// ResourceId
// -----------------------------------------------------------------------------

crate::utils::define_ident!(
    /// A unique identifier for a `Resource` type.
    ResourceId
);

// -----------------------------------------------------------------------------
// Resource
// -----------------------------------------------------------------------------

#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a `Resource`",
    label = "invalid `Resource`",
    note = "consider annotating `{Self}` with `#[derive(Resource)]`"
)]
pub trait Resource: TypePath + Sized {
    const DROPPER: Option<Dropper> = Dropper::of::<Self>();

    const FIELDS: &'static [&'static str] = &[];
    const MUTABLE_FIELDS: &'static [&'static str] = &[];
    const READONLY_FIELDS: &'static [&'static str] = &[];

    fn field(&self, name: &str) -> Option<&dyn Reflect>;

    fn field_mut(&mut self, name: &str) -> Option<&mut dyn Reflect>;
}

// -----------------------------------------------------------------------------
// alias
// -----------------------------------------------------------------------------

pub mod alias {
    use zlim_ptr::{Ptr, PtrMut};
    use zlim_reflect::Reflect;

    pub type FieldRefFunc = for<'a> unsafe fn(Ptr<'a>, &str) -> Option<&'a dyn Reflect>;
    pub type FieldMutFunc = for<'a> unsafe fn(PtrMut<'a>, &str) -> Option<&'a mut dyn Reflect>;
}

use alias::*;

// -----------------------------------------------------------------------------
// ResourceDB
// -----------------------------------------------------------------------------

pub struct ResourceDB {
    // --------------------------------
    // ident
    pub id: ResourceId,
    pub type_id: TypeId,

    pub typa_path: &'static str,
    pub typa_name: &'static str,
    pub module_path: &'static str,
    // --------------------------------
    // editor accessor
    pub fields: &'static [&'static str],
    pub mutable_fields: &'static [&'static str],
    pub readonly_fields: &'static [&'static str],
    pub field_ref_func: FieldRefFunc,
    pub field_mut_func: FieldMutFunc,
    // --------------------------------
    // dropper
    pub layout: Layout,
    pub dropper: Option<Dropper>,
}

// -----------------------------------------------------------------------------
// Register
// -----------------------------------------------------------------------------

static ID_REGISTRY: RwLock<Vec<&'static ResourceDB>> = RwLock::new(Vec::new());
static TYPE_REGISTRY: RwLock<TypeMap<&'static ResourceDB>> = RwLock::new(TypeMap::new());
static PATH_REGISTRY: RwLock<HashMap<&'static str, &'static ResourceDB>> =
    RwLock::new(HashMap::new());

impl ResourceDB {
    #[inline(always)]
    pub fn of<T: Resource>() -> &'static ResourceDB {
        if let Some(db) = ResourceDB::get_by_type(TypeId::of::<T>()) {
            return db;
        }

        ResourceDB::register::<T>()
    }

    pub fn get_by_type(id: TypeId) -> Option<&'static ResourceDB> {
        TYPE_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .copied()
    }

    pub fn get_by_path(path: &str) -> Option<&'static ResourceDB> {
        PATH_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(path)
            .copied()
    }

    #[cold]
    #[inline(never)]
    pub fn register<R: Resource>() -> &'static ResourceDB {
        let type_id = TypeId::of::<R>();

        // Quick read-check first — hot path when already registered.
        let type_guard = TYPE_REGISTRY.read().unwrap_or_else(PoisonError::into_inner);
        if let Some(existing) = type_guard.get(type_id) {
            return existing;
        }
        ::core::mem::drop(type_guard);

        ::core::hint::cold_path();

        let mut db = ResourceDB {
            id: ResourceId::without_provenance(0),
            type_id: TypeId::of::<R>(),
            typa_path: R::type_path(),
            typa_name: R::type_name(),
            module_path: R::MODULE.unwrap_or(""),
            fields: R::FIELDS,
            mutable_fields: R::MUTABLE_FIELDS,
            readonly_fields: R::READONLY_FIELDS,
            field_ref_func: field_ref::<R>,
            field_mut_func: field_mut::<R>,
            layout: Layout::new::<R>(),
            dropper: R::DROPPER,
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
            .insert(db.typa_path, db);

        db
    }
}

// -----------------------------------------------------------------------------
// Collect
// -----------------------------------------------------------------------------

impl ResourceDB {
    pub fn collect() {
        #[cold]
        #[inline(never)]
        fn collect_internal() {
            use __internal__::__ResourceReg__ as Reg;
            use zlim_os::time::Instant;
            const PRE: usize = 100;

            let start = Instant::now();
            log::info!("Collecting ResourceDB registrations...");

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

            log::info!("ResourceDB collection finished in {:?}", start.elapsed());
        }

        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(collect_internal);
    }
}

#[doc(hidden)]
pub mod __internal__ {
    pub use zlim_reg::submit;

    use super::{Resource, ResourceDB};

    #[repr(transparent)]
    pub struct __ResourceReg__(pub(super) fn() -> &'static ResourceDB);

    impl __ResourceReg__ {
        /// Creates a registration token for type `T`.
        #[inline(always)]
        pub const fn of<R: Resource>() -> Self {
            Self(ResourceDB::register::<R>)
        }
    }

    zlim_reg::collect!(__ResourceReg__);
}

#[macro_export]
macro_rules! register_resource {
    ($($ty:ty),* $(,)?) => {
        const _: () = {
            $(
                $crate::resource::__internal__::submit!(
                    $crate::resource::__internal__::__ResourceReg__::of::<$ty>()
                    => $crate::resource::__internal__::__ResourceReg__
                );
            )*
        };
    };
}

// -----------------------------------------------------------------------------
// Resources
// -----------------------------------------------------------------------------

pub struct Resources {
    pub dbs: Vec<&'static ResourceDB>,
    pub type_map: TypeMap<&'static ResourceDB>,
    pub path_map: HashMap<&'static str, &'static ResourceDB>,
}

impl Default for Resources {
    fn default() -> Self {
        let dbs: Vec<&'static ResourceDB> = ID_REGISTRY
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

impl Resources {
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

    pub fn get<R: Resource>(&self) -> &'static ResourceDB {
        if let Some(&r) = self.type_map.get(TypeId::of::<R>()) {
            return r;
        }
        ::core::hint::cold_path();
        ResourceDB::register::<R>()
    }

    pub fn get_by_id(&self, id: ResourceId) -> Option<&'static ResourceDB> {
        if let Some(info) = self.dbs.get(id.index()) {
            return Some(*info);
        }

        #[cold]
        #[inline(never)]
        fn slow_path(id: ResourceId) -> Option<&'static ResourceDB> {
            ID_REGISTRY
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .get(id.index())
                .copied()
        }

        slow_path(id)
    }

    pub fn get_by_path(&self, path: &str) -> Option<&'static ResourceDB> {
        if let Some(info) = self.path_map.get(path) {
            return Some(*info);
        }

        #[cold]
        #[inline(never)]
        fn slow_path(path: &str) -> Option<&'static ResourceDB> {
            PATH_REGISTRY
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .get(path)
                .copied()
        }

        slow_path(path)
    }

    pub fn get_by_type(&self, ty: TypeId) -> Option<&'static ResourceDB> {
        if let Some(info) = self.type_map.get(ty) {
            return Some(*info);
        }

        #[cold]
        #[inline(never)]
        fn slow_path(ty: TypeId) -> Option<&'static ResourceDB> {
            TYPE_REGISTRY
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .get(ty)
                .copied()
        }

        slow_path(ty)
    }

    /// Registers the resource type `R` and returns its [`ResourceId`].
    ///
    /// This is idempotent: if the type is already registered, the existing
    /// ID is returned without creating a duplicate.
    #[inline]
    pub fn register<R: Resource>(&mut self) -> ResourceId {
        let db = self.get::<R>();
        db.id
    }

    /// Looks up a [`ResourceId`] by [`TypeId`].
    ///
    /// Returns `None` if no resource with the given `TypeId` has been
    /// registered.
    #[inline]
    pub fn get_id(&self, type_id: TypeId) -> Option<ResourceId> {
        self.get_by_type(type_id).map(|db| db.id)
    }
}

// -----------------------------------------------------------------------------
// Free helper functions for type-erased function pointers
// -----------------------------------------------------------------------------

/// Type-erased field accessor for `R::field`.
fn field_ref<'a, R: Resource>(ptr: Ptr<'a>, name: &str) -> Option<&'a dyn Reflect> {
    ptr.debug_assert_aligned::<R>();
    unsafe { ptr.deref::<R>().field(name) }
}

/// Type-erased mutable field accessor for `R::field_mut`.
fn field_mut<'a, R: Resource>(ptr: PtrMut<'a>, name: &str) -> Option<&'a mut dyn Reflect> {
    ptr.debug_assert_aligned::<R>();
    unsafe { ptr.deref::<R>().field_mut(name) }
}
