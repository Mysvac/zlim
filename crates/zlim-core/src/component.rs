//! Component Core Implementation

use core::alloc::Layout;
use core::any::TypeId;
use core::fmt::{Debug, Formatter};
use core::ptr::NonNull;
use std::sync::{PoisonError, RwLock};

use erased_serde::Deserializer as ErasedDeserializer;
use erased_serde::Serialize as ErasedSerialize;
use serde::{Deserialize, Serialize};
use zlim_ptr::{OwningPtr, Ptr, PtrMut};
use zlim_reflect::{Reflect, TypePath};
use zlim_utils::debug::DebugLocation;
use zlim_utils::ext::TypeMap;
use zlim_utils::hash::HashMap;
use zlim_utils::mem::{Bump, Global};

use crate::clone::ComponentCloner;
use crate::entity::{EntityId, EntityMapper};
use crate::utils::Dropper;
use crate::world::DeferredWorld;

pub use zlim_core_derive::Component;

// -----------------------------------------------------------------------------
// ComponentId
// -----------------------------------------------------------------------------

crate::utils::define_ident!(
    /// A unique identifier for a `Component` type.
    ///
    /// This ID is shared by all worlds.
    ComponentId
);

// -----------------------------------------------------------------------------
// ComponentId
// -----------------------------------------------------------------------------

/// Context passed to [`Component`] lifecycle hooks.
///
/// Identifies which component type triggered the hook (`id`), which entity
/// it belongs to (`entity`), and the source location that caused the hook
/// to fire (`caller`).
#[derive(Debug, Clone, Copy)]
pub struct HookContext {
    /// The [`ComponentId`] of the component that triggered the hook.
    pub id: ComponentId,
    /// The [`EntityId`] of the entity the component belongs to.
    pub entity: EntityId,
    /// The source location (`file:line:column`) where the hook was triggered.
    pub caller: DebugLocation,
}

/// A lifecycle hook for [`Component`]s.
///
/// A function pointer that receives deferred world access along with a
/// [`HookContext`] describing the triggering component, entity, and location.
pub type ComponentHook = fn(DeferredWorld, HookContext);

// -----------------------------------------------------------------------------
// Component
// -----------------------------------------------------------------------------

/// The core trait for all component types.
///
/// Any type stored in ECS component storage must implement this trait.
///
/// `Component` describes runtime metadata that drives how ECS stores and
/// manages values of this type: memory layout, clone and drop behavior, etc.
///
/// # Derive Macro
///
/// Most users should not implement this trait manually. Prefer deriving it with
/// [Component derive macro], which sets sensible defaults and validates options.
///
/// ```ignore
/// # use voker_ecs::derive::Component;
/// // Basic usage
/// #[derive(TypePath, Component, Clone, Serialize, Deserialize)]
/// struct Foo;
///
/// // Expose field to editor
/// #[derive(TypePath, Component, Clon, Serialize, Deserialize)]
/// struct Transform{
///     #[editor(mutable)]
///     x: f32,
///     #[editor(mutable)]
///     y: f32,
///     #[editor(mutable)]
///     z: f32,
/// }
///
/// // Contains Entity:
/// #[derive(TypePath, Component, Clon, Serialize, Deserialize)]
/// struct Linked {
///     #[entities]
///     linked_entities: BTreeSet<EntityId>,
/// }
/// ```
///
/// See [Component derive macro] documentation for details.
///
/// # Safety
///
/// Implementing this trait promises that the type can be stored behind the
/// ECS' type-erased resource storage. If you override [`Self::DROPPER`],
/// they must match the implementor's actual layout and drop behavior.
///
/// [Component derive macro]: crate::derive::Component
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a `Component`",
    label = "invalid `Component`",
    note = "consider annotating `{Self}` with `#[derive(Component)]`"
)]
pub trait Component: TypePath + Send + Sync + Sized + Serialize + for<'d> Deserialize<'d> {
    /// When `true`, this component does not belong to a specific entity.
    ///
    /// Defaults to `false`.
    const NO_ENTITY: bool = false;

