//! Entity handles and their operations.
//!
//! Defines the four entity view types — [`Entity`], [`EntityRef`],
//! [`EntityMut`], and [`EntityOwned`] — together with the component-access
//! traits [`FetchComponents`] and [`GetComponents`].

// -----------------------------------------------------------------------------
// Modules
// -----------------------------------------------------------------------------

mod clear;
mod clone;
mod despawn;
mod fetch_trait;
mod get_trait;
mod insert;
mod remove;

pub use fetch_trait::FetchComponents;
pub use get_trait::GetComponents;

// -----------------------------------------------------------------------------
// Inline Content
// -----------------------------------------------------------------------------

use core::any::TypeId;
use core::fmt::{Debug, Formatter};
use core::ptr;

use zlim_ptr::Ptr;
use zlim_utils::debug::DebugLocation;

use crate::borrow::Res;
use crate::borrow::ResMut;
use crate::borrow::UntypedMut;
use crate::borrow::UntypedRef;
use crate::bundle::{Bundle, DataBundle};
use crate::component::ComponentId;
use crate::entity::EntityId;
use crate::entity::Location;
use crate::entity::{EntityError, EntityNode};
use crate::resource::Resource;
use crate::table::Table;
use crate::tick::Tick;
use crate::utils::DebugCheckedUnwrap;
use crate::world::WorldCell;
use crate::world::{DeferredWorld, World};

// -----------------------------------------------------------------------------
// Entity & EntityRef & EntityMut & EntityOwned
// -----------------------------------------------------------------------------

/// A mutable view that can access the entity's component data and hierarchy,
/// but cannot change the entity's structure (no add/remove components).
///
/// This is the most capable non-owning view: it carries a [`WorldCell`]
/// handle, the entity's [`EntityNode`] for hierarchy traversal, and
/// mutable access to the component table.
///
/// Obtained via [`World::entity`], [`World::get_entity`],
/// [`TryFrom<EntityOwned>`] or [`EntityOwned::as_view`].
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone, PartialEq, Debug)]
/// struct Hp(u32);
///
/// let mut world = World::alloc();
/// let mut owned = world.spawn(Hp(100), None);
///
/// // `Entity` can read and mutate components and traverse the hierarchy,
/// // but cannot add or remove components.
/// let mut view = owned.as_view();
/// assert_eq!(view.get::<Hp>(), Some(&Hp(100)));
///
/// *view.get_mut::<Hp>().unwrap().into_inner() = Hp(75);
/// assert_eq!(owned.get::<Hp>(), Some(&Hp(75)));
/// ```
///
/// [`WorldCell`]: crate::world::WorldCell
/// [`EntityNode`]: crate::entity::EntityNode
/// [`World::entity`]: crate::world::World::entity
/// [`World::get_entity`]: crate::world::World::get_entity
pub struct Entity<'w> {
    pub(crate) id: EntityId,
    pub(crate) world: WorldCell<'w>,
    pub(crate) node: &'w EntityNode,
    pub(crate) table: &'w mut Table,
    pub(crate) location: Location,
}

/// A read-only view of a spawned entity's component data.
///
/// Provides shared access with change detection.  Obtained from
/// [`EntityOwned`], [`Entity`], or [`EntityMut`] via their respective
/// conversion methods, or directly from the world through
/// [`World::entity_ref`] / [`World::get_entity_ref`].
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone, PartialEq, Debug)]
/// struct Name(&'static str);
///
/// let mut world = World::alloc();
/// let entity = world.spawn(Name("hero"), None);
///
/// // `EntityRef` yields change-aware references: `Ref<T>` reports whether
/// // the component was added or changed since the last run.
/// let read = entity.as_readonly();
/// let name = read.get_ref::<Name>().unwrap();
/// assert!(name.is_added());
/// assert_eq!(name.into_inner(), &Name("hero"));
/// ```
///
/// It can also be used to query target.
///
/// ```ignore
/// fn system(query: Query<EntityRef, With<Name>>) {
///     for entity in query {
///         // ......
///     }
/// }
/// ```
///
/// When used as a query target, EntityRef represents read-only access
/// to all components, so no other mutable access should exist.
///
/// ```ignore
/// // ↓ At present, this system will not panic, but log a error.
/// fn system(query: Query<(EntityRef, &mut Name), With<Name>>) {
///     for entity in query { // ↑ ❌️ ↑
///         // ......
///     }
/// }
/// ```
///
/// [`World::entity_ref`]: crate::world::World::entity_ref
/// [`World::get_entity_ref`]: crate::world::World::get_entity_ref
pub struct EntityRef<'w> {
    pub(crate) id: EntityId,
    pub(crate) table: &'w Table,
    pub(crate) location: Location,
    pub(crate) last_run: Tick,
    pub(crate) this_run: Tick,
}

/// A mutable view limited to the entity's own component data.
///
/// Provides exclusive access with change detection, but cannot traverse
/// hierarchy or perform structural mutations.  Obtained from
/// [`EntityOwned`] or [`Entity`] via their respective conversion methods,
/// or directly from the world through [`World::entity_mut`] /
/// [`World::get_entity_mut`].
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone, PartialEq, Debug)]
/// struct Hp(u32);
///
/// let mut world = World::alloc();
/// let mut entity = world.spawn(Hp(100), None);
///
/// // `EntityMut` gives exclusive, change-aware access to components.
/// let mut view = entity.as_mutable();
/// *view.get_mut::<Hp>().unwrap().into_inner() = Hp(50);
/// assert_eq!(view.get::<Hp>(), Some(&Hp(50)));
/// ```
///
/// It can also be used to query target.
///
/// ```ignore
/// fn system(query: Query<EntityMut, With<Name>>) {
///     for entity in query {
///         // ......
///     }
/// }
/// ```
///
/// When used as a query target, EntityMut represents mutable access
/// to all components, so no other readonly/mutable access should exist.
///
/// ```ignore
/// // ↓ At present, this system will not panic, but log a error.
/// fn system(query: Query<(EntityMut, &Name), With<Name>>) {
///     for entity in query { // ↑ ❌️ ↑
///         // ......
///     }
/// }
/// ```
///
/// [`World::entity_mut`]: crate::world::World::entity_mut
/// [`World::get_entity_mut`]: crate::world::World::get_entity_mut
pub struct EntityMut<'w> {
    pub(crate) id: EntityId,
    pub(crate) table: &'w mut Table,
    pub(crate) location: Location,
    pub(crate) last_run: Tick,
    pub(crate) this_run: Tick,
}

