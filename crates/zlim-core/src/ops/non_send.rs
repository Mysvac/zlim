//! Non-`Send` resource access through [`NonSendWorld`].
//!
//! [`NonSendWorld`] is only reachable on the main thread — through
//! [`World::with_non_send`] / [`World::with_non_send_mut`] or as a
//! `NonSend` system parameter — so the `!Send` resource accessors live here
//! instead of directly on [`World`].

use core::any::TypeId;
use core::sync::atomic::Ordering;

use zlim_ptr::PtrMut;
use zlim_utils::debug::DebugName;

use super::resource::ReinsertGuard;
use super::resource::uninitialized_resource;
use crate::borrow::UntypedMut;
use crate::borrow::{NonSend, NonSendMut};
use crate::resource::Resource;
use crate::resource::ResourceCell;
use crate::tick::Tick;
use crate::tick::TicksMut;
use crate::utils::DebugCheckedUnwrap;
use crate::world::{FromWorld, NonSendWorld, World};

impl NonSendWorld {
    /// Initializes the resource if it does not exist.
    pub fn init_non_send<T: Resource + FromWorld>(&mut self) {
        let ty = TypeId::of::<T>();
        if let Some(slot) = self.resources.get(ty)
            && slot.is_present()
        {
            return;
        }

        let value = T::from_world(&*self);
        self.insert_non_send::<T>(value);
    }

    /// Returns `true` if a `!Send` resource of type `T` is present and active.
    ///
    /// In the current storage model, this inspects the same underlying cell
    /// as [`contains_resource`] and reports the same presence result.
    /// Lookup is O(1) via the type map.
    ///
    /// [`contains_resource`]: World::contains_resource
    #[inline]
    pub fn contains_non_send<T: Resource>(&self) -> bool {
        let ty = TypeId::of::<T>();
        self.resources.get(ty).is_some_and(ResourceCell::is_present)
    }

    /// Inserts or replaces a `!Send` resource and returns a mutable reference
    /// to it.
    ///
    /// Unlike [`insert_resource`], this accepts `!Sync` / `!Send` values
    /// without additional trait bounds.
    ///
    /// [`insert_resource`]: World::insert_resource
    #[inline]
    pub fn insert_non_send<T: Resource>(&mut self, value: T) -> &mut T {
        zlim_ptr::into_owning!(value);
        let tick = self.this_run_fast();
        unsafe {
            let cell = &mut *self.resources.entry::<T>().get();
            cell.insert_untyped(value, tick);
            cell.get_data_mut().debug_checked_unwrap().deref::<T>()
        }
    }

    /// Removes and returns a `!Send` resource if it exists.
    #[inline]
    pub fn remove_non_send<T: Resource>(&mut self) -> Option<T> {
        let data = self.resources.get_mut(TypeId::of::<T>())?;
        unsafe { data.remove::<T>() }
    }

    /// Drops a `!Send` resource if it exists.
    ///
    /// This is faster than [`remove_non_send`] because the data does not
    /// need to be returned to the caller.
    ///
    /// [`remove_non_send`]: Self::remove_non_send
    pub fn drop_non_send<T: Resource>(&mut self) {
        if let Some(data) = self.resources.get_mut(TypeId::of::<T>()) {
            unsafe {
                data.clear();
            }
        }
    }

    /// Returns a shared reference to a `!Send` resource **without** change
    /// detection.  Lookup is O(1) via the type map.
    pub fn get_non_send<T: Resource>(&self) -> Option<&T> {
        let data = self.resources.get(TypeId::of::<T>())?;
        let ptr = data.get_data()?;
        ptr.debug_assert_aligned::<T>();
        Some(unsafe { ptr.deref::<T>() })
    }

