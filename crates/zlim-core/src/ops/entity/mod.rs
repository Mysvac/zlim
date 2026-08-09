// -----------------------------------------------------------------------------
// Modules
// -----------------------------------------------------------------------------

mod fetch_trait;
mod get_trait;

pub use fetch_trait::FetchComponents;
pub use get_trait::GetComponents;

// -----------------------------------------------------------------------------
// Inline Content
// -----------------------------------------------------------------------------

use core::any::TypeId;
use core::fmt::{Debug, Formatter};

use zlim_ptr::Ptr;

use crate::borrow::Res;
use crate::borrow::ResMut;
use crate::borrow::UntypedMut;
use crate::borrow::UntypedRef;
use crate::component::ComponentId;
use crate::entity::EntityId;
use crate::entity::EntityNode;
use crate::entity::Location;
use crate::resource::Resource;
use crate::table::Table;
use crate::tick::Tick;
use crate::utils::DebugLocation;
use crate::world::World;
use crate::world::WorldCell;

// -----------------------------------------------------------------------------
// EntityRef & EntityMut
// -----------------------------------------------------------------------------

pub struct EntityRef<'w> {
    pub(crate) id: EntityId,
    pub(crate) table: &'w Table,
    pub(crate) location: Location,
    pub(crate) last_run: Tick,
    pub(crate) this_run: Tick,
}

pub struct EntityMut<'w> {
    pub(crate) id: EntityId,
    pub(crate) table: &'w mut Table,
    pub(crate) location: Location,
    pub(crate) last_run: Tick,
    pub(crate) this_run: Tick,
}

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

macro_rules! impl_common_methods {
    ($name:ident) => {
        impl Debug for $name<'_> {
            fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
                f.debug_struct(stringify!($name))
                    .field("id", &self.id)
                    .field("location", &self.location)
                    .finish()
            }
        }

        impl<'a> $name<'a> {
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

            /// Returns this entity's Table.
            #[inline(always)]
            pub fn table(&self) -> &Table {
                self.table
            }

            /// Returns whether the entity's archetype contains `T`.
            ///
            /// See [`GetComponents`] for examples.
            #[inline(always)]
            pub fn contains<T: GetComponents>(&self) -> bool {
                T::contains(self.table)
            }

            /// Gets change-aware shared component reference for `T`.
            ///
            /// See [`GetComponents`] for examples.
            #[inline]
            pub fn get_ref<T: GetComponents>(&self) -> Option<T::Ref<'_>> {
                unsafe {
                    T::get_ref(
                        self.table,
                        self.location.table_row,
                        self.last_run,
                        self.this_run,
                    )
                }
            }

            /// Gets change-aware shared component reference for `T`.
            ///
            /// See [`GetComponents`] for examples.
            #[inline]
            pub fn into_ref<T: GetComponents>(self) -> Option<T::Ref<'a>> {
                unsafe {
                    T::get_ref(
                        self.table,
                        self.location.table_row,
                        self.last_run,
                        self.this_run,
                    )
                }
            }

            /// Checks whether the entity contains given Component Type.
            pub fn contains_by_type(&self, ty: TypeId) -> bool {
                self.table.contains_type(ty)
            }

            /// Checks whether the entity contains given Component(Id).
            pub fn contains_by_id(&self, id: ComponentId) -> bool {
                self.table.contains_component(id)
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
    /// Reborrow self as same lifetime.
    #[inline]
    pub fn reborrow(&self) -> EntityRef<'a> {
        EntityRef {
            id: self.id,
            table: self.table,
            location: self.location,
            last_run: self.last_run,
            this_run: self.this_run,
        }
    }

    /// Gets raw shared component reference for `T`.
    ///
    /// See [`GetComponents`] for examples.
    #[inline]
    pub fn get<T: GetComponents>(&self) -> Option<T::Raw<'a>> {
        unsafe { T::get(self.table, self.location.table_row) }
    }
}

impl<'a> EntityMut<'a> {
    /// Reborrow self as samller lifetime.
    #[inline]
    pub fn reborrow(&mut self) -> EntityMut<'_> {
        EntityMut {
            id: self.id,
            table: self.table,
            location: self.location,
            last_run: self.last_run,
            this_run: self.this_run,
        }
    }

    /// Gets raw shared component reference for `T`.
    ///
    /// See [`GetComponents`] for examples.
    #[inline]
    pub fn get<T: GetComponents>(&self) -> Option<T::Raw<'_>> {
        unsafe { T::get(self.table, self.location.table_row) }
    }

    /// Gets change-aware mutable component reference for `T`.
    ///
    /// See [`GetComponents`] for examples.
    #[inline]
    pub fn get_mut<T: GetComponents>(&mut self) -> Option<T::Mut<'_>> {
        unsafe {
            T::get_mut(
                self.table,
                self.location.table_row,
                self.last_run,
                self.this_run,
            )
        }
    }

    /// Gets change-aware mutable component reference for `T`.
    ///
    /// See [`GetComponents`] for examples.
    #[inline]
    pub fn into_mut<T: GetComponents>(self) -> Option<T::Mut<'a>> {
        unsafe {
            T::get_mut(
                self.table,
                self.location.table_row,
                self.last_run,
                self.this_run,
            )
        }
    }

    /// Fetches an arbitrary component reference pattern described by `T`.
    ///
    /// See [`FetchComponents`] for examples.
    #[inline]
    pub fn fetch<T: FetchComponents>(&mut self) -> Option<T::Item<'_>> {
        unsafe {
            T::fetch(
                true,
                self.table,
                self.location.table_row,
                self.last_run,
                self.this_run,
            )
        }
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
}

