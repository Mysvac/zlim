//! Resource Core Implementation

use core::alloc::Layout;
use core::any::TypeId;
use core::fmt::{Debug, Formatter};
use std::sync::{PoisonError, RwLock};

use zlim_ptr::{Ptr, PtrMut};
use zlim_reflect::{Reflect, TypePath};
use zlim_utils::ext::TypeMap;
use zlim_utils::hash::HashMap;
use zlim_utils::mem::Global;

use crate::utils::Dropper;

pub use zlim_core_derive::Resource;

// -----------------------------------------------------------------------------
// ResourceId
// -----------------------------------------------------------------------------

crate::utils::define_ident!(
    /// A unique identifier for a `Resource` type.
    ///
    /// This ID is shared by all worlds.
    ResourceId
);

// -----------------------------------------------------------------------------
// Resource
// -----------------------------------------------------------------------------

/// A type that can be stored as a global resource in the ECS `World`.
///
/// A resource is a singleton value identified by its concrete Rust type.
/// At most one value of a given resource type can exist in a [`World`].
/// Thread-safety determines which access APIs are available:
///
/// - `Send + Sync` resources can be accessed through [`Res`] and [`ResMut`].
///
/// - `!Sync` resources must stay on the main thread and are accessed through
///   [`NonSend`], and [`NonSendMut`].
///
/// # Derive Macro
///
/// For most resource types, prefer using the [Resource derive macro].
///
/// ```ignore
/// // Basic usage
/// #[derive(TypePath, Resource)]
/// struct Foo;
///
/// // Expose field to editor
/// #[derive(TypePath, Resource)]
/// struct Logger {
///     #[editor(readonly)]
///     level: String,
///     #[editor(mutable)]
///     filter: String,
///     // other private fields:
///     foo: u8,
///     bar: String,
/// }
/// ```
///
/// See [Resource derive macro] documentation for details.
///
/// # Safety
///
/// Implementing this trait promises that the type can be stored behind the
/// ECS' type-erased resource storage. If you override [`Self::DROPPER`],
/// they must match the implementor's actual layout and drop behavior.
///
/// [`World`]: crate::world::World
/// [`Res`]: crate::borrow::Res
/// [`ResMut`]: crate::borrow::ResMut
/// [`NonSend`]: crate::borrow::NonSend
/// [`NonSendMut`]: crate::borrow::NonSendMut
/// [`field`]: Resource::field
/// [`field_mut`]: Resource::field_mut
/// [Resource derive macro]: crate::derive::Resource
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a `Resource`",
    label = "invalid `Resource`",
    note = "consider annotating `{Self}` with `#[derive(Resource)]`"
)]
pub trait Resource: TypePath + Sized {
    /// The dropper function for this type, if it is not trivially droppable.
    ///
    /// Set to `Some(...)` when the type [`needs_drop`].
    ///
    /// [`needs_drop`]: core::mem::needs_drop
    const DROPPER: Option<Dropper> = Dropper::of::<Self>();

    /// The set of all field names exposed for reflection, in declaration order.
    const FIELDS: &'static [&'static str] = &[];

    /// The subset of [`FIELDS`](Resource::FIELDS) that can be written to.
    const MUTABLE_FIELDS: &'static [&'static str] = &[];

    /// The subset of [`FIELDS`](Resource::FIELDS) that are read-only.
    const READONLY_FIELDS: &'static [&'static str] = &[];

    /// Returns a reflected reference to the named field.
    ///
    /// Returns `None` if `name` does not match any field of this type.
    fn field(&self, name: &str) -> Option<&dyn Reflect>;

    /// Returns a reflected mutable reference to the named field.
    ///
    /// Returns `None` if `name` does not match any mutable field of this type.
    fn field_mut(&mut self, name: &str) -> Option<&mut dyn Reflect>;
}

// -----------------------------------------------------------------------------
// alias
// -----------------------------------------------------------------------------

/// Type aliases shared across the resource subsystem.
pub mod alias {
    use zlim_ptr::{Ptr, PtrMut};
    use zlim_reflect::Reflect;