/// A fully-mutable owning handle — equivalent to `World` + `EntityId`.
///
/// With [`EntityOwned`], you can:
///
/// - Insert and remove components (structural mutation).
/// - Traverse and modify the entity hierarchy.
/// - Despawn the entity.
/// - Access the underlying [`World`] directly.
///
/// The internal [`EntityId`] is tracked automatically.  When the entity
/// is despawned (or otherwise invalidated), many operations return
/// `Err(EntityError)` instead of panicking.  Panics only occur when you
/// explicitly call a panicking conversion (e.g.
/// [`into_view`](Self::into_view)).
///
/// Obtained via [`World::spawn`] or [`World::entity_owned`].
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone, PartialEq, Debug)]
/// struct Hp(u32);
///
/// #[derive(TypePath, Component, Clone, PartialEq, Debug)]
/// struct Speed(f32);
///
/// let mut world = World::alloc();
///
/// // `World::spawn` returns an `EntityOwned` handle.
/// let mut player = world.spawn(Hp(100), None);
/// player.insert(Speed(3.0)).unwrap();
/// assert_eq!(player.get::<Speed>(), Some(&Speed(3.0)));
///
/// // The handle can also spawn children and manage the hierarchy.
/// player.with_child(Hp(10)).unwrap();
/// assert_eq!(player.children().unwrap().len(), 1);
///
/// // Despawning the parent recursively despawns its descendants.
/// player.despawn().unwrap();
/// ```
///
/// [`World::spawn`]: crate::world::World::spawn
/// [`World::entity_owned`]: crate::world::World::entity_owned
/// [`World`]: crate::world::World
/// [`EntityId`]: crate::entity::EntityId
/// [`EntityError`]: crate::entity::EntityError
pub struct EntityOwned<'w> {
    pub(crate) id: EntityId,
    pub(crate) world: WorldCell<'w>,
    pub(crate) storage: Option<(&'w mut Table, Location)>,
}

// -----------------------------------------------------------------------------
// Debug
// -----------------------------------------------------------------------------

macro_rules! impl_common_debug {
    ($name:ident) => {
        impl Debug for $name<'_> {
            fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
                f.debug_struct(stringify!($name))
                    .field("id", &self.id)
                    .field("location", &self.location)
                    .finish()
            }
        }
    };
}

impl_common_debug!(Entity);
impl_common_debug!(EntityRef);
impl_common_debug!(EntityMut);

impl Debug for EntityOwned<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        if let Some(s) = &self.storage {
            f.debug_struct("EntityOwned")
                .field("id", &self.id)
                .field("location", &s.1)
                .finish()
        } else {
            f.debug_struct("EntityOwned")
                .field("id", &self.id)
                .field("location", &"None")
                .finish()
        }
    }
}

// -----------------------------------------------------------------------------
// Reborrow
// -----------------------------------------------------------------------------

impl Entity<'_> {
    /// Creates a shorter-lived reborrow of this entity view.
    #[inline(always)]
    pub fn reborrow(&mut self) -> Entity<'_> {
        // SAFETY: no need drop
        unsafe { ptr::read(self) }
    }
}

impl EntityRef<'_> {
    /// Creates a shorter-lived reborrow of this entity view.
    #[inline(always)]
    pub fn reborrow(&self) -> EntityRef<'_> {
        // SAFETY: no need drop
        unsafe { ptr::read(self) }
    }
}

impl EntityMut<'_> {
    /// Creates a shorter-lived reborrow of this entity view.
    #[inline(always)]
    pub fn reborrow(&mut self) -> EntityMut<'_> {
        // SAFETY: no need drop
        unsafe { ptr::read(self) }
    }
}

// Cannot implement `reborrow`, we need impl auto `reload`.
// impl EntityOwned<'_> {
//     #[inline(always)]
//     pub fn reborrow(&mut self) -> EntityOwned<'_> {
//         // SAFETY: no need drop
//         unsafe { ptr::read(self) }
//     }
// }

// -----------------------------------------------------------------------------
// From & TryFrom
// -----------------------------------------------------------------------------

macro_rules! impl_common_try_from {
    ($name:ident) => {
        impl<'a> From<Entity<'a>> for $name<'a> {
            #[inline]
            fn from(value: Entity<'a>) -> Self {
                let world = unsafe { value.world.full_mut() };
                let last_run = world.last_run();
                let this_run = world.this_run_fast();
                $name {
                    id: value.id,
                    table: value.table,
                    location: value.location,
                    last_run,
                    this_run,
                }
            }
        }

        impl<'a> TryFrom<EntityOwned<'a>> for $name<'a> {
            type Error = EntityError;
            #[inline]
            fn try_from(value: EntityOwned<'a>) -> Result<Self, Self::Error> {
                if value.storage.is_none() {
                    core::hint::cold_path();
                    return Err(EntityError::NotSpawned(value.id));
                }

                let (table, location) = unsafe { value.storage.unwrap_unchecked() };
                let world = unsafe { value.world.full_mut() };
                let last_run = world.last_run();
                let this_run = world.this_run_fast();
                Ok($name {
                    id: value.id,
                    table,
                    location,
                    last_run,
                    this_run,
                })
            }
        }
    };
}

impl_common_try_from!(EntityRef);
impl_common_try_from!(EntityMut);

impl<'a> From<EntityMut<'a>> for EntityRef<'a> {
    #[inline]
    fn from(value: EntityMut<'a>) -> Self {
        EntityRef {
            id: value.id,
            table: value.table,
            location: value.location,
            last_run: value.last_run,
            this_run: value.this_run,
        }
    }
}

impl<'a> TryFrom<EntityOwned<'a>> for Entity<'a> {
    type Error = EntityError;

    #[inline]
    fn try_from(value: EntityOwned<'a>) -> Result<Self, Self::Error> {
        if value.storage.is_none() {
            core::hint::cold_path();
            return Err(EntityError::NotSpawned(value.id));
        }

        let (table, location) = unsafe { value.storage.unwrap_unchecked() };
        let node = unsafe {
            let world = value.world.data_mut();
            world.entities.get(value.id).debug_checked_unwrap()
        };

        Ok(Entity {
            id: value.id,
            world: value.world,
            node,
            table,
            location,
        })
    }
}

// -----------------------------------------------------------------------------
// Helper
// -----------------------------------------------------------------------------

impl Entity<'_> {
    #[inline(always)]
    fn this_run(&self) -> Tick {
        unsafe { self.world.full_mut().this_run_fast() }
    }

    #[inline(always)]
    fn last_run(&self) -> Tick {
        unsafe { self.world.read_only().last_run() }
    }
}

impl EntityOwned<'_> {
    #[cold]
    #[inline(never)]
    fn panic_despawned(&self, caller: DebugLocation) -> ! {
        let world = unsafe { self.world.read_only() };
        let id = self.id;
        let info = world.entities.locate(self.id).unwrap_err();
        panic!("`EntityOwned` try operate a despawned Entity({id}): {info}, {caller}.");
    }

    #[cold]
    #[inline(never)]
    fn assert_is_spawned(&self, caller: DebugLocation) {
        if self.storage.is_none() {
            self.panic_despawned(caller);
        }
    }

    #[inline(always)]
    fn this_run(&self) -> Tick {
        unsafe { self.world.full_mut().this_run_fast() }
    }

    #[inline(always)]
    fn last_run(&self) -> Tick {
        unsafe { self.world.read_only().last_run() }
    }
}