// -----------------------------------------------------------------------------
// EntityOwned
// -----------------------------------------------------------------------------

pub struct EntityOwned<'w> {
    pub(crate) id: EntityId,
    pub(crate) world: WorldCell<'w>,
    pub(crate) storage: Option<(&'w mut Table, Location)>,
}

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

impl<'a> EntityOwned<'a> {
    #[cold]
    #[inline(never)]
    fn panic_despawned(&self, caller: DebugLocation) -> ! {
        let world = unsafe { self.world.read_only() };
        let id = self.id;
        let info = world.entities.locate(self.id).unwrap_err();
        panic!("`EntityOwned` try operate a despawned Entity({id}): {info}, {caller}.");
    }

    #[inline(always)]
    pub(crate) fn assert_is_spawned(&self, caller: DebugLocation) {
        if self.storage.is_none() {
            self.panic_despawned(caller)
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

impl<'a> From<EntityOwned<'a>> for EntityMut<'a> {
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    fn from(value: EntityOwned<'a>) -> Self {
        let caller = DebugLocation::caller();
        value.assert_is_spawned(caller);

        let id = value.id;
        let last_run = value.last_run();
        let this_run = value.this_run();
        let (table, location) = unsafe { value.storage.unwrap_unchecked() };
        EntityMut {
            id,
            table,
            location,
            last_run,
            this_run,
        }
    }
}

impl<'a> From<EntityOwned<'a>> for EntityRef<'a> {
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    fn from(value: EntityOwned<'a>) -> Self {
        let caller = DebugLocation::caller();
        value.assert_is_spawned(caller);

        let id = value.id;
        let last_run = value.last_run();
        let this_run = value.this_run();
        let (table, location) = unsafe { value.storage.unwrap_unchecked() };
        EntityRef {
            id,
            table,
            location,
            last_run,
            this_run,
        }
    }
}

impl<'a> EntityOwned<'a> {
    // -------------------------------------------------------------------------
    // View conversions

    /// Returns the underlying entity id.
    #[inline(always)]
    pub fn id(&self) -> EntityId {
        self.id
    }

    /// Consumes `self` and returns read-only access to all of the entity's
    /// components, with the world `'w` lifetime.
    ///
    /// # Panics
    /// Panics if `self` is despawned.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn into_readonly(self) -> EntityRef<'a> {
        EntityRef::from(self)
    }

    /// Consumes `self` and returns non-structural mutable access to all of the
    /// entity's components, with the world `'w` lifetime.
    ///
    /// # Panics
    /// Panics if `self` is despawned.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn into_mutable(self) -> EntityMut<'a> {
        EntityMut::from(self)
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

        let id = self.id;
        let last_run = self.last_run();
        let this_run = self.this_run();
        let (table, location) = unsafe { self.storage.as_ref().unwrap_unchecked() };
        EntityRef {
            id,
            table,
            location: *location,
            last_run,
            this_run,
        }
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

