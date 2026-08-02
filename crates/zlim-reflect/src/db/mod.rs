//! Type database and registration system.
//!
//! This module provides [`TypeDB`], a per-type `'static` registry that stores
//! compile-time metadata ([`TypeInfo`], [`TypePath`]), conversion functions,
//! constructors, serde hooks, and pointer reconstruction helpers.
//!
//! # Registration
//!
//! Types opt into the database by implementing [`TypeDatabase`]. The
//! [`register!`] macro submits registration closures to a platform
//! linker section (via [`zlim_reg`]), and [`TypeDB::collect`] invokes
//! them all at startup.
//!
//! # Menu
//!
//! - [`TypeDB`] — per-type metadata and runtime operations.
//! - [`TypeDatabase`] — trait for types registered in the database.
//! - [`register!`] — macro to register types for auto-discovery.
//! - Serialization / Deserialization — serde integration (see `ser` / `des` submodules).
//!
//! [`TypePath`]: crate::path::TypePath
//! [`register!`]: crate::register!

// ----------------------------------------------------------------------------
// Imports
// ----------------------------------------------------------------------------

use core::any::TypeId;
use std::sync::{OnceLock, PoisonError, RwLock};

use zlim_ptr::{Ptr, PtrMut};
use zlim_utils::ext::TypeMap;
use zlim_utils::hash::HashMap;
use zlim_utils::mem::Global;

use crate::info::{Attributes, Generics, TypeInfo, Typed};
use crate::ops::Reflect;

// ----------------------------------------------------------------------------
// TypeDatabase & TypeDB
// ----------------------------------------------------------------------------

/// Trait for types that can be registered in the [`TypeDB`] database.
///
/// Implementing this trait (together with [`Reflect`] and [`Typed`])
/// makes a type eligible for [`register!`](crate::register!) and auto-discovery via
/// [`TypeDB::collect`].
pub trait TypeDatabase: Reflect + Typed {
    /// Called after the type's [`TypeDB`] entry is created.
    #[expect(unused_variables, reason = "no-op implementation")]
    #[inline(always)]
    fn on_register(db: &'static TypeDB) {}

    /// Called after [`on_register`](Self::on_register).
    #[inline(always)]
    fn register_dependencies() {}
}

/// Per-type metadata and runtime operation registry.
///
/// Each registered type gets one `'static` [`TypeDB`] instance that stores
/// the type's identity, reflection metadata, conversion functions, optional
/// constructor, and function pointers for safe pointer reconstruction.
///
/// # Acquisition
///
/// - [`TypeDB::of::<T>()`] — get or register type `T`.
/// - `TypeDB::get_by_type` — look up an already-registered type by `TypeId`.
pub struct TypeDB {
    id: TypeId,
    type_path: &'static str,
    type_info: &'static TypeInfo,
    into_func: RwLock<TypeMap<IntoFunc>>,
    ctor_func: OnceLock<CtorFunc>,
    serialize: OnceLock<SerdFunc>,
    deserialize: OnceLock<DeseFunc>,
    from_reflect: FromFunc,
    ref_from_ptr: unsafe fn(Ptr<'_>) -> &'_ dyn Reflect,
    mut_from_ptr: unsafe fn(PtrMut<'_>) -> &'_ mut dyn Reflect,
}

static TYPE_REGISTRY: RwLock<TypeMap<&'static TypeDB>> = RwLock::new(TypeMap::new());
static PATH_REGISTRY: RwLock<HashMap<&'static str, &'static TypeDB>> = RwLock::new(HashMap::new());

type IntoFunc = &'static (dyn Fn(Box<dyn Reflect>) -> Box<dyn Reflect> + Sync + 'static);
type CtorFunc = &'static (dyn Fn() -> Box<dyn Reflect> + Sync + 'static);
type FromFunc = fn(Box<dyn Reflect>) -> Result<Box<dyn Reflect>, Box<dyn Reflect>>;

// Returns `&dyn Serialize` instead of taking `&mut dyn Serializer` because
// `erased_serde::Serialize` returns `Ok(S::Ok)`, which is compatible with serde's `Serialize`.
type SerdFunc = fn(&dyn Reflect) -> &dyn erased_serde::Serialize;
type DeseFunc =
    fn(&mut dyn erased_serde::Deserializer) -> Result<Box<dyn Reflect>, erased_serde::Error>;

// ----------------------------------------------------------------------------
// Register
// ----------------------------------------------------------------------------

impl TypeDB {
    /// Returns the [`TypeDB`] for `T`, registering it if this is the first
    /// access.
    ///
    /// This is the primary entry point for obtaining a type's database
    /// entry. The first call for a given type registers it; subsequent
    /// calls return the cached `'static` reference.
    #[inline(always)]
    pub fn of<T: TypeDatabase>() -> &'static TypeDB {
        if let Some(db) = TypeDB::get_by_type(TypeId::of::<T>()) {
            return db;
        }