// -----------------------------------------------------------------------------
// EntityOwned Validation Checker
// -----------------------------------------------------------------------------

impl EntityOwned<'_> {
    /// Check if the current Entity Owned is valid.
    ///
    /// - Return `Ok(())` if the entity is spawned.
    /// - Return `Err(EntityError::NotSpawned)` if the entity is despawned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let mut entity = world.spawn((), None);
    /// assert!(entity.validate().is_ok());
    ///
    /// // `world_scope` hands out raw `&mut World` access; despawning the
    /// // entity there invalidates the handle, and the scope guard refreshes
    /// // the cached location on the way out.
    /// let id = entity.id();
    /// entity.world_scope(|world| world.despawn(id).unwrap());
    /// assert!(entity.validate().is_err());
    /// ```
    #[inline(always)]
    pub fn validate(&self) -> Result<(), EntityError> {
        if self.storage.is_none() {
            Err(EntityError::NotSpawned(self.id))
        } else {
            Ok(())
        }
    }

    /// Returns `true` if the entity is spawned.
    ///
    /// Note that this function checks the cached [`Location`] directly;
    /// call [`EntityOwned::relocate`] first if you need to refresh it.
    #[inline(always)]
    pub fn is_spawned(&self) -> bool {
        self.storage.is_some()
    }

    /// Returns `true` if the entity is despawned.
    ///
    /// Note that this function checks the cached [`Location`] directly;
    /// call [`EntityOwned::relocate`] first if you need to refresh it.
    #[inline(always)]
    pub fn is_despawned(&self) -> bool {
        self.storage.is_none()
    }

    /// Updates the internal entity location to match the current location
    /// in the internal [`World`].
    ///
    /// This is required after structural operations performed through raw
    /// world access — for example inside [`EntityOwned::world_scope`] — may
    /// have moved the entity between tables.  Most methods re-locate
    /// automatically and never need this.
    ///
    /// Note that if the entity is not spawned for any reason, this will leave
    /// the handle's cached location as `None`, causing the panicking
    /// conversion methods to panic.
    pub fn relocate(&mut self) {
        let world = unsafe { self.world.data_mut() };

        match world.entities.locate(self.id) {
            Err(_) => self.storage = None,
            Ok(location) => {
                let id = location.table_id;
                let table = unsafe { world.tables.get_unchecked_mut(id) };
                self.storage = Some((table, location));
            }
        }
    }
}

#[repr(transparent)]
struct RelocateGuard<'w, 'a>(&'a mut EntityOwned<'w>);

impl Drop for RelocateGuard<'_, '_> {
    fn drop(&mut self) {
        self.0.relocate();
    }
}

// -----------------------------------------------------------------------------
// Into & As
// -----------------------------------------------------------------------------

impl<'a> Entity<'a> {
    /// Consumes `self` and returns read-only access to all of the entity's
    /// components, with the world `'w` lifetime.
    #[inline]
    pub fn into_readonly(self) -> EntityRef<'a> {
        EntityRef::from(self)
    }

    /// Consumes `self` and returns non-structural mutable access to all of the
    /// entity's components, with the world `'w` lifetime.
    #[inline]
    pub fn into_mutable(self) -> EntityMut<'a> {
        EntityMut::from(self)
    }

    /// Gets read-only access to all of the entity's components.
    #[inline]
    pub fn as_readonly(&self) -> EntityRef<'_> {
        EntityRef::from(unsafe { ptr::read(self) })
    }

    /// Gets non-structural mutable access to all of the entity's components.
    #[inline]
    pub fn as_mutable(&mut self) -> EntityMut<'_> {
        EntityMut::from(unsafe { ptr::read(self) })
    }
}

impl<'a> EntityOwned<'a> {
    /// Consumes `self` and returns a mutable view, with the world `'w`
    /// lifetime.
    ///
    /// # Panics
    /// Panics if `self` is despawned.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn into_view(self) -> Entity<'a> {
        let caller = DebugLocation::caller();
        self.assert_is_spawned(caller);
        unsafe { Entity::try_from(self).unwrap_unchecked() }
    }

    /// Consumes `self` and returns read-only access to all of the entity's
    /// components, with the world `'w` lifetime.
    ///
    /// # Panics
    /// Panics if `self` is despawned.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn into_readonly(self) -> EntityRef<'a> {
        let caller = DebugLocation::caller();
        self.assert_is_spawned(caller);
        unsafe { EntityRef::try_from(self).unwrap_unchecked() }
    }

    /// Consumes `self` and returns non-structural mutable access to all of the
    /// entity's components, with the world `'w` lifetime.
    ///
    /// # Panics
    /// Panics if `self` is despawned.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn into_mutable(self) -> EntityMut<'a> {
        let caller = DebugLocation::caller();
        self.assert_is_spawned(caller);
        unsafe { EntityMut::try_from(self).unwrap_unchecked() }
    }

    /// Gets mutable access to this entity and data-mut world.
    ///
    /// # Panics
    /// Panics if `self` is despawned.
    #[inline]
    pub fn as_view(&mut self) -> Entity<'_> {
        let caller = DebugLocation::caller();
        self.assert_is_spawned(caller);
        unsafe { Entity::try_from(ptr::read(self)).unwrap_unchecked() }
    }

    /// Gets read-only access to all of the entity's components.
    ///
    /// # Panics
    /// Panics if `self` is despawned.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn as_readonly(&self) -> EntityRef<'_> {
        let caller = DebugLocation::caller();
        self.assert_is_spawned(caller);
        unsafe { EntityRef::try_from(ptr::read(self)).unwrap_unchecked() }
    }

    /// Gets non-structural mutable access to all of the entity's components.
    ///
    /// # Panics
    /// Panics if `self` is despawned.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn as_mutable(&mut self) -> EntityMut<'_> {
        let caller = DebugLocation::caller();
        self.assert_is_spawned(caller);
        unsafe { EntityMut::try_from(ptr::read(self)).unwrap_unchecked() }
    }
}

// -----------------------------------------------------------------------------
// Common Methods - 1 : id + location + contains
// -----------------------------------------------------------------------------

