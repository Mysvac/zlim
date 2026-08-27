//! Resource management methods implemented on `World`.

use core::any::TypeId;
use core::cell::UnsafeCell;
use core::ptr::NonNull;
use core::sync::atomic::Ordering;

use zlim_ptr::PtrMut;
use zlim_utils::debug::DebugName;

use crate::borrow::{NonSendMut, Res, ResMut, UntypedMut};
use crate::resource::{Resource, ResourceCell};
use crate::tick::{Tick, TicksMut};
use crate::utils::DebugCheckedUnwrap;
use crate::world::{DeferredWorld, FromWorld, NonSendWorld, World};

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

#[cold]
#[track_caller]
#[inline(never)]
pub(crate) fn uninitialized_resource(name: DebugName) -> ! {
    panic!(
        "Requested resource `{name}` does not exist in the `World`.\n\
         Did you forget to add it using `world.insert_resource`?\n\
         Resources can also be added via `register_resource!`."
    )
}

pub(super) struct ReinsertGuard {
    pub cell: &'static UnsafeCell<ResourceCell>,
    pub ptr: NonNull<u8>,
    pub added: Tick,
    pub changed: Tick,
}

impl Drop for ReinsertGuard {
    fn drop(&mut self) {
        let cell_mut = unsafe { &mut *self.cell.get() };

        if cell_mut.is_present() {
            ::core::hint::cold_path();

            let name = cell_mut.database().type_name;

            if std::thread::panicking() {
                // return early if panicking
                return;
            }

            zlim_log::error!(
                "Resource `{name}` was inserted during a call to World::resource_scope, \
                which may result in unexpected behavior.\n The value inserted will be \
                overwritten at the end of the scope.",
            );
        }

        unsafe {
            cell_mut.from_raw(self.ptr, self.added, self.changed);
        }
    }
}

// -----------------------------------------------------------------------------
// Send resources
// -----------------------------------------------------------------------------

impl World {
    /// Initializes the resource if it does not exist.
    pub fn init_resource<T: Resource + Send + FromWorld>(&mut self) {
        let ty = TypeId::of::<T>();
        if let Some(slot) = self.resources.get(ty)
            && slot.is_present()
        {
            return;
        }

        let value = T::from_world(self);
        self.insert_resource::<T>(value);
    }

    /// Returns `true` if a resource of type `T` is present and active.
    ///
    /// This only checks storage state and does **not** create the resource
    /// or borrow it.  Lookup is O(1) via the type map.
    #[inline]
    pub fn contains_resource<T: Resource>(&self) -> bool {
        let ty = TypeId::of::<T>();
        self.resources.get(ty).is_some_and(ResourceCell::is_present)
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
        zlim_ptr::into_owning!(value);
        let tick = self.this_run_fast();
        unsafe {
            let cell = &mut *self.resources.entry::<T>().get();
            cell.insert_untyped(value, tick);
            cell.get_data_mut().debug_checked_unwrap().deref::<T>()
        }
    }

    /// Removes and returns a `Send` resource if it exists.
    #[inline]
    pub fn remove_resource<T: Resource + Send>(&mut self) -> Option<T> {
        let data = self.resources.get_mut(TypeId::of::<T>())?;
        unsafe { data.remove::<T>() }
    }

    /// Drops a `Send` resource if it exists.
    ///
    /// This is faster than [`remove_resource`] because the data does not
    /// need to be returned to the caller.
    ///
    /// [`remove_resource`]: World::remove_resource
    pub fn drop_resource<T: Resource + Send>(&mut self) {
        if let Some(data) = self.resources.get_mut(TypeId::of::<T>()) {
            unsafe {
                data.clear();
            }
        }
    }

    /// Returns a shared reference to a resource **without** change detection.
    /// Lookup is O(1) via the type map.
    pub fn get_resource<T: Resource + Sync>(&self) -> Option<&T> {
        let data = self.resources.get(TypeId::of::<T>())?;
        let ptr = data.get_data()?;
        ptr.debug_assert_aligned::<T>();
        Some(unsafe { ptr.deref::<T>() })
    }

    /// Returns a shared resource borrow **with** change detection.
    ///
    /// This mirrors the behaviour of the [`Res`] system parameter.
    /// Lookup is O(1) via the type map.
    pub fn get_resource_ref<T: Resource + Sync>(&self) -> Option<Res<'_, T>> {
        let data = self.resources.get(TypeId::of::<T>())?;
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
        let this_run = self.this_run_fast();
        let data = self.resources.get_mut(TypeId::of::<T>())?;
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

    /// Returns an exclusive resource borrow with change detection.
    ///
    /// If the resource does not exist, it will be automatically initialized.
    pub fn resource_mut_or_init<T: Resource + Send + FromWorld>(&mut self) -> ResMut<'_, T> {
        #[cold]
        #[inline(never)]
        fn get_or_init_cold<T: Resource + Send + FromWorld>(this: &mut World) -> ResMut<'_, T> {
            let value = T::from_world(this);

            let this_run = this.this_run_fast();
            let last_run = this.last_run();

            let cell = unsafe { &mut *this.resources.entry::<T>().get() };
            unsafe {
                cell.insert(value, this_run);

                cell.get_mut(last_run, this_run)
                    .debug_checked_unwrap()
                    .into_resource::<T>()
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
            unsafe { ptr.into_resource::<T>() }
        } else {
            let full_mut = unsafe { world_cell.full_mut() };
            get_or_init_cold::<T>(full_mut)
        }
    }
}

// -----------------------------------------------------------------------------
// DeferredWorld — mutable resource access
// -----------------------------------------------------------------------------

