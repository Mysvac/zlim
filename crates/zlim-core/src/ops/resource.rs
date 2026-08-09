use core::any::TypeId;
use core::sync::atomic::Ordering;

use zlim_ptr::OwningPtr;

use crate::borrow::{NonSend, NonSendMut, Res, ResMut};
use crate::resource::{Resource, ResourceId};
use crate::tick::Tick;
use crate::utils::{DebugCheckedUnwrap, DebugName};
use crate::world::World;

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

/// Inserts a resource value from an [`OwningPtr`] into the world's slot
/// storage and returns a type-erased mutable pointer to the stored value.
///
/// The slot is prepared (allocated) if it does not already exist.
#[inline(never)]
fn insert_internal<'w>(
    world: &'w mut World,
    value: OwningPtr<'_>,
    id: ResourceId,
) -> zlim_ptr::PtrMut<'w> {
    unsafe {
        // SAFETY: `id` corresponds to a valid resource type registered
        // through Resources, and we hold exclusive access to the world.
        let slot = world.slots.register(&world.resources, id);

        let tick = Tick::new(*world.this_run.get_mut());
        slot.insert_untyped(value, tick);
        slot.get_data_mut().debug_checked_unwrap()
    }
}

#[cold]
#[track_caller]
#[inline(never)]
fn uninitialized_resource(name: DebugName) -> ! {
    panic!(
        "Requested resource `{name}` does not exist in the `World`.\n\
         Did you forget to add it using `world.insert_resource`?\n\
         Resources can also be added via `register_resource!`."
    )
}

// -----------------------------------------------------------------------------
// Send resources
// -----------------------------------------------------------------------------

impl World {
    /// Returns `true` if a resource of type `T` is present and active.
    ///
    /// This only checks storage state and does **not** create the resource
    /// or borrow it.  Lookup is O(1) via the type map.
    #[inline]
    pub fn contains_resource<T: Resource>(&self) -> bool {
        self.slots
            .get_by_type(TypeId::of::<T>())
            .is_some_and(|s| s.is_present())
    }

    /// Inserts or replaces a `Send` resource and returns a mutable reference
    /// to it.
    ///
    /// The resource is registered by type on first use.  Once inserted, it
    /// can be accessed through [`Res`], [`ResMut`], [`get_resource`],
    /// [`get_resource_ref`], or [`get_resource_mut`].
    ///
    /// [`get_resource`]: World::get_resource
    /// [`get_resource_ref`]: World::get_resource_ref
    /// [`get_resource_mut`]: World::get_resource_mut
    #[inline]
    pub fn insert_resource<T: Resource + Send>(&mut self, value: T) -> &mut T {
        let id = self.resources.register::<T>();
        zlim_ptr::into_owning!(value);
        unsafe { insert_internal(self, value, id).deref::<T>() }
    }

    /// Removes and returns a `Send` resource if it exists.
    #[inline]
    pub fn remove_resource<T: Resource + Send>(&mut self) -> Option<T> {
        let data = self.slots.get_by_type_mut(TypeId::of::<T>())?;
        unsafe { data.remove::<T>() }
    }

    /// Drops a `Send` resource if it exists.
    ///
    /// This is faster than [`remove_resource`] because the data does not
    /// need to be returned to the caller.
    ///
    /// [`remove_resource`]: World::remove_resource
    pub fn drop_resource<T: Resource + Send>(&mut self) {
        if let Some(data) = self.slots.get_by_type_mut(TypeId::of::<T>()) {
            unsafe {
                data.clear();
            }
        }
    }

    /// Returns a shared reference to a resource **without** change detection.
    /// Lookup is O(1) via the type map.
    pub fn get_resource<T: Resource + Sync>(&self) -> Option<&T> {
        let data = self.slots.get_by_type(TypeId::of::<T>())?;
        let ptr = data.get_data()?;
        ptr.debug_assert_aligned::<T>();
        Some(unsafe { ptr.deref::<T>() })
    }