    /// Type-erased function pointer for reading a reflected field.
    ///
    /// Given a type-erased [`Ptr`] and a field name, returns a reflected reference
    /// to that field if it exists. Used internally by the editor and serialization
    /// systems to inspect resource data without knowing the concrete type.
    pub type FieldRefFunc = for<'a> unsafe fn(Ptr<'a>, &str) -> Option<&'a dyn Reflect>;

    /// Type-erased function pointer for mutably accessing a reflected field.
    ///
    /// Given a type-erased [`PtrMut`] and a field name, returns a mutable reflected
    /// reference to that field if it exists and is writable.
    pub type FieldMutFunc = for<'a> unsafe fn(PtrMut<'a>, &str) -> Option<&'a mut dyn Reflect>;
}

use alias::*;

// -----------------------------------------------------------------------------
// ResourceDB
// -----------------------------------------------------------------------------

/// Static per-type metadata for a registered [`Resource`].
///
/// Each resource type has exactly one `ResourceDB` instance, allocated as a
/// `&'static` reference during registration. It holds the type's identity
/// (id, type path), reflection metadata (field names and accessors), and
/// memory layout information needed for allocation and destruction.
///
/// `ResourceDB` instances are stored in per-world registries for O(1)
/// lookup by id, type, or path.
pub struct ResourceDB {
    // --------------------------------
    // Ident
    /// Unique numeric identifier for this resource type.
    pub id: ResourceId,
    /// The [`TypeId`] of the resource type.
    pub type_id: TypeId,

    /// The full type path string (e.g., `"my_crate::MyResource"`).
    pub typa_path: &'static str,
    /// The short type name string (e.g., `"MyResource"`).
    pub typa_name: &'static str,
    /// The module path where the type is defined.
    pub module_path: &'static str,

    // --------------------------------
    // Editor accessor
    /// All field names in declaration order.
    pub fields: &'static [&'static str],
    /// Field names that accept writes.
    pub mutable_fields: &'static [&'static str],
    /// Field names that are read-only.
    pub readonly_fields: &'static [&'static str],
    /// Type-erased function to read a field by name.
    pub field_ref_func: FieldRefFunc,
    /// Type-erased function to mutably access a field by name.
    pub field_mut_func: FieldMutFunc,

    // --------------------------------
    // Memory Layout
    /// Memory layout of the resource type (size + alignment).
    pub layout: Layout,
    /// Optional dropper function for explicit cleanup.
    pub dropper: Option<Dropper>,
}

impl Debug for ResourceDB {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_map()
            .entry(&"id", &self.id)
            .entry(&"type_id", &self.type_id)
            .entry(&"type_path", &self.typa_path)
            .finish()
    }
}

// -----------------------------------------------------------------------------
// Register
// -----------------------------------------------------------------------------

static ID_REGISTRY: RwLock<Vec<&'static ResourceDB>> = RwLock::new(Vec::new());
static TYPE_REGISTRY: RwLock<TypeMap<&'static ResourceDB>> = RwLock::new(TypeMap::new());
static PATH_REGISTRY: RwLock<HashMap<&'static str, &'static ResourceDB>> =
    RwLock::new(HashMap::new());

impl ResourceDB {
    /// Returns the [`ResourceDB`] metadata for type `T`, registering it if necessary.
    ///
    /// This is the primary entry point for obtaining resource metadata. It first
    /// checks the type registry for an existing entry; if none is found, it
    /// calls [`register`](ResourceDB::register) to create one.
    #[inline(always)]
    pub fn of<T: Resource>() -> &'static ResourceDB {
        if let Some(db) = ResourceDB::get_by_type(TypeId::of::<T>()) {
            return db;
        }

        ResourceDB::register::<T>()
    }

    /// Looks up [`ResourceDB`] metadata by [`TypeId`].
    ///
    /// Returns `None` if no resource of the given type has been registered.
    pub fn get_by_type(id: TypeId) -> Option<&'static ResourceDB> {
        TYPE_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .copied()
    }

    /// Looks up [`ResourceDB`] metadata by type path string.
    ///
    /// The path is the full type path as returned by [`TypePath::type_path`]
    /// (e.g., `"my_crate::MyResource"`). Returns `None` if no resource with
    /// the given path has been registered.
    pub fn get_by_path(path: &str) -> Option<&'static ResourceDB> {
        PATH_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(path)
            .copied()
    }

    /// Registers a [`Resource`] type `R` in the global registries, returning its
    /// `&'static` [`ResourceDB`].
    ///
    /// Registration is idempotent: if `R` is already registered, the existing
    /// entry is returned without creating a duplicate. This function is marked
    /// `#[cold]` because it should only execute once per type during the
    /// application lifetime.
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