        TypeDB::register::<T>()
    }

    /// Looks up an already-registered [`TypeDB`] by [`TypeId`].
    ///
    /// Returns `None` if the type has not been registered yet.
    #[inline(never)]
    pub fn get_by_type(id: TypeId) -> Option<&'static TypeDB> {
        TYPE_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .copied()
    }

    /// Looks up an already-registered [`TypeDB`] by type path string.
    ///
    /// Returns `None` if no type with the given path has been registered.
    #[inline(never)]
    pub fn get_by_path(path: &str) -> Option<&'static TypeDB> {
        PATH_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(path)
            .copied()
    }

    /// Registers `T` in the global type database.
    ///
    /// This is an idempotent, thread-safe operation — concurrent callers
    /// racing to register the same type will all receive the same
    /// `&'static TypeDB` reference.
    ///
    /// After registration, [`on_register`](TypeDatabase::on_register) and
    /// [`register_dependencies`](TypeDatabase::register_dependencies) are
    /// called on `T`.
    #[cold]
    #[inline(never)]
    pub fn register<T: TypeDatabase>() -> &'static TypeDB {
        use zlim_utils::hash::map::Entry;

        // Double-checked locking: if another thread registered between
        // the caller's check and acquiring the write lock, return the
        // existing entry.
        if let Some(db) = TypeDB::get_by_type(TypeId::of::<T>()) {
            return db;
        }
        ::core::hint::cold_path();

        fn from_reflect<T: Reflect>(
            x: Box<dyn Reflect>,
        ) -> Result<Box<dyn Reflect>, Box<dyn Reflect>> {
            T::from_reflect(x).map(|v| v as Box<dyn Reflect>)
        }

        let tdb = TypeDB {
            id: TypeId::of::<T>(),
            type_path: T::type_path(),
            type_info: T::type_info(),
            into_func: RwLock::new(TypeMap::new()),
            ctor_func: OnceLock::new(),
            serialize: OnceLock::new(),
            deserialize: OnceLock::new(),
            from_reflect: from_reflect::<T>,
            ref_from_ptr: T::reflect_from_ptr,
            mut_from_ptr: T::reflect_from_ptr_mut,
        };

        let db: &'static TypeDB = {
            let db = match TYPE_REGISTRY
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(TypeId::of::<T>())
            {
                Entry::Occupied(entry) => {
                    ::core::hint::cold_path();
                    return entry.get();
                }
                Entry::Vacant(entry) => {
                    // SAFETY: TypeDB is intentionally leaked — it serves as
                    // a global data store for the lifetime of the process.
                    // The `into_func` map internally requires `Drop`, but
                    // the OS reclaims all memory at process exit.
                    #[expect(unsafe_code, reason = "TypeDB is !Copy")]
                    let db: &'static TypeDB = unsafe { Global::alloc_unchecked(tdb) };
                    entry.insert(db);
                    db
                }
            };

            PATH_REGISTRY
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(db.type_path, db);

            db
        };

        T::on_register(db);
        T::register_dependencies();

        db
    }
}

impl dyn Reflect {
    /// Looks up the [`TypeDB`] entry for this value's concrete type.
    #[inline(always)]
    pub fn type_db(&self) -> Option<&'static TypeDB> {
        TypeDB::get_by_type(self.type_id())
    }
}

// ----------------------------------------------------------------------------
// Basic
// ----------------------------------------------------------------------------

impl TypeDB {
    /// Returns the [`TypeId`] of the registered type.
    #[inline(always)]
    pub fn id(&self) -> TypeId {
        self.id
    }

    /// Returns the [`TypeId`] of the registered type (alias for [`id`](Self::id)).
    #[inline(always)]
    pub fn type_id(&self) -> TypeId {
        self.id
    }