    /// Returns a shared resource borrow **with** change detection.
    ///
    /// This mirrors the behaviour of the [`Res`] system parameter.
    /// Lookup is O(1) via the type map.
    pub fn get_resource_ref<T: Resource + Sync>(&self) -> Option<Res<'_, T>> {
        let data = self.slots.get_by_type(TypeId::of::<T>())?;
        let last_run = self.last_run();
        let this_run = Tick::new(self.this_run.load(Ordering::Relaxed));
        let ptr = data.get_ref(last_run, this_run)?;
        Some(unsafe { ptr.into_resource::<T>() })
    }

    /// Returns an exclusive resource borrow **with** change detection.
    ///
    /// This mirrors the behaviour of the [`ResMut`] system parameter.
    /// Lookup is O(1) via the type map.
    pub fn get_resource_mut<T: Resource + Send>(&mut self) -> Option<ResMut<'_, T>> {
        let last_run = self.last_run();
        let this_run = Tick::new(self.this_run.load(Ordering::Relaxed));
        let data = self.slots.get_by_type_mut(TypeId::of::<T>())?;
        let ptr = data.get_mut(last_run, this_run)?;
        Some(unsafe { ptr.into_resource::<T>() })
    }

    /// Returns a shared reference to a resource **without** change detection.
    ///
    /// # Panics
    ///
    /// Panics if the resource does not exist.  Use [`get_resource`] for a
    /// fallible variant.
    ///
    /// [`get_resource`]: World::get_resource
    #[track_caller]
    pub fn resource<T: Resource + Sync>(&self) -> &T {
        self.get_resource()
            .unwrap_or_else(|| uninitialized_resource(DebugName::type_name::<T>()))
    }

    /// Returns a shared resource borrow **with** change detection.
    ///
    /// # Panics
    ///
    /// Panics if the resource does not exist.  Use [`get_resource_ref`] for
    /// a fallible variant.
    ///
    /// [`get_resource_ref`]: World::get_resource_ref
    #[track_caller]
    pub fn resource_ref<T: Resource + Sync>(&self) -> Res<'_, T> {
        self.get_resource_ref()
            .unwrap_or_else(|| uninitialized_resource(DebugName::type_name::<T>()))
    }

    /// Returns an exclusive resource borrow **with** change detection.
    ///
    /// # Panics
    ///
    /// Panics if the resource does not exist.  Use [`get_resource_mut`] for
    /// a fallible variant.
    ///
    /// [`get_resource_mut`]: World::get_resource_mut
    #[track_caller]
    pub fn resource_mut<T: Resource + Send>(&mut self) -> ResMut<'_, T> {
        self.get_resource_mut()
            .unwrap_or_else(|| uninitialized_resource(DebugName::type_name::<T>()))
    }
}

// -----------------------------------------------------------------------------
// NonSend resources
// -----------------------------------------------------------------------------

impl World {
    /// Returns `true` if a `!Send` resource of type `T` is present and active.
    ///
    /// In the current storage model, this inspects the same underlying slot
    /// as [`contains_resource`] and reports the same presence result.
    /// Lookup is O(1) via the type map.
    ///
    /// [`contains_resource`]: World::contains_resource
    #[inline]
    pub fn contains_non_send<T: Resource>(&self) -> bool {
        self.slots
            .get_by_type(TypeId::of::<T>())
            .is_some_and(|s| s.is_present())
    }

    /// Inserts or replaces a `!Send` resource and returns a mutable reference
    /// to it.
    ///
    /// Unlike [`insert_resource`], this accepts `!Sync` / `!Send` values
    /// without additional trait bounds.
    ///
    /// [`insert_resource`]: World::insert_resource
    pub fn insert_non_send<T: Resource>(&mut self, value: T) -> &mut T {
        let id = self.resources.register::<T>();
        zlim_ptr::into_owning!(value);
        unsafe { insert_internal(self, value, id).deref::<T>() }
    }