macro_rules! impl_common_methods_1 {
    ($name:ident) => {
        impl $name<'_> {
            /// Returns the underlying entity id.
            #[inline(always)]
            pub fn id(&self) -> EntityId {
                self.id
            }

            /// Returns this entity's location.
            #[inline(always)]
            pub fn location(&self) -> Location {
                self.location
            }

            /// Returns whether the entity's archetype contains `T`.
            ///
            /// See [`GetComponents`] for examples.
            pub fn contains<T: GetComponents>(&self) -> bool {
                T::contains(self.table)
            }

            /// Checks whether the entity contains given Component(Id).
            pub fn contains_by_id(&self, id: ComponentId) -> bool {
                self.table.contains_component(id)
            }

            /// Checks whether the entity contains given Component Type.
            pub fn contains_by_type(&self, ty: TypeId) -> bool {
                self.table.contains_type(ty)
            }
        }
    };
}

impl_common_methods_1!(Entity);
impl_common_methods_1!(EntityRef);
impl_common_methods_1!(EntityMut);

impl EntityOwned<'_> {
    /// Returns the underlying entity id.
    #[inline(always)]
    pub fn id(&self) -> EntityId {
        self.id
    }

    /// Returns this entity's location, if it's spawned.
    #[inline(always)]
    pub fn location(&self) -> Option<Location> {
        unsafe { ptr::read(&self.storage).map(|(_, x)| x) }
    }

    /// Returns whether the entity's archetype contains `T`.
    ///
    /// If the entity has been destroyed, return false directly.
    ///
    /// See [`GetComponents`] for examples.
    pub fn contains<T: GetComponents>(&self) -> bool {
        match unsafe { ptr::read(&self.storage) } {
            Some((table, _)) => T::contains(table),
            None => false,
        }
    }

    /// Checks whether the entity contains given Component(Id).
    ///
    /// If the entity has been destroyed, return false directly.
    pub fn contains_by_id(&self, id: ComponentId) -> bool {
        match unsafe { ptr::read(&self.storage) } {
            Some((table, _)) => table.contains_component(id),
            None => false,
        }
    }

    /// Checks whether the entity contains given Component Type.
    ///
    /// If the entity has been destroyed, return false directly.
    pub fn contains_by_type(&self, ty: TypeId) -> bool {
        match unsafe { ptr::read(&self.storage) } {
            Some((table, _)) => table.contains_type(ty),
            None => false,
        }
    }
}

// -----------------------------------------------------------------------------
// Common Methods - 2.1 : get + get_ref
// -----------------------------------------------------------------------------

macro_rules! impl_common_methods {
    ($name:ident) => {
        impl<'a> $name<'a> {
            /// Gets change-aware shared component reference for `T`.
            ///
            /// See [`GetComponents`] for examples.
            pub fn get_ref<T: GetComponents>(&self) -> Option<T::Ref<'_>> {
                let last_run = self.last_run;
                let this_run = self.this_run;
                let row = self.location.table_row;
                unsafe { T::get_ref(self.table, row, last_run, this_run) }
            }

            /// Gets change-aware shared component reference for `T`.
            ///
            /// See [`GetComponents`] for examples.
            #[inline]
            pub fn into_ref<T: GetComponents>(self) -> Option<T::Ref<'a>> {
                let last_run = self.last_run;
                let this_run = self.this_run;
                let row = self.location.table_row;
                unsafe { T::get_ref(self.table, row, last_run, this_run) }
            }

            /// Gets raw shared type-erased pointer by given ComponentId.
            pub fn get_by_id(&self, id: ComponentId) -> Option<Ptr<'_>> {
                let col = self.table.get_table_col(id)?;
                let row = self.location.table_row;
                Some(unsafe { self.table.get_data(row, col) })
            }

            /// Gets raw shared type-erased pointer by given Component Type.
            pub fn get_by_type(&self, ty: TypeId) -> Option<Ptr<'_>> {
                let col = self.table.get_type_col(ty)?;
                let row = self.location.table_row;
                Some(unsafe { self.table.get_data(row, col) })
            }

            /// Gets type-erased change-aware shared component reference by given ComponentId.
            pub fn get_ref_by_id(&self, id: ComponentId) -> Option<UntypedRef<'_>> {
                let col = self.table.get_table_col(id)?;
                let row = self.location.table_row;
                Some(unsafe { self.table.get_ref(row, col, self.last_run, self.this_run) })
            }

            /// Gets type-erased change-aware shared component reference by given Component Type.
            pub fn get_ref_by_type(&self, ty: TypeId) -> Option<UntypedRef<'_>> {
                let col = self.table.get_type_col(ty)?;
                let row = self.location.table_row;
                Some(unsafe { self.table.get_ref(row, col, self.last_run, self.this_run) })
            }
        }
    };
}

impl_common_methods!(EntityMut);
impl_common_methods!(EntityRef);

impl<'a> EntityRef<'a> {
    /// Gets raw shared component reference for `T`.
    ///
    /// See [`GetComponents`] for examples.
    #[inline]
    pub fn get<T: GetComponents>(&self) -> Option<T::Raw<'a>> {
        unsafe { T::get(self.table, self.location.table_row) }
    }
}

impl<'a> EntityMut<'a> {
    /// Gets raw shared component reference for `T`.
    ///
    /// See [`GetComponents`] for examples.
    #[inline]
    pub fn get<T: GetComponents>(&self) -> Option<T::Raw<'_>> {
        unsafe { T::get(self.table, self.location.table_row) }
    }
}

impl<'a> Entity<'a> {
    /// Gets raw shared component reference for `T`.
    ///
    /// See [`GetComponents`] for examples.
    #[inline]
    pub fn get<T: GetComponents>(&self) -> Option<T::Raw<'_>> {
        unsafe { T::get(self.table, self.location.table_row) }
    }

    /// Gets change-aware shared component reference for `T`.
    ///
    /// See [`GetComponents`] for examples.
    pub fn get_ref<T: GetComponents>(&self) -> Option<T::Ref<'_>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let row = self.location.table_row;
        unsafe { T::get_ref(self.table, row, last_run, this_run) }
    }

    /// Gets change-aware shared component reference for `T`.
    ///
    /// See [`GetComponents`] for examples.
    pub fn into_ref<T: GetComponents>(self) -> Option<T::Ref<'a>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let row = self.location.table_row;
        unsafe { T::get_ref(self.table, row, last_run, this_run) }
    }

    /// Gets raw shared type-erased pointer by given ComponentId.
    pub fn get_by_id(&self, id: ComponentId) -> Option<Ptr<'_>> {
        let col = self.table.get_table_col(id)?;
        let row = self.location.table_row;
        Some(unsafe { self.table.get_data(row, col) })
    }

    /// Gets raw shared type-erased pointer by given Component Type.
    pub fn get_by_type(&self, ty: TypeId) -> Option<Ptr<'_>> {
        let col = self.table.get_type_col(ty)?;
        let row = self.location.table_row;
        Some(unsafe { self.table.get_data(row, col) })
    }

    /// Gets type-erased change-aware shared component reference by given ComponentId.
    pub fn get_ref_by_id(&self, id: ComponentId) -> Option<UntypedRef<'_>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let col = self.table.get_table_col(id)?;
        let row = self.location.table_row;
        Some(unsafe { self.table.get_ref(row, col, last_run, this_run) })
    }

    /// Gets type-erased change-aware shared component reference by given Component Type.
    pub fn get_ref_by_type(&self, ty: TypeId) -> Option<UntypedRef<'_>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let col = self.table.get_type_col(ty)?;
        let row = self.location.table_row;
        Some(unsafe { self.table.get_ref(row, col, last_run, this_run) })
    }
}