    /// Returns a shared `!Send` resource borrow **with** change detection.
    ///
    /// This mirrors the behaviour of the [`NonSend`] system parameter.
    /// Lookup is O(1) via the type map.
    pub fn get_non_send_ref<T: Resource>(&self) -> Option<NonSend<'_, T>> {
        let data = self.resources.get(TypeId::of::<T>())?;
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
        let this_run = self.this_run_fast();
        let data = self.resources.get_mut(TypeId::of::<T>())?;
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
    /// [`get_non_send`]: Self::get_non_send
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
    /// [`get_non_send_ref`]: Self::get_non_send_ref
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
    /// [`get_non_send_mut`]: Self::get_non_send_mut
    #[track_caller]
    pub fn non_send_mut<T: Resource>(&mut self) -> NonSendMut<'_, T> {
        self.get_non_send_mut()
            .unwrap_or_else(|| uninitialized_resource(DebugName::type_name::<T>()))
    }

    /// Returns an exclusive resource borrow with change detection.
    ///
    /// If the resource does not exist, it will be automatically initialized.
    pub fn non_send_mut_or_init<T: Resource + FromWorld>(&mut self) -> NonSendMut<'_, T> {
        #[cold]
        #[inline(never)]
        fn get_or_init_cold<T: Resource + FromWorld>(world: &mut World) -> NonSendMut<'_, T> {
            let value = T::from_world(world);

            let this_run = world.this_run_fast();
            let last_run = world.last_run();

            let cell = unsafe { &mut *world.resources.entry::<T>().get() };
            unsafe {
                cell.insert(value, this_run);

                cell.get_mut(last_run, this_run)
                    .debug_checked_unwrap()
                    .into_non_send::<T>()
            }
        }

        let last_run = self.last_run();
        let this_run = self.this_run_fast();
        let world_cell = self.cell();
        let world_mut = unsafe { world_cell.data_mut() };

        let ty = TypeId::of::<T>();
        if let Some(slot) = world_mut.resources.get_mut(ty)
            && let Some(ptr) = slot.get_mut(last_run, this_run)
        {
            unsafe { ptr.into_non_send::<T>() }
        } else {
            let full_mut = unsafe { world_cell.full_mut() };
            get_or_init_cold::<T>(full_mut)
        }
    }
}

// -----------------------------------------------------------------------------
// scope

impl NonSendWorld {
    /// Executes a closure with exclusive access to a non_send resource and the world.
    ///
    /// If the resource is not exist, return `None` directly.
    ///
    /// This method temporarily removes the resource from the world to satisfy
    /// Rust's borrowing rules, allowing the closure to mutably borrow both the
    /// resource and the world simultaneously.
    pub fn try_non_send_scope<T: Resource, R>(
        &mut self,
        func: impl FnOnce(&mut NonSendWorld, NonSendMut<T>) -> R,
    ) -> Option<R> {
        let cell = self.resources.get_cell(TypeId::of::<T>())?;
        let cell_mut = unsafe { &mut *cell.get() };
        let (ptr, added, changed) = unsafe { cell_mut.take_raw()? };

        let mut guard = ReinsertGuard {
            cell,
            ptr,
            added,
            changed,
        };

        let last_run = self.last_run();
        let this_run = self.this_run_fast();

        unsafe {
            let res_mut: NonSendMut<T> = UntypedMut {
                value: PtrMut::new(ptr),
                ticks: TicksMut {
                    added: &mut guard.added,
                    changed: &mut guard.changed,
                    last_run,
                    this_run,
                },
            }
            .into_non_send();

            Some(func(self, res_mut))
        }
    }

    /// Executes a closure with exclusive access to a non_send resource and the world.
    ///
    /// This method temporarily removes the resource from the world to satisfy
    /// Rust's borrowing rules, allowing the closure to mutably borrow both the
    /// resource and the world simultaneously.
    ///
    /// # Panics
    ///
    /// Panics if the resource does not exist.  Use [`try_non_send_scope`] for
    /// a fallible variant.
    ///
    /// [`try_non_send_scope`]: NonSendWorld::try_non_send_scope
    pub fn non_send_scope<T: Resource, R>(
        &mut self,
        func: impl FnOnce(&mut NonSendWorld, NonSendMut<T>) -> R,
    ) -> R {
        self.try_non_send_scope(func)
            .unwrap_or_else(|| uninitialized_resource(DebugName::type_name::<T>()))
    }
}

// -----------------------------------------------------------------------------