impl DeferredWorld<'_> {
    /// Initializes the resource if it does not exist.
    ///
    /// We temporarily believe that the "initialization" of resources does not belong
    /// to structure change. Because ResourceCell is stored in isolation, "initialization"
    /// does not cause external references to become invalid.
    pub fn init_resource<T: Resource + Send + FromWorld>(&mut self) {
        // SAFETY: `DeferredWorld` holds exclusive access to the world for the
        // duration of this borrow; `data_mut` reinterprets it as `&mut World`,
        // and the resource accessors only mutate existing values.
        unsafe { self.cell().data_mut() }.init_resource::<T>()
    }

    /// Returns an exclusive resource borrow **with** change detection.
    ///
    /// # Panics
    ///
    /// Panics if the resource does not exist.  Use [`Self::get_resource_mut`]
    /// for a fallible variant.
    ///
    /// Immutable resource access (e.g. [`World::get_resource_ref`]) is
    /// available through the [`DeferredWorld`] deref to `&World`.
    #[track_caller]
    pub fn resource_mut<T: Resource + Send>(&mut self) -> ResMut<'_, T> {
        // SAFETY: `DeferredWorld` holds exclusive access to the world for the
        // duration of this borrow; `data_mut` reinterprets it as `&mut World`,
        // and the resource accessors only mutate existing values.
        unsafe { self.cell().data_mut() }.resource_mut::<T>()
    }

    /// Returns an exclusive resource borrow **with** change detection.
    ///
    /// Lookup is O(1) via the type map.  Returns `None` if the resource does
    /// not exist.
    pub fn get_resource_mut<T: Resource + Send>(&mut self) -> Option<ResMut<'_, T>> {
        // SAFETY: `DeferredWorld` holds exclusive access to the world for the
        // duration of this borrow; `data_mut` reinterprets it as `&mut World`,
        // and the resource accessors only mutate existing values.
        unsafe { self.cell().data_mut() }.get_resource_mut::<T>()
    }

    /// Returns an exclusive resource borrow with change detection.
    ///
    /// If the resource does not exist, it will be automatically initialized.
    ///
    /// We temporarily believe that the "initialization" of resources does not belong
    /// to structure change. Because ResourceCell is stored in isolation, "initialization"
    /// does not cause external references to become invalid.
    pub fn resource_mut_or_init<T: FromWorld + Resource + Send>(&mut self) -> ResMut<'_, T> {
        // SAFETY: `DeferredWorld` holds exclusive access to the world for the
        // duration of this borrow; `data_mut` reinterprets it as `&mut World`,
        // and the resource accessors only mutate existing values.
        unsafe { self.cell().data_mut() }.resource_mut_or_init::<T>()
    }
}

// -----------------------------------------------------------------------------
// scope

impl World {
    /// Executes a closure with exclusive access to a resource and the world.
    ///
    /// If the resource is not exist, return `None` directly.
    ///
    /// This method temporarily removes the resource from the world to satisfy
    /// Rust's borrowing rules, allowing the closure to mutably borrow both the
    /// resource and the world simultaneously.
    pub fn try_resource_scope<T: Resource + Send, R>(
        &mut self,
        func: impl FnOnce(&mut World, ResMut<T>) -> R,
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
            let res_mut: ResMut<T> = UntypedMut {
                value: PtrMut::new(ptr),
                ticks: TicksMut {
                    added: &mut guard.added,
                    changed: &mut guard.changed,
                    last_run,
                    this_run,
                },
            }
            .into_resource();

            Some(func(self, res_mut))
        }
    }

    /// Executes a closure with exclusive access to a resource and the world.
    ///
    /// This method temporarily removes the resource from the world to satisfy
    /// Rust's borrowing rules, allowing the closure to mutably borrow both the
    /// resource and the world simultaneously.
    ///
    /// # Panics
    ///
    /// Panics if the resource does not exist.  Use [`try_resource_scope`] for
    /// a fallible variant.
    ///
    /// [`try_resource_scope`]: World::try_resource_scope
    pub fn resource_scope<T: Resource + Send, R>(
        &mut self,
        func: impl FnOnce(&mut World, ResMut<T>) -> R,
    ) -> R {
        self.try_resource_scope(func)
            .unwrap_or_else(|| uninitialized_resource(DebugName::type_name::<T>()))
    }
}

impl World {
    /// Executes a closure with exclusive access to a non_send resource and the world.
    ///
    /// If the resource is not exist, return `None` directly.
    ///
    /// This function is equivalant to:
    ///
    /// - `self.with_non_send_mut(|world| world.try_non_send_scope(func))`.
    ///
    /// This method temporarily removes the resource from the world to satisfy
    /// Rust's borrowing rules, allowing the closure to mutably borrow both the
    /// resource and the world simultaneously.
    pub fn try_non_send_scope<T: Resource, R: Send>(
        &mut self,
        func: impl FnOnce(&mut NonSendWorld, NonSendMut<T>) -> R + Send,
    ) -> Option<R> {
        self.with_non_send_mut(|world| world.try_non_send_scope(func))
    }

    /// Executes a closure with exclusive access to a non_send resource and the world.
    ///
    /// This function is equivalant to:
    ///
    /// - `self.with_non_send_mut(|world| world.try_non_send_scope(func)).unwrap()`.
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
    /// [`try_non_send_scope`]: World::try_non_send_scope
    pub fn non_send_scope<T: Resource, R: Send>(
        &mut self,
        func: impl FnOnce(&mut NonSendWorld, NonSendMut<T>) -> R + Send,
    ) -> R {
        self.with_non_send_mut(|world| world.try_non_send_scope(func))
            .unwrap_or_else(|| uninitialized_resource(DebugName::type_name::<T>()))
    }
}

// -----------------------------------------------------------------------------