impl<'a> EntityOwned<'a> {
    /// Gets raw shared component access for `T`.
    ///
    /// Specially, return `None` if the Entity is not spawned.
    ///
    /// See [`GetComponents`] for examples.
    #[inline]
    pub fn get<T: GetComponents>(&self) -> Option<T::Raw<'_>> {
        let (table, location) = unsafe { ptr::read(&self.storage) }?;
        unsafe { T::get(table, location.table_row) }
    }

    /// Gets change-aware shared component access for `T`.
    ///
    /// See [`GetComponents`] for examples.
    pub fn get_ref<T: GetComponents>(&self) -> Option<T::Ref<'_>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let (table, location) = unsafe { ptr::read(&self.storage) }?;
        unsafe { T::get_ref(table, location.table_row, last_run, this_run) }
    }

    /// Gets change-aware shared component reference for `T`.
    ///
    /// See [`GetComponents`] for examples.
    pub fn into_ref<T: GetComponents>(self) -> Option<T::Ref<'a>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let (table, location) = self.storage?;
        unsafe { T::get_ref(table, location.table_row, last_run, this_run) }
    }
    /// Gets raw shared type-erased pointer by given ComponentId.
    pub fn get_by_id(&self, id: ComponentId) -> Option<Ptr<'_>> {
        let (table, location) = unsafe { ptr::read(&self.storage) }?;
        let col = table.get_table_col(id)?;
        let row = location.table_row;
        Some(unsafe { table.get_data(row, col) })
    }

    /// Gets raw shared type-erased pointer by given Component Type.
    pub fn get_by_type(&self, ty: TypeId) -> Option<Ptr<'_>> {
        let (table, location) = unsafe { ptr::read(&self.storage) }?;
        let col = table.get_type_col(ty)?;
        let row = location.table_row;
        Some(unsafe { table.get_data(row, col) })
    }

    /// Gets type-erased change-aware shared component reference by given ComponentId.
    pub fn get_ref_by_id(&self, id: ComponentId) -> Option<UntypedRef<'_>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let (table, location) = unsafe { ptr::read(&self.storage) }?;
        let col = table.get_table_col(id)?;
        let row = location.table_row;
        Some(unsafe { table.get_ref(row, col, last_run, this_run) })
    }

    /// Gets type-erased change-aware shared component reference by given Component Type.
    pub fn get_ref_by_type(&self, ty: TypeId) -> Option<UntypedRef<'_>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let (table, location) = unsafe { ptr::read(&self.storage) }?;
        let col = table.get_type_col(ty)?;
        let row = location.table_row;
        Some(unsafe { table.get_ref(row, col, last_run, this_run) })
    }
}

// -----------------------------------------------------------------------------
// Common Methods - 2.2 : get_mut + fetch
// -----------------------------------------------------------------------------

impl<'a> EntityMut<'a> {
    /// Gets change-aware mutable component reference for `T`.
    ///
    /// See [`GetComponents`] for examples.
    pub fn get_mut<T: GetComponents>(&mut self) -> Option<T::Mut<'_>> {
        let last_run = self.last_run;
        let this_run = self.this_run;
        let row = self.location.table_row;
        unsafe { T::get_mut(self.table, row, last_run, this_run) }
    }

    /// Gets change-aware mutable component reference for `T`.
    ///
    /// See [`GetComponents`] for examples.
    pub fn into_mut<T: GetComponents>(self) -> Option<T::Mut<'a>> {
        let last_run = self.last_run;
        let this_run = self.this_run;
        let row = self.location.table_row;
        unsafe { T::get_mut(self.table, row, last_run, this_run) }
    }

    /// Gets type-erased change-aware mutable component reference by given ComponentId.
    pub fn get_mut_by_id(&mut self, id: ComponentId) -> Option<UntypedMut<'_>> {
        let col = self.table.get_table_col(id)?;
        let row = self.location.table_row;
        Some(unsafe { self.table.get_mut(row, col, self.last_run, self.this_run) })
    }

    /// Gets type-erased change-aware mutable component reference by given Component Type.
    pub fn get_mut_by_type(&mut self, ty: TypeId) -> Option<UntypedMut<'_>> {
        let col = self.table.get_type_col(ty)?;
        let row = self.location.table_row;
        Some(unsafe { self.table.get_mut(row, col, self.last_run, self.this_run) })
    }

    /// Fetches an arbitrary component reference pattern described by `T`.
    ///
    /// See [`FetchComponents`] for examples.
    pub fn fetch<T: FetchComponents>(&mut self) -> Option<T::Item<'_>> {
        let last_run = self.last_run;
        let this_run = self.this_run;
        let row = self.location.table_row;
        unsafe { T::fetch(true, self.table, row, last_run, this_run) }
    }
}

impl<'a> Entity<'a> {
    /// Gets change-aware mutable component reference for `T`.
    ///
    /// See [`GetComponents`] for examples.
    pub fn get_mut<T: GetComponents>(&mut self) -> Option<T::Mut<'_>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let row = self.location.table_row;
        unsafe { T::get_mut(self.table, row, last_run, this_run) }
    }

    /// Gets change-aware mutable component reference for `T`.
    ///
    /// See [`GetComponents`] for examples.
    pub fn into_mut<T: GetComponents>(self) -> Option<T::Mut<'a>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let row = self.location.table_row;
        unsafe { T::get_mut(self.table, row, last_run, this_run) }
    }

    /// Gets type-erased change-aware mutable component reference by given ComponentId.
    pub fn get_mut_by_id(&mut self, id: ComponentId) -> Option<UntypedMut<'_>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let col = self.table.get_table_col(id)?;
        let row = self.location.table_row;
        Some(unsafe { self.table.get_mut(row, col, last_run, this_run) })
    }

    /// Gets type-erased change-aware mutable component reference by given Component Type.
    pub fn get_mut_by_type(&mut self, ty: TypeId) -> Option<UntypedMut<'_>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let col = self.table.get_type_col(ty)?;
        let row = self.location.table_row;
        Some(unsafe { self.table.get_mut(row, col, last_run, this_run) })
    }

    /// Fetches an arbitrary component reference pattern described by `T`.
    ///
    /// See [`FetchComponents`] for examples.
    pub fn fetch<T: FetchComponents>(&mut self) -> Option<T::Item<'_>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let row = self.location.table_row;
        unsafe { T::fetch(true, self.table, row, last_run, this_run) }
    }
}

