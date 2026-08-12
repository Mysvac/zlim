// -----------------------------------------------------------------------------
// Modules
// -----------------------------------------------------------------------------

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

use crate::borrow::ResMut;
use crate::borrow::UntypedMut;
use crate::borrow::UntypedRef;
use crate::borrow::{NonSend, NonSendMut, Res};
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
// Entity & EntityRef & EntityMut & EntityOwend
// -----------------------------------------------------------------------------

/// 一个仅数据可变的视图，可以访问层级关系，结构不可变。
pub struct Entity<'w> {
    pub(crate) id: EntityId,
    pub(crate) world: WorldCell<'w>,
    pub(crate) node: &'w EntityNode,
    pub(crate) table: &'w mut Table,
    pub(crate) location: Location,
}

/// 一个不可变视图，只能访问自身的组件数据。
pub struct EntityRef<'w> {
    pub(crate) id: EntityId,
    pub(crate) table: &'w Table,
    pub(crate) location: Location,
    pub(crate) last_run: Tick,
    pub(crate) this_run: Tick,
}

/// 一个可变视图，但只能访问自身的组件数据。
pub struct EntityMut<'w> {
    pub(crate) id: EntityId,
    pub(crate) table: &'w mut Table,
    pub(crate) location: Location,
    pub(crate) last_run: Tick,
    pub(crate) this_run: Tick,
}

/// 一个代表所有权的视图，可以访问组件，以及层级关系，可以修改结构。
///
/// 同时，它可能指向不存在的实现，此时某些操作会返回失败，而转换会 Panic。
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
    #[inline(always)]
    pub fn reborrow(&mut self) -> Entity<'_> {
        // SAFETY: no need drop
        unsafe { ptr::read(self) }
    }
}

impl EntityRef<'_> {
    #[inline(always)]
    pub fn reborrow(&self) -> EntityRef<'_> {
        // SAFETY: no need drop
        unsafe { ptr::read(self) }
    }
}

impl EntityMut<'_> {
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
    #[inline(always)]
    pub fn validate(&self) -> Result<(), EntityError> {
        if self.storage.is_none() {
            Err(EntityError::NotSpawned(self.id))
        } else {
            Ok(())
        }
    }

    /// Return `true` if the entity is spawned.
    ///
    /// Note that this function check cached [`Location`] directly,
    /// if you want to update it, call [`EntityOwned::relocate`] before
    /// this function.
    #[inline(always)]
    pub fn is_spawned(&self) -> bool {
        self.storage.is_some()
    }

    /// Return `true` if the entity is despawned.
    ///
    /// Note that this function check cached [`Location`] directly,
    /// if you want to update it, call [`EntityOwned::relocate`] before
    /// this function.
    #[inline(always)]
    pub fn is_despawned(&self) -> bool {
        self.storage.is_none()
    }

    /// Updates the internal entity location to match the current location
    /// in the internal [`World`].
    ///
    /// This is *only* required when using the unsafe function [`EntityOwned::world_cell`],
    /// which enables the location to change.
    ///
    /// Note that if the entity is not spawned for any reason, this will have a location of
    /// `None`, leading some methods to panic.
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
    /// Consumes `self` and returns mutable view, with the world `'w` lifetime.
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
    #[inline]
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
    #[inline]
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
    #[inline]
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
    #[inline]
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
    #[inline]
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
    /// Return `None` if this entity is despawned
    #[inline(always)]
    pub fn components(&self) -> Option<&'static [ComponentId]> {
        Some(unsafe { ptr::read(&self.storage)?.0.components() })
    }

    /// Returns the cached Table that the current entity belongs to.
    ///
    /// Return `None` if this entity is despawned
    ///
    /// if you want to update it, call [`EntityOwned::relocate`] before
    /// this function.
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

    /// Returns this entity's [`World`], consuming itself.
    ///
    /// This is readonly, because the `Entity` cannot modify world structure.
    #[inline(always)]
    pub fn into_world(self) -> &'a World {
        unsafe { self.world.full_mut() }
    }
}