    /// An optional function pointer to drop the component when it is deallocated.
    ///
    /// Defaults to `Some(Dropper::of::<Self>())` which calls [`drop`] on `Self`.
    const DROPPER: Option<Dropper> = Dropper::of::<Self>();

    /// The cloning strategy for this component.
    ///
    /// Set by `#[derive(Component)]` to one of `clonable`, `copyable`, or `custom(fn)`.
    const CLONER: ComponentCloner;

    /// Hook invoked when the component is **first** added to an entity.
    ///
    /// (i.e. entity spawn or insert new component).
    ///
    /// Called after component initialization is complete, before `on_insert`.
    const ON_ADD: Option<ComponentHook> = None;

    /// Hook invoked when this component instance is created by cloning another.
    ///
    /// (i.e. entity clone).
    ///
    /// Called after entity cloning is complete, before `on_add` and `on_insert`.
    const ON_CLONE: Option<ComponentHook> = None;

    /// Hook invoked on every insertion, including updates to an entity that
    /// already had this component type.
    ///
    /// (i.e. entity spawn, clone or component insert).
    ///
    /// Called after component initialization is complete, after `on_add`.
    const ON_INSERT: Option<ComponentHook> = None;

    /// Hook invoked when the component is removed from an entity.
    ///
    /// (i.e. component remove or entity despawn).
    ///
    /// Call before component is actually removed, after `on_discard`.
    const ON_REMOVE: Option<ComponentHook> = None;

    /// Hook invoked when the component is discarded.
    ///
    /// (i.e. component replace, remove or entity despawn).
    ///
    /// Call before component is actually removed, before `on_remove` and `on_despawn`.
    const ON_DISCARD: Option<ComponentHook> = None;

    /// Hook invoked when the owning entity is despawned.
    ///
    /// (i.e. entity despawn).
    ///
    /// Call before component is actually dropped, after `on_discard` and `on_remove`.
    const ON_DESPAWN: Option<ComponentHook> = None;

    /// Names of all fields available for reflection. Defaults to empty.
    const FIELDS: &'static [&'static str] = &[];

    /// Names of fields that editors may mutate. Defaults to empty.
    const MUTABLE_FIELDS: &'static [&'static str] = &[];

    /// Names of fields that editors may read but not mutate. Defaults to empty.
    const READONLY_FIELDS: &'static [&'static str] = &[];

    /// Returns a shared reference to the named reflected field, if it exists.
    fn field(&self, name: &str) -> Option<&dyn Reflect>;

    /// Returns a mutable reference to the named reflected field, if it exists.
    fn field_mut(&mut self, name: &str) -> Option<&mut dyn Reflect>;

    /// Remaps entity references inside this component.
    ///
    /// Called during entity cloning / scene instantiation. The default
    /// implementation is a no-op. Override this if your component stores
    /// [`EntityId`] values that need remapping.
    #[inline(always)]
    fn map_entities<M: EntityMapper>(&mut self, _: &mut M) {}
}

// -----------------------------------------------------------------------------
// alias
// -----------------------------------------------------------------------------

/// Type-erased function pointer aliases used by [`ComponentDB`].
///
/// These allow the component database to store type-independent function
/// pointers for field reflection, entity mapping, serialization, and
/// deserialization.
pub mod alias {
    use crate::entity::EntityMapper;
    use erased_serde::{Deserializer, Error, Serialize};
    use zlim_ptr::{OwningPtr, Ptr, PtrMut};
    use zlim_reflect::Reflect;
    use zlim_utils::mem::Bump;