impl<'a> EntityOwned<'a> {
    /// Gets change-aware mutable component reference for `T`.
    ///
    /// See [`GetComponents`] for examples.
    pub fn get_mut<T: GetComponents>(&mut self) -> Option<T::Mut<'_>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let (table, location) = unsafe { ptr::read(&self.storage)? };
        unsafe { T::get_mut(table, location.table_row, last_run, this_run) }
    }

    /// Gets change-aware mutable component reference for `T`.
    ///
    /// See [`GetComponents`] for examples.
    pub fn into_mut<T: GetComponents>(self) -> Option<T::Mut<'a>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let (table, location) = self.storage?;
        unsafe { T::get_mut(table, location.table_row, last_run, this_run) }
    }

    /// Gets type-erased change-aware mutable component reference by given ComponentId.
    pub fn get_mut_by_id(&mut self, id: ComponentId) -> Option<UntypedMut<'_>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let (table, location) = unsafe { ptr::read(&self.storage)? };
        let col = table.get_table_col(id)?;
        let row = location.table_row;
        Some(unsafe { table.get_mut(row, col, last_run, this_run) })
    }

    /// Gets type-erased change-aware mutable component reference by given Component Type.
    pub fn get_mut_by_type(&mut self, ty: TypeId) -> Option<UntypedMut<'_>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let (table, location) = unsafe { ptr::read(&self.storage)? };
        let col = table.get_type_col(ty)?;
        let row = location.table_row;
        Some(unsafe { table.get_mut(row, col, last_run, this_run) })
    }

    /// Fetches an arbitrary component reference pattern described by `T`.
    ///
    /// See [`FetchComponents`] for examples.
    pub fn fetch<T: FetchComponents>(&mut self) -> Option<T::Item<'_>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let (table, location) = unsafe { ptr::read(&self.storage)? };
        let row = location.table_row;
        unsafe { T::fetch(true, table, row, last_run, this_run) }
    }
}

// -----------------------------------------------------------------------------
// Archetype
// -----------------------------------------------------------------------------

impl EntityRef<'_> {
    /// Returns the component schema of this table.
    #[inline(always)]
    pub fn components(&self) -> &'static [ComponentId] {
        self.table.components()
    }
}

impl EntityMut<'_> {
    /// Returns the component schema of this table.
    #[inline(always)]
    pub fn components(&self) -> &'static [ComponentId] {
        self.table.components()
    }
}

impl Entity<'_> {
    /// Returns the component schema of this table.
    #[inline(always)]
    pub fn components(&self) -> &'static [ComponentId] {
        self.table.components()
    }

    /// Returns the cached Table that the current entity belongs to.
    #[inline]
    pub fn table(&self) -> &Table {
        self.table
    }
}

impl<'a> EntityOwned<'a> {
    /// Returns the component schema of this table.
    ///
    /// Returns `None` if this entity is despawned.
    #[inline(always)]
    pub fn components(&self) -> Option<&'static [ComponentId]> {
        Some(unsafe { ptr::read(&self.storage)?.0.components() })
    }

    /// Returns the cached Table that the current entity belongs to.
    ///
    /// Returns `None` if this entity is despawned.
    ///
    /// The table is cached; call [`EntityOwned::relocate`] first if you need
    /// to refresh it.
    #[inline]
    pub fn table(&self) -> Option<&Table> {
        Some(unsafe { ptr::read(&self.storage)?.0 })
    }
}

// -----------------------------------------------------------------------------
// World Reference
// -----------------------------------------------------------------------------

impl<'a> Entity<'a> {
    /// Gets read-only access to the world that the current entity belongs to.
    #[inline(always)]
    pub fn world(&self) -> &World {
        unsafe { self.world.read_only() }
    }

    /// Returns this entity's [`World`], consuming itself.
    ///
    /// This is read-only, because the `Entity` cannot modify world structure.
    #[inline(always)]
    pub fn into_world(self) -> &'a World {
        unsafe { self.world.full_mut() }
    }

    /// Gets a restricted mutable world handle for deferred mutation workflows.
    #[inline(always)]
    pub fn deferred(&mut self) -> DeferredWorld<'_> {
        unsafe { self.world.deferred() }
    }

    /// Gets a restricted mutable world handle for deferred mutation workflows.
    #[inline(always)]
    pub fn into_deferred(self) -> DeferredWorld<'a> {
        unsafe { self.world.deferred() }
    }
}

impl<'a> EntityOwned<'a> {
    /// Gets read-only access to the world that the current entity belongs to.
    #[inline(always)]
    pub fn world(&self) -> &World {
        unsafe { self.world.read_only() }
    }

    /// Returns this entity's [`World`], consuming itself.
    ///
    /// Unlike the [`Entity`] variant, this returns a mutable world reference:
    /// the owning handle gives up its entity-specific access and hands the
    /// whole world back.
    #[inline(always)]
    pub fn into_world(self) -> &'a mut World {
        unsafe { self.world.full_mut() }
    }

    /// Gets a restricted mutable world handle for deferred mutation workflows.
    #[inline(always)]
    pub fn deferred(&mut self) -> DeferredWorld<'_> {
        unsafe { self.world.deferred() }
    }

    /// Gets a restricted mutable world handle for deferred mutation workflows.
    #[inline(always)]
    pub fn into_deferred(self) -> DeferredWorld<'a> {
        unsafe { self.world.deferred() }
    }

    /// Gives mutable access to this entity's [`World`] in a temporary scope.
    #[inline]
    pub fn world_scope<R>(&mut self, func: impl FnOnce(&mut World) -> R) -> R {
        let unsafe_world = self.world;
        let _guard = RelocateGuard(self);
        func(unsafe { unsafe_world.data_mut() })
    }
}

// -----------------------------------------------------------------------------
// Resource
// -----------------------------------------------------------------------------