    /// Returns the fully-qualified type path (see `TypePath::type_path`).
    #[inline(always)]
    pub fn type_path(&self) -> &'static str {
        self.type_path
    }

    /// Returns the compile-time [`TypeInfo`] of the registered type.
    #[inline(always)]
    pub fn type_info(&self) -> &'static TypeInfo {
        self.type_info
    }

    /// Returns the generic parameters of this type (see [`TypeInfo::generics`]).
    #[inline]
    pub fn generics(&self) -> Generics {
        self.type_info.generics()
    }

    /// Returns the custom attributes attached to this type (see [`TypeInfo::attributes`]).
    #[inline]
    pub fn attributes(&self) -> Attributes {
        self.type_info.attributes()
    }

    /// Returns the documentation string for the type, if `reflect_docs` is
    /// enabled and docs are present.
    #[inline]
    pub fn docs(&self) -> Option<&'static str> {
        self.type_info.docs()
    }

    /// Constructs an instance of `Self` from a boxed reflected value by
    /// delegating to [`Reflect::from_reflect`].
    #[inline]
    pub fn from_reflect(
        &self,
        val: Box<dyn Reflect>,
    ) -> Result<Box<dyn Reflect>, Box<dyn Reflect>> {
        (self.from_reflect)(val)
    }

    /// Reconstructs a `&dyn Reflect` from a raw [`Ptr`].
    ///
    /// # Safety
    ///
    /// `ptr` must point to a valid, initialized value of the registered
    /// type with the correct alignment.
    #[expect(unsafe_code, reason = "pointer operation")]
    #[inline]
    pub unsafe fn reflect_from_ptr<'a>(&self, ptr: Ptr<'a>) -> &'a dyn Reflect {
        unsafe { (self.ref_from_ptr)(ptr) }
    }

    /// Reconstructs a `&mut dyn Reflect` from a raw [`PtrMut`].
    ///
    /// # Safety
    ///
    /// `ptr` must point to a valid, initialized, uniquely-referenced value
    /// of the registered type with the correct alignment.
    #[expect(unsafe_code, reason = "pointer operation")]
    #[inline]
    pub unsafe fn reflect_from_ptr_mut<'a>(&self, ptr: PtrMut<'a>) -> &'a mut dyn Reflect {
        unsafe { (self.mut_from_ptr)(ptr) }
    }
}

// ----------------------------------------------------------------------------
// Default
// ----------------------------------------------------------------------------

mod default;

// ----------------------------------------------------------------------------
// Convert
// ----------------------------------------------------------------------------

mod convert;

// ----------------------------------------------------------------------------
// Serialize & Deserialize
// ----------------------------------------------------------------------------

crate::cfg::debug! {
    mod info_stack;
    use info_stack::TypeInfoStack;
}

mod des;
mod ser;

// ----------------------------------------------------------------------------
// Bulk Registration
// ----------------------------------------------------------------------------

impl TypeDB {
    /// Triggers registration of all types submitted via [`register!`].
    ///
    /// This iterates over the linker-section entries populated by
    /// [`register!`] and calls each registration function. Idempotent —
    /// safe to call multiple times; subsequent calls are no-ops.
    ///
    /// Typical usage is to call this once at program startup, before any
    /// ECS or reflection operations.
    ///
    /// [`register!`]: crate::register!
    pub fn collect() {
        #[cold]
        #[inline(never)]
        fn collect_internal() {
            use __internal__::__TypeReg__ as Reg;
            use std::time::Instant;
            const PRE: usize = 800;

            let start = Instant::now();
            log::info!("Collecting TypeDB registrations...");

            {
                // pre-reserve, for better register speed.
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

            log::info!("TypeDB collection finished in {:?}", start.elapsed());
        }

        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(collect_internal);
    }
}

/// Linker-section plumbing for auto-registration.
///
/// Not intended for direct use. Prefer the [`register!`](crate::register!) macro.
pub mod __internal__ {
    pub use zlim_reg::submit;

    use super::{TypeDB, TypeDatabase};

    /// A registration token placed in the platform linker section.
    ///
    /// When [`TypeDB::collect`] is called, all submitted tokens are
    /// iterated and their inner function is invoked to register each
    /// type.
    #[repr(transparent)]
    pub struct __TypeReg__(pub(super) fn() -> &'static TypeDB);

    impl __TypeReg__ {
        /// Creates a registration token for type `T`.
        #[inline(always)]
        pub const fn of<T: TypeDatabase>() -> Self {
            Self(TypeDB::register::<T>)
        }
    }

    zlim_reg::collect!(__TypeReg__);
}

/// Registers one or more types in the [`TypeDB`] database at program
/// startup.
///
/// Uses [`zlim_reg`] linker-section machinery to ensure types are
/// registered before `main()` when [`TypeDB::collect`] is called.
///
/// # Example
///
/// ```ignore
/// use zlim_reflect::register;
///
/// register!(MyComponent, MyResource, MyEvent);
/// ```
///
/// # Requirements
///
/// Each listed type must implement [`TypeDatabase`].
#[macro_export]
macro_rules! register {
    ($($ty:ty),* $(,)?) => {
        const _: () = {
            $(
                $crate::db::__internal__::submit!(
                    $crate::db::__internal__::__TypeReg__::of::<$ty>()
                    => $crate::db::__internal__::__TypeReg__
                );
            )*
        };
    };
}

// ----------------------------------------------------------------------------
// Tests