    /// Type-erased function for reading a field by name via reflection.
    ///
    /// Returns `Some(&dyn Reflect)` on success, `None` if the field does not exist.
    pub type FieldRefFunc = for<'a> unsafe fn(Ptr<'a>, &str) -> Option<&'a dyn Reflect>;

    /// Type-erased function for mutably accessing a field by name via reflection.
    ///
    /// Returns `Some(&mut dyn Reflect)` on success, `None` if the field does not exist.
    pub type FieldMutFunc = for<'a> unsafe fn(PtrMut<'a>, &str) -> Option<&'a mut dyn Reflect>;

    /// Type-erased function that remaps entity references within a component instance.
    pub type MapEntitiesFunc = unsafe fn(PtrMut<'_>, &mut dyn EntityMapper);

    /// Type-erased function that returns an `erased_serde::Serialize` reference
    /// from a component pointer.
    pub type SeriailizeFunc = for<'a> fn(Ptr<'a>) -> &'a dyn Serialize;

    /// Type-erased function that deserializes a component from an `erased_serde`
    /// deserializer, allocating through a [`Bump`].
    pub type DeserializeFunc =
        for<'a, 'b> fn(&'a mut dyn Deserializer, &'b Bump) -> Result<OwningPtr<'b>, Error>;
}

use alias::*;

// -----------------------------------------------------------------------------
// ComponentDB
// -----------------------------------------------------------------------------

/// Static metadata for a single component type.
///
/// Created lazily by [`ComponentDB::register`] and stored in the global
/// `ID_REGISTRY`. Holds type identity, lifecycle hooks, field introspection
/// function pointers, memory layout, clone/drop strategy, and serialization
/// routines — all type-erased so they can be stored homogeneously.
pub struct ComponentDB {
    // --------------------------------
    // Ident
    /// Unique identifier assigned at registration time.
    pub id: ComponentId,
    /// Opaque [`TypeId`] for runtime type comparison.
    pub type_id: TypeId,

    /// Fully-qualified type path (e.g. `"my_crate::components::Transform"`).
    pub typa_path: &'static str,
    /// Short type name (e.g. `"Transform"`).
    pub typa_name: &'static str,
    /// Module path of the type definition.
    pub module_path: &'static str,

    // --------------------------------
    // Hook
    /// Hook invoked on first add to an entity.
    pub on_add: Option<ComponentHook>,
    /// Hook invoked when a component is cloned.
    pub on_clone: Option<ComponentHook>,
    /// Hook invoked on every insertion (including updates).
    pub on_insert: Option<ComponentHook>,
    /// Hook invoked when the component is removed from its entity.
    pub on_remove: Option<ComponentHook>,
    /// Hook invoked when the component is discarded (entity alive, value dropped).
    pub on_discard: Option<ComponentHook>,
    /// Hook invoked when the owning entity is despawned.
    pub on_despawn: Option<ComponentHook>,

    // --------------------------------
    // Editor accessor
    /// Names of all reflected fields.
    pub fields: &'static [&'static str],
    /// Names of fields editors are allowed to mutate.
    pub mutable_fields: &'static [&'static str],
    /// Names of fields editors may read but not mutate.
    pub readonly_fields: &'static [&'static str],
    /// Type-erased accessor for shared field reflection.
    pub field_ref_func: FieldRefFunc,
    /// Type-erased accessor for mutable field reflection.
    pub field_mut_func: FieldMutFunc,

    // --------------------------------
    // Memory Layout
    /// Memory layout (size + alignment) of `Self`.
    pub layout: Layout,
    /// Cloning strategy for this component.
    pub cloner: ComponentCloner,
    /// Optional custom dropper; `None` means standard drop.
    pub dropper: Option<Dropper>,
    /// Type-erased entity-remapping function.
    pub map_entities: MapEntitiesFunc,

    // --------------------------------
    // Serialization
    /// Type-erased serialization function pointer.
    pub serialize: SeriailizeFunc,
    /// Type-erased deserialization function pointer.
    pub deserialize: DeserializeFunc,
}

impl Debug for ComponentDB {
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