macro_rules! impl_common_resource {
    ($name:ident) => {
        impl<'a> $name<'a> {
            /// Gets a reference to the resource of the given type
            ///
            /// # Panics
            ///
            /// Panics if the resource does not exist.
            /// Use `get_resource` instead if you want to handle this case.
            #[inline]
            #[track_caller]
            pub fn resource<R: Resource + Sync>(&self) -> &R {
                unsafe { self.world.read_only().resource::<R>() }
            }

            /// Gets a reference with change detections to the resource of the given type
            ///
            /// # Panics
            ///
            /// Panics if the resource does not exist.
            /// Use `get_resource_ref` instead if you want to handle this case.
            #[inline]
            #[track_caller]
            pub fn resource_ref<R: Resource + Sync>(&self) -> Res<'_, R> {
                unsafe { self.world.read_only().resource_ref::<R>() }
            }

            /// Gets a mutable reference with change detections to the resource of the given type
            ///
            /// # Panics
            ///
            /// Panics if the resource does not exist.
            /// Use `get_resource_mut` instead if you want to handle this case.
            #[inline]
            #[track_caller]
            pub fn resource_mut<R: Resource + Send>(&mut self) -> ResMut<'_, R> {
                unsafe { self.world.data_mut().resource_mut::<R>() }
            }

            /// Gets a reference to the resource of the given type if it exists
            #[inline]
            pub fn get_resource<R: Resource + Sync>(&self) -> Option<&R> {
                unsafe { self.world.read_only().get_resource() }
            }

            /// Gets a reference with change detections to the resource of the given type if it exists
            #[inline]
            pub fn get_resource_ref<R: Resource + Sync>(&self) -> Option<Res<'_, R>> {
                unsafe { self.world.read_only().get_resource_ref() }
            }

            /// Gets a mutable reference with change detections to the resource of the given type if it exists
            #[inline]
            pub fn get_resource_mut<R: Resource + Send>(&mut self) -> Option<ResMut<'_, R>> {
                unsafe { self.world.data_mut().get_resource_mut() }
            }
        }
    };
}

impl_common_resource!(Entity);
impl_common_resource!(EntityOwned);

// -----------------------------------------------------------------------------
// Hierarchy Spawn
// -----------------------------------------------------------------------------

impl<'a> Entity<'a> {
    /// Returns the parent entity's ID, if any.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    ///
    /// let root = world.spawn((), None);
    /// let root_id = root.id();
    /// drop(root); // release the world borrow held by the handle
    ///
    /// assert!(world.entity(root_id).parent().is_none());
    ///
    /// let a = world.spawn((), Some(root_id));
    /// let a_id = a.id();
    /// drop(a);
    ///
    /// assert_eq!(world.entity(a_id).parent(), Some(root_id));
    /// ```
    #[inline]
    pub fn parent(&self) -> Option<EntityId> {
        self.node.parent
    }

    /// Returns the IDs of this entity's direct children.
    ///
    /// Children are ordered by insertion.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let root_id = world.spawn((), None).id();
    /// let a = world.spawn((), Some(root_id)).id();
    /// let b = world.spawn((), Some(root_id)).id();
    ///
    /// let root = world.entity_owned(root_id).into_view();
    ///
    /// let ids: &[EntityId] = root.children();
    /// assert_eq!(ids, &[a, b]);
    /// ```
    #[inline]
    pub fn children(&self) -> &'_ [EntityId] {
        &self.node.children
    }

    /// Returns a view of the parent entity, if it exists.
    ///
    /// This reborrows `self` — the parent view coexists with the current
    /// view.
    pub fn as_parent(&mut self) -> Option<Entity<'_>> {
        let id = self.node.parent?;
        let world = self.world;
        Some(Self::from_id(world, id))
    }

    /// Returns an iterator over views of this entity's direct children.
    ///
    /// This reborrows `self` — the child views coexist with the parent
    /// view.
    pub fn as_children(&mut self) -> impl ExactSizeIterator<Item = Entity<'_>> + '_ {
        let world = self.world;
        self.node
            .children
            .iter()
            .map(move |&id| Self::from_id(world, id))
    }

    /// Consumes `self` and returns a view of the parent entity, if it
    /// exists.
    pub fn into_parent(self) -> Option<Entity<'a>> {
        let id = self.node.parent?;
        Some(Self::from_id(self.world, id))
    }

    /// Consumes `self` and returns an iterator over views of this entity's
    /// direct children.
    pub fn into_children(self) -> impl ExactSizeIterator<Item = Entity<'a>> + 'a {
        let world = self.world;
        self.node
            .children
            .iter()
            .map(move |&id| Self::from_id(world, id))
    }

    /// Returns a view of the direct child at `index`, if it exists.
    ///
    /// Children are ordered by insertion, so `index` is the position in
    /// that order.  Returns `None` when `index` is out of bounds.
    ///
    /// This reborrows `self` — the child view coexists with the current
    /// view.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let root_id = world.spawn((), None).id();
    /// let first = world.spawn((), Some(root_id)).id();
    /// let second = world.spawn((), Some(root_id)).id();
    ///
    /// let mut root = world.entity_owned(root_id);
    /// let mut view = root.as_view();
    /// assert_eq!(view.get_child(0).unwrap().id(), first);
    /// assert_eq!(view.get_child(1).unwrap().id(), second);
    /// assert!(view.get_child(2).is_none());
    /// ```
    pub fn get_child(&mut self, index: usize) -> Option<Entity<'_>> {
        let world = self.world;
        let id = *self.node.children.get(index)?;
        Some(Self::from_id(world, id))
    }

    /// Builds an entity view for a spawned entity ID in `world`.
    ///
    /// # Panics
    ///
    /// Panics if `id` is not a spawned entity.
    #[inline]
    fn from_id(world: WorldCell<'a>, id: EntityId) -> Entity<'a> {
        let w = unsafe { world.data_mut() };
        let node = w.entities.get(id).unwrap();
        let location = unsafe { node.location.unwrap_unchecked() };
        let table_id = location.table_id;
        let table = unsafe { w.tables.get_unchecked_mut(table_id) };
        Entity {
            id,
            world,
            node,
            table,
            location,
        }
    }
}

impl<'w> EntityOwned<'w> {
    /// Returns the parent entity's ID, if any.
    ///
    /// Return `Err` is the entity is despawned.
    #[inline]
    pub fn parent(&self) -> Result<Option<EntityId>, EntityError> {
        let world = self.world;
        let w = unsafe { world.read_only() };
        let info = w.entities.get(self.id)?;
        Ok(info.parent)
    }

    /// Returns the IDs of this entity's direct children.
    ///
    /// Children are ordered by insertion.
    ///
    /// Return `Err` is the entity is despawned.
    #[inline]
    pub fn children(&self) -> Result<&[EntityId], EntityError> {
        let world = self.world;
        let w = unsafe { world.read_only() };
        let info = w.entities.get(self.id)?;
        Ok(&info.children)
    }