/// Internal module, public for resource regisration.
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

/// Registers one or more [`Resource`] types for deferred collection.
///
/// This macro submits registration tokens that are later collected by
/// [`ResourceDB::collect`]. Use it at the crate root or in a module to ensure
/// resource types are discoverable by the engine at startup.
///
/// # Examples
///
/// ```ignore
/// register_resource!(MyResource, AnotherResource);
/// ```
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

impl ResourceDB {
    /// Runs all deferred resource registrations submitted via the
    /// [`register_resource!`] macro.
    ///
    /// Non generic types marked with [`Resource`] derive macro will be
    /// automatically registered.
    ///
    /// This is called once at engine startup to batch-collect registration
    /// tokens from across the crate graph. Pre-reserving registry capacity
    /// before iteration improves registration throughput. The function is
    /// guarded by a [`std::sync::Once`] so it is safe to call multiple times.
    ///
    /// [`Resource`]: crate::derive::Resource
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

// -----------------------------------------------------------------------------
// Resources
// -----------------------------------------------------------------------------

/// A local snapshot of the global resource registries for fast access
/// within a [`World`].
///
/// This snapshot is initialized from the static registries and can be
/// incrementally updated via [`Resources::update`] when new types are
/// registered after construction.
///
/// [`World`]: crate::world::World
pub struct Resources {
    /// All registered resource databases, indexed by [`ResourceId`].
    dbs: Vec<&'static ResourceDB>,
    /// O(1) lookup by [`TypeId`].
    type_map: TypeMap<&'static ResourceDB>,
    /// O(1) lookup by type path string.
    path_map: HashMap<&'static str, &'static ResourceDB>,
}

impl Debug for Resources {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&self.dbs, f)
    }
}

impl Default for Resources {
    fn default() -> Self {
        crate::cfg::debug! {
            #[cfg(not(test))]
            let start = ::std::time::Instant::now();
        }

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

        crate::cfg::debug! {
            #[cfg(not(test))]
            log::debug!("`zlim_core::Resources` initialized: {:?}`", start.elapsed());
        }

        Self {
            dbs,
            type_map,
            path_map,
        }
    }
}

impl Resources {
    /// Synchronizes this snapshot with the global registries.
    ///
    /// Any resource types that were registered after this `Resources` was
    /// created are appended to the internal vectors and maps. This is a
    /// cheap no-op if no new registrations have occurred.
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

    /// Return the number of registered resources.
    pub fn len(&self) -> usize {
        self.dbs.len()
    }

    /// Returns the [`ResourceDB`] metadata for type `R`, falling back to
    /// the global registries if not yet in this snapshot.
    ///
    /// If `R` has never been registered anywhere, this triggers a new
    /// registration.
    pub fn get<R: Resource>(&self) -> &'static ResourceDB {
        if let Some(&r) = self.type_map.get(TypeId::of::<R>()) {
            return r;
        }
        ::core::hint::cold_path();
        ResourceDB::register::<R>()
    }

    /// Looks up [`ResourceDB`] metadata by [`ResourceId`].
    ///
    /// Checks the local cache first; falls back to the global registry
    /// on a miss. Returns `None` if the id is out of bounds.
    /// out of bounds.
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

    /// Looks up [`ResourceDB`] metadata by type path string.
    ///
    /// Checks the local cache first; falls back to the global registry
    /// on a miss. Returns `None` if no resource with the given path has
    /// resource with the given path has been registered.
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

    /// Looks up [`ResourceDB`] metadata by [`TypeId`].
    ///
    /// Checks the local cache first; falls back to the global registry
    /// on a miss. Returns `None` if no resource with the given [`TypeId`] has
    /// resource with the given [`TypeId`] has been registered.
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

// -----------------------------------------------------------------------------