    /// Looks up a [`ComponentDB`] by its [`TypeId`].
    ///
    /// Returns `None` if the type has not been registered yet.
    pub fn get_by_type(id: TypeId) -> Option<&'static ComponentDB> {
        TYPE_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .copied()
    }

    /// Looks up a [`ComponentDB`] by its fully-qualified type path (e.g.
    /// `"my_crate::components::Transform"`).
    ///
    /// Returns `None` if the type has not been registered yet.
    pub fn get_by_path(path: &str) -> Option<&'static ComponentDB> {
        PATH_REGISTRY
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(path)
            .copied()
    }

    /// Registers component type `C` in the global registries and returns
    /// its `&'static` [`ComponentDB`].
    ///
    /// This is a cold path — the function double-checks the read lock first
    /// and only proceeds with registration if the type is genuinely unknown.
    /// The returned reference lives for the lifetime of the program.
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
    /// Triggers bulk registration of all component types submitted via the
    /// `register_component!` macro or `#[derive(Component)]`.
    ///
    /// Internally iterates the `__ComponentReg__` registry, calling each
    /// registration function. The process is guarded by [`std::sync::Once`]
    /// so it only runs once per program lifetime.
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

    /// A registration token that defers [`ComponentDB::register`] for a type.
    ///
    /// Collecting these tokens via [`zlim_reg::collect!`] enables bulk
    /// registration at startup instead of incurring the cold-path cost
    /// on every first access.
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

/// Submits one or more component types for bulk registration.
///
/// Equivalent to calling [`ComponentDB::register`] for each listed type,
/// but defers the actual work until [`ComponentDB::collect`] is called.
/// This amortizes the cold-path cost of lazy registration at startup.
///
/// # Example
///
/// ```ignore
/// register_component!(Transform, Velocity);
/// ```
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

/// A local snapshot of the global component registry.
///
/// Created from the global `ID_REGISTRY`, `TYPE_REGISTRY`, and
/// `PATH_REGISTRY` at construction time. Provides fast local lookups
/// by id, type path, or [`TypeId`] without needing to acquire the global
/// locks.
pub struct Components {
    /// Ordered list of all currently-known component descriptors.
    dbs: Vec<&'static ComponentDB>,
    /// Indexed by [`TypeId`] for O(1) lookup.
    type_map: TypeMap<&'static ComponentDB>,
    /// Indexed by type path string for O(1) lookup.
    path_map: HashMap<&'static str, &'static ComponentDB>,
}

impl Debug for Components {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&self.dbs, f)
    }
}

impl Default for Components {
    fn default() -> Self {
        crate::cfg::debug! {
            #[cfg(not(test))]
            let start = ::std::time::Instant::now();
        }

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

        crate::cfg::debug! {
            #[cfg(not(test))]
            log::debug!("`zlim_core::Components` initialized: {:?}`", start.elapsed());
        }

        Self {
            dbs,
            type_map,
            path_map,
        }
    }
}

impl Components {
    /// Refreshes this snapshot from the global `ID_REGISTRY`.
    ///
    /// Picks up any component types that were registered after this
    /// `Components` instance was created. New entries are appended to
    /// `dbs`, `type_map`, and `path_map`.
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

    /// Return the number of registered components.
    pub fn len(&self) -> usize {
        self.dbs.len()
    }

    /// Returns the [`ComponentDB`] for component type `C`.
    ///
    /// Checks the local `type_map` first (fast path). If the type is not
    /// yet in this snapshot, falls back to lazy registration via
    /// [`ComponentDB::register`].
    pub fn get<C: Component>(&self) -> &'static ComponentDB {
        if let Some(&r) = self.type_map.get(TypeId::of::<C>()) {
            return r;
        }
        ::core::hint::cold_path();
        ComponentDB::register::<C>()
    }

    /// Looks up a [`ComponentDB`] by its [`ComponentId`].
    ///
    /// First checks the local `dbs` slice via index (fast path). On a miss
    /// falls back to the global `ID_REGISTRY`.
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

    /// Looks up a [`ComponentDB`] by its fully-qualified type path.
    ///
    /// First checks the local `path_map` (fast path). On a miss falls
    /// back to the global `PATH_REGISTRY`.
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

    /// Looks up a [`ComponentDB`] by its [`TypeId`].
    ///
    /// First checks the local `type_map` (fast path). On a miss falls
    /// back to the global `TYPE_REGISTRY`.
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

/// Type-erased entity mapper for `C::map_entities`.
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

// -----------------------------------------------------------------------------