    /// Returns a view of the direct child at `index`, if it exists.
    ///
    /// Children are ordered by insertion, so `index` is the position in
    /// that order.  Returns `None` when this entity is despawned or
    /// `index` is out of bounds.
    pub fn get_child(&mut self, index: usize) -> Option<Entity<'_>> {
        self.validate().ok()?;
        let this = self.id;
        let world = self.world;
        let w = unsafe { world.data_mut() };
        let node = w.entities.get(this).ok()?;
        let child = *node.children.get(index)?;
        let cnode = w.entities.get(child).ok()?;
        let location = unsafe { cnode.location.unwrap_unchecked() };
        let table_id = location.table_id;
        let table = unsafe { w.tables.get_unchecked_mut(table_id) };
        Some(Entity {
            id: child,
            world,
            node: cnode,
            table,
            location,
        })
    }

    /// Changes the parent of this entity.
    ///
    /// Pass `None` to detach this entity from its current parent (making
    /// it a root entity).  Pass `Some(id)` to make `id` the new parent.
    ///
    /// Returns `Err` if this entity is despawned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let a = world.spawn((), None).id();
    /// let b = world.spawn((), None).id();
    /// let mut child = world.spawn((), Some(a));
    ///
    /// // Move the child under a new parent.
    /// child.reparent(Some(b)).unwrap();
    /// assert_eq!(child.as_view().parent(), Some(b));
    ///
    /// // Detach it again, making it a root entity.
    /// child.reparent(None).unwrap();
    /// assert_eq!(child.as_view().parent(), None);
    /// ```
    #[inline]
    pub fn reparent(&mut self, parent: Option<EntityId>) -> Result<&mut Self, EntityError> {
        self.validate()?;
        let this = self.id();
        let world = unsafe { self.world.full_mut() };
        let guard = RelocateGuard(self);
        world.entities.modify_parent(this, parent)?;
        ::core::mem::drop(guard); // drop, not forget
        Ok(self)
    }

    /// Spawns a child entity as a direct child of this entity.
    ///
    /// The spawned entity inherits the parent's lifecycle — when the
    /// parent is despawned, all descendants are recursively despawned.
    ///
    /// Returns `Err` if this entity is despawned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let mut parent = world.spawn((), None);
    ///
    /// parent.with_child(()).unwrap();
    /// assert_eq!(parent.children().unwrap().len(), 1);
    /// ```
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn with_child(&mut self, bundle: impl Bundle) -> Result<&mut Self, EntityError> {
        self.validate()?;
        let caller = DebugLocation::caller();
        let this = self.id();
        let world = unsafe { self.world.full_mut() };
        let guard = RelocateGuard(self);
        world.spawn_with_caller(bundle, Some(this), caller);
        ::core::mem::drop(guard); // drop, not forget
        Ok(self)
    }

    /// Spawns multiple child entities as direct children of this entity.
    ///
    /// Each item in the iterator produces one child.  All children inherit
    /// the parent's lifecycle.
    ///
    /// Returns `Err` if this entity is despawned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let mut parent = world.spawn((), None);
    ///
    /// // Batch-spawn several children at once.
    /// parent.with_children([(), ()]).unwrap();
    /// assert_eq!(parent.children().unwrap().len(), 2);
    /// ```
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn with_children<B, I>(&mut self, iter: I) -> Result<&mut Self, EntityError>
    where
        B: DataBundle,
        I: IntoIterator<Item = B>,
    {
        self.validate()?;
        let caller = DebugLocation::caller();
        let this = self.id();
        let world = unsafe { self.world.full_mut() };
        let guard = RelocateGuard(self);
        world.spawn_batch_with_caller(iter, Some(this), caller);
        ::core::mem::drop(guard); // drop, not forget
        Ok(self)
    }

    /// Despawns all direct children of this entity.
    ///
    /// The despawn is recursive — each child's own children are also
    /// despawned.
    ///
    /// Returns `Err` if this entity is despawned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let mut parent = world.spawn((), None);
    /// parent.with_children([(), ()]).unwrap();
    /// assert_eq!(parent.children().unwrap().len(), 2);
    ///
    /// parent.despawn_children().unwrap();
    /// assert_eq!(parent.children().unwrap().len(), 0);
    /// ```
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn despawn_children(&mut self) -> Result<&mut Self, EntityError> {
        self.validate()?;
        let caller = DebugLocation::caller();
        let this = self.id();
        let world = unsafe { self.world.full_mut() };
        let guard = RelocateGuard(self);
        let x = world.entities.get(this).unwrap();
        let children: Vec<EntityId> = x.children.to_vec();

        for child in children {
            world.try_despawn_with_caller(child, caller);
        }

        ::core::mem::drop(guard); // drop, not forget
        Ok(self)
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;

    #[test]
    fn hierarchy_child_access() {
        let mut world = World::alloc();
        let root = world.spawn((), None).id();
        let a = world.spawn((), Some(root)).id();
        let b = world.spawn((), Some(root)).id();
        let c = world.spawn((), Some(root)).id();

        // EntityOwned view: child count + ordered get_child.
        let mut owned = world.entity_owned(root);
        assert_eq!(owned.children().unwrap().len(), 3);
        assert_eq!(owned.get_child(0).unwrap().id(), a);
        assert_eq!(owned.get_child(1).unwrap().id(), b);
        assert_eq!(owned.get_child(2).unwrap().id(), c);
        assert!(owned.get_child(3).is_none());

        // Entity view: same APIs.
        let mut view = owned.as_view();
        assert_eq!(view.children().len(), 3);
        assert_eq!(view.get_child(1).unwrap().id(), b);
        assert!(view.get_child(3).is_none());

        // Children are ordered by insertion.
        let ids: &[EntityId] = view.children();
        assert_eq!(ids, &[a, b, c]);

        // Parent links.
        assert_eq!(view.get_child(0).unwrap().parent(), Some(root));
        assert_eq!(owned.children().unwrap().len(), 3); // view reborrow ended
    }

    #[test]
    fn hierarchy_reparent_keeps_insertion_order() {
        let mut world = World::alloc();
        let root = world.spawn((), None).id();
        let other = world.spawn((), None).id();
        let a = world.spawn((), Some(root)).id();
        let b = world.spawn((), Some(root)).id();
        let c = world.spawn((), Some(root)).id();

        // Move `a` under another parent, then back — it is appended, so the
        // insertion order becomes `[b, c, a]`.
        world.entity_owned(a).reparent(Some(other)).unwrap();
        world.entity_owned(a).reparent(Some(root)).unwrap();

        {
            let mut owned = world.entity_owned(root);
            let view = owned.as_view();
            let ids: &[EntityId] = view.children();
            assert_eq!(ids, &[b, c, a]);
            assert_eq!(owned.get_child(2).unwrap().id(), a);
            assert_eq!(owned.children().unwrap().len(), 3);
        }

        // Detach `b` — it becomes a root.
        world.entity_owned(b).reparent(None).unwrap();

        {
            let mut owned = world.entity_owned(root);
            assert_eq!(owned.children().unwrap().len(), 2);
            let view = owned.as_view();
            let ids: &[EntityId] = view.children();
            assert_eq!(ids, &[c, a]);
            assert!(owned.get_child(2).is_none());
        }
    }
}