    /// Removes and returns a `!Send` resource if it exists.
    pub fn remove_non_send<T: Resource>(&mut self) -> Option<T> {
        let data = self.slots.get_by_type_mut(TypeId::of::<T>())?;
        unsafe { data.remove::<T>() }
    }

    /// Drops a `!Send` resource if it exists.
    ///
    /// This is faster than [`remove_non_send`] because the data does not
    /// need to be returned to the caller.
    ///
    /// [`remove_non_send`]: World::remove_non_send
    pub fn drop_non_send<T: Resource>(&mut self) {
        if let Some(data) = self.slots.get_by_type_mut(TypeId::of::<T>()) {
            unsafe {
                data.clear();
            }
        }
    }

    /// Returns a shared reference to a `!Send` resource **without** change
    /// detection.  Lookup is O(1) via the type map.
    pub fn get_non_send<T: Resource>(&self) -> Option<&T> {
        let data = self.slots.get_by_type(TypeId::of::<T>())?;
        let ptr = data.get_data()?;
        ptr.debug_assert_aligned::<T>();
        Some(unsafe { ptr.deref::<T>() })
    }

    /// Returns a shared `!Send` resource borrow **with** change detection.
    ///
    /// This mirrors the behaviour of the [`NonSend`] system parameter.
    /// Lookup is O(1) via the type map.
    pub fn get_non_send_ref<T: Resource>(&self) -> Option<NonSend<'_, T>> {
        let data = self.slots.get_by_type(TypeId::of::<T>())?;
        let last_run = self.last_run();
        let this_run = Tick::new(self.this_run.load(Ordering::Relaxed));
        let ptr = data.get_ref(last_run, this_run)?;
        Some(unsafe { ptr.into_non_send::<T>() })
    }

    /// Returns an exclusive `!Send` resource borrow **with** change detection.
    ///
    /// This mirrors the behaviour of the [`NonSendMut`] system parameter.
    /// Lookup is O(1) via the type map.
    pub fn get_non_send_mut<T: Resource>(&mut self) -> Option<NonSendMut<'_, T>> {
        let last_run = self.last_run();
        let this_run = Tick::new(self.this_run.load(Ordering::Relaxed));
        let data = self.slots.get_by_type_mut(TypeId::of::<T>())?;
        let ptr = data.get_mut(last_run, this_run)?;
        Some(unsafe { ptr.into_non_send::<T>() })
    }

    /// Returns a shared reference to a `!Send` resource **without** change
    /// detection.
    ///
    /// # Panics
    ///
    /// Panics if the resource does not exist.  Use [`get_non_send`] for a
    /// fallible variant.
    ///
    /// [`get_non_send`]: World::get_non_send
    #[track_caller]
    pub fn non_send<T: Resource>(&self) -> &T {
        self.get_non_send()
            .unwrap_or_else(|| uninitialized_resource(DebugName::type_name::<T>()))
    }

    /// Returns a shared `!Send` resource borrow **with** change detection.
    ///
    /// # Panics
    ///
    /// Panics if the resource does not exist.  Use [`get_non_send_ref`] for
    /// a fallible variant.
    ///
    /// [`get_non_send_ref`]: World::get_non_send_ref
    #[track_caller]
    pub fn non_send_ref<T: Resource>(&self) -> NonSend<'_, T> {
        self.get_non_send_ref()
            .unwrap_or_else(|| uninitialized_resource(DebugName::type_name::<T>()))
    }

    /// Returns an exclusive `!Send` resource borrow **with** change detection.
    ///
    /// # Panics
    ///
    /// Panics if the resource does not exist.  Use [`get_non_send_mut`] for
    /// a fallible variant.
    ///
    /// [`get_non_send_mut`]: World::get_non_send_mut
    #[track_caller]
    pub fn non_send_mut<T: Resource>(&mut self) -> NonSendMut<'_, T> {
        self.get_non_send_mut()
            .unwrap_or_else(|| uninitialized_resource(DebugName::type_name::<T>()))
    }
}