impl<'a> EntityOwned<'a> {
    /// Gets read-only access to the world that the current entity belongs to.
    #[inline(always)]
    pub fn world(&self) -> &World {
        unsafe { self.world.read_only() }
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

    /// Returns this entity's [`World`], consuming itself.
    #[inline(always)]
    pub fn into_world(self) -> &'a mut World {
        unsafe { self.world.full_mut() }
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

impl<'a> EntityOwned<'a> {
    /// Gets a reference to the resource of the given type
    ///
    /// # Panics
    ///
    /// Panics if the resource does not exist.
    /// Use `get_non_send` instead if you want to handle this case.
    #[inline]
    #[track_caller]
    pub fn non_send<R: Resource>(&self) -> &R {
        unsafe { self.world.read_only().non_send::<R>() }
    }

    /// Gets a reference with change detections to the resource of the given type
    ///
    /// # Panics
    ///
    /// Panics if the resource does not exist.
    /// Use `get_non_send_ref` instead if you want to handle this case.
    #[inline]
    #[track_caller]
    pub fn non_send_ref<R: Resource>(&self) -> NonSend<'_, R> {
        unsafe { self.world.read_only().non_send_ref::<R>() }
    }

    /// Gets a mutable reference with change detections to the resource of the given type
    ///
    /// # Panics
    ///
    /// Panics if the resource does not exist.
    /// Use `get_non_send_mut` instead if you want to handle this case.
    #[inline]
    #[track_caller]
    pub fn non_send_mut<R: Resource>(&mut self) -> NonSendMut<'_, R> {
        unsafe { self.world.data_mut().non_send_mut::<R>() }
    }

    /// Gets a reference to the resource of the given type if it exists
    #[inline]
    pub fn get_non_send<R: Resource>(&self) -> Option<&R> {
        unsafe { self.world.read_only().get_non_send() }
    }

    /// Gets a reference with change detections to the resource of the given type if it exists
    #[inline]
    pub fn get_non_send_ref<R: Resource>(&self) -> Option<NonSend<'_, R>> {
        unsafe { self.world.read_only().get_non_send_ref() }
    }

    /// Gets a mutable reference with change detections to the resource of the given type if it exists
    #[inline]
    pub fn get_non_send_mut<R: Resource>(&mut self) -> Option<NonSendMut<'_, R>> {
        unsafe { self.world.data_mut().get_non_send_mut() }
    }
}

// -----------------------------------------------------------------------------
// Hierachy Spawn
// -----------------------------------------------------------------------------

impl<'a> Entity<'a> {
    #[inline]
    pub fn parent(&self) -> Option<EntityId> {
        self.node.child_of
    }

    #[inline]
    pub fn children(&self) -> impl ExactSizeIterator<Item = EntityId> + '_ {
        self.node.children.iter().copied()
    }

    pub fn as_parent(&mut self) -> Option<Entity<'_>> {
        let id = self.node.child_of?;
        let world = self.world;
        let w = unsafe { world.data_mut() };
        let node = w.entities.get(id).unwrap();
        let location = unsafe { node.location.unwrap_unchecked() };
        let table_id = location.table_id;
        let table = unsafe { w.tables.get_unchecked_mut(table_id) };
        Some(Entity {
            id,
            world,
            node,
            table,
            location,
        })
    }

    pub fn into_parent(self) -> Option<Entity<'a>> {
        let id = self.node.child_of?;
        let world = self.world;
        let w = unsafe { world.data_mut() };
        let node = w.entities.get(id).unwrap();
        let location = unsafe { node.location.unwrap_unchecked() };
        let table_id = location.table_id;
        let table = unsafe { w.tables.get_unchecked_mut(table_id) };
        Some(Entity {
            id,
            world,
            node,
            table,
            location,
        })
    }

    pub fn as_children(&mut self) -> impl ExactSizeIterator<Item = Entity<'_>> + '_ {
        let world = self.world;
        self.node.children.iter().map(move |&id| {
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
        })
    }

    pub fn into_children(self) -> impl ExactSizeIterator<Item = Entity<'a>> + 'a {
        let world = self.world;
        self.node.children.iter().map(move |&id| {
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
        })
    }
}

impl<'w> EntityOwned<'w> {
    #[inline]
    pub fn modify_parent(&mut self, child_of: Option<EntityId>) -> Result<&mut Self, EntityError> {
        self.validate()?;
        let this = self.id();
        let world = unsafe { self.world.full_mut() };
        let guard = RelocateGuard(self);
        world.entities.modify_child_of(this, child_of)?;
        ::core::mem::drop(guard); // drop, not forget
        Ok(self)
    }

    /// Spawns a child entity related to this entity with [`ChildOf`].
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

    /// Spawns child entities related to this entity with [`ChildOf`]
    /// by running a builder closure against a [`RelatedSpawner`].
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

    /// Despawns all children of this entity.
    ///
    /// This removes child entities entirely (and recursively, because
    /// `Children` enables linked lifecycle).
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn despawn_children(&mut self) -> Result<&mut Self, EntityError> {
        self.validate()?;
        let caller = DebugLocation::caller();
        let this = self.id();
        let world = unsafe { self.world.full_mut() };
        let guard = RelocateGuard(self);
        let x = world.entities.get(this).unwrap();
        let children: Vec<EntityId> = x.children.iter().copied().collect();

        for child in children {
            world.try_despawn_with_caller(child, caller);
        }

        ::core::mem::drop(guard); // drop, not forget
        Ok(self)
    }
}