        let id = self.id;
        let last_run = self.last_run();
        let this_run = self.this_run();
        let (table, location) = unsafe { self.storage.as_mut().unwrap_unchecked() };
        EntityMut {
            id,
            table,
            location: *location,
            last_run,
            this_run,
        }
    }

    // -------------------------------------------------------------------------
    // Component access

    /// Returns whether the entity's archetype contains `T`.
    ///
    /// Specially, return `false` if the Entity is not spawned.
    ///
    /// See [`GetComponents`] for examples.
    pub fn contains<T: GetComponents>(&self) -> bool {
        if let Some((table, _)) = self.storage.as_ref() {
            T::contains(table)
        } else {
            false
        }
    }

    /// Gets raw shared component access for `T`.
    ///
    /// Specially, return `None` if the Entity is not spawned.
    ///
    /// See [`GetComponents`] for examples.
    pub fn get<T: GetComponents>(&self) -> Option<T::Raw<'_>> {
        let (table, location) = self.storage.as_ref()?;
        unsafe { T::get(table, location.table_row) }
    }

    /// Gets change-aware shared component access for `T`.
    ///
    /// See [`GetComponents`] for examples.
    pub fn get_ref<T: GetComponents>(&self) -> Option<T::Ref<'_>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let (table, location) = self.storage.as_ref()?;
        unsafe { T::get_ref(table, location.table_row, last_run, this_run) }
    }

    /// Gets change-aware shared component access for `T`.
    ///
    /// See [`GetComponents`] for examples.
    pub fn get_mut<T: GetComponents>(&mut self) -> Option<T::Mut<'_>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let (table, location) = self.storage.as_mut()?;
        unsafe { T::get_mut(table, location.table_row, last_run, this_run) }
    }

    /// Fetches an arbitrary component access pattern described by `T`.
    ///
    /// Specially, return `None` if the Entity is not spawned.
    ///
    /// See [`FetchComponents`] for examples.
    pub fn fetch<T: FetchComponents>(&mut self) -> Option<T::Item<'_>> {
        let last_run = self.last_run();
        let this_run = self.this_run();
        let (table, location) = self.storage.as_mut()?;
        unsafe { T::fetch(true, table, location.table_row, last_run, this_run) }
    }

    // -------------------------------------------------------------------------
    // State inspection
    /// Return `true` if the entity is spawned.
    ///
    /// Note that this function check cached [`Location`] directly,
    /// if you want to update it, call [`Entity::relocate`] before
    /// this function.
    #[inline]
    pub fn is_spawned(&self) -> bool {
        self.storage.is_some()
    }

    /// Return `true` if the entity is despawned.
    ///
    /// Note that this function check cached [`Location`] directly,
    /// if you want to update it, call [`Entity::relocate`] before
    /// this function.
    #[inline]
    pub fn is_despawned(&self) -> bool {
        self.storage.is_none()
    }

    /// Return the cached [`Location`].
    ///
    /// if you want to update it, call [`Entity::relocate`] before
    /// this function.
    #[inline]
    pub fn try_location(&self) -> Option<Location> {
        Some(self.storage.as_ref()?.1)
    }

    /// Returns the cached Table that the current entity belongs to.
    ///
    /// if you want to update it, call [`Entity::relocate`] before
    /// this function.
    #[inline]
    pub fn try_table(&self) -> Option<&Table> {
        Some(self.storage.as_ref()?.0)
    }

    #[inline]
    pub fn try_node(&self) -> Option<&EntityNode> {
        let x = unsafe { self.world.read_only() };
        x.entities.get(self.id).ok()
    }

    /// Return the cached [`EntityLocation`].
    ///
    /// if you want to update it, call [`EntityOwned::relocate`] before
    /// this function.
    ///
    /// # Panics
    /// If the entity has been despawned while this `EntityOwned` is still alive.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn location(&self) -> Location {
        match &self.storage {
            Some((_, y)) => *y,
            None => self.panic_despawned(DebugLocation::caller()),
        }
    }

    /// Returns the cached archetype that the current entity belongs to.
    ///
    /// if you want to update it, call [`EntityOwned::relocate`] before
    /// this function.
    ///
    /// # Panics
    /// If the entity has been despawned while this `EntityOwned` is still alive.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn table(&self) -> &Table {
        match &self.storage {
            Some((x, _)) => x,
            None => self.panic_despawned(DebugLocation::caller()),
        }
    }

    #[inline]
    pub fn node(&self) -> &EntityNode {
        let x = unsafe { self.world.read_only() };
        match x.entities.get(self.id) {
            Ok(x) => x,
            Err(_) => self.panic_despawned(DebugLocation::caller()),
        }
    }

    #[inline]
    pub fn relocate(&mut self) {
        let world = unsafe { self.world.data_mut() };
        match world.entities.locate(self.id) {
            Err(_) => self.storage = None,
            Ok(location) => {
                let table = unsafe { world.tables.get_unchecked_mut(location.table_id) };
                self.storage = Some((table, location));
            }
        }
    }

    /// Gets read-only access to the world that the current entity belongs to.
    #[inline]
    pub fn world(&self) -> &World {
        unsafe { self.world.read_only() }
    }

    /// Returns this entity's [`World`], consuming itself.
    #[inline]
    pub fn into_world(self) -> &'a mut World {
        unsafe { self.world.full_mut() }
    }

    #[inline]
    pub fn world_cell(&self) -> WorldCell<'_> {
        self.world
    }
}

#[repr(transparent)]
struct RelocateGuard<'w, 'a>(&'a mut EntityOwned<'w>);

impl Drop for RelocateGuard<'_, '_> {
    fn drop(&mut self) {
        self.0.relocate();
    }
}

impl<'a> EntityOwned<'a> {
    /// Gives mutable access to this entity's [`World`] in a temporary scope.
    ///
    /// This is a safe alternative to using [`EntityOwned::world_cell`].
    #[inline]
    pub fn world_scope<R>(&mut self, func: impl FnOnce(&mut World) -> R) -> R {
        let unsafe_world = self.world;
        let _guard = RelocateGuard(self);
        func(unsafe { unsafe_world.data_mut() })
    }

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
