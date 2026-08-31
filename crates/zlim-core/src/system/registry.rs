//! The world's system cache: [`SystemHandle`]s and the [`SystemCache`] registry
//! that stores cached system instances keyed by their [`SystemId`].

use core::any::Any;
use core::any::TypeId;
use core::fmt::Debug;
use core::hash::Hash;
use core::marker::PhantomData;

use zlim_utils::ext::TypeMap;
use zlim_utils::hash::map::Entry;
use zlim_utils::hash::{HashMap, NoopState};

use crate::system::System;
use crate::system::SystemId;
use crate::system::SystemInput;
use crate::utils::DebugCheckedUnwrap;

// -----------------------------------------------------------------------------
// SystemHandle

/// A cheap, type-safe handle to a system cached in the world.
///
/// Obtained from [`World::insert_system`] and passed to
/// [`World::invoke_handle`].  The `I`/`O` type parameters tie the
/// handle to the system's input/output signature so cache lookups stay
/// type-checked.
///
/// Handles are `Copy` and compare by the underlying [`SystemId`].
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// fn tick() {}
///
/// let mut world = World::alloc();
///
/// // Insert the system once, then run it through its handle.
/// let handle = world.insert_system(tick);
/// assert_eq!(handle.id(), tick.system_id());
/// world.invoke_handle(handle, ()).unwrap();
///
/// // Handles are `Copy`, so the same handle can be reused.
/// world.invoke_handle(handle, ()).unwrap();
/// ```
///
/// [`World::insert_system`]: crate::world::World::insert_system
/// [`World::invoke_handle`]: crate::world::World::invoke_handle
pub struct SystemHandle<I, O> {
    id: SystemId,
    _marker: PhantomData<(I, O)>,
}

unsafe impl<I, O> Sync for SystemHandle<I, O> {}
unsafe impl<I, O> Send for SystemHandle<I, O> {}

impl<I, O> Hash for SystemHandle<I, O> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.id.type_id().hash(state);
    }
}

impl<I, O> Debug for SystemHandle<I, O> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&self.id, f)
    }
}

impl<I, O> PartialEq for SystemHandle<I, O> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<I, O> Eq for SystemHandle<I, O> {}

impl<I, O> Copy for SystemHandle<I, O> {}

impl<I, O> Clone for SystemHandle<I, O> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<I, O> SystemHandle<I, O> {
    /// Returns the underlying [`SystemId`].
    #[inline(always)]
    pub const fn id(self) -> SystemId {
        self.id
    }
}

impl<I, O> From<SystemHandle<I, O>> for SystemId {
    #[inline(always)]
    fn from(value: SystemHandle<I, O>) -> Self {
        value.id
    }
}

impl<I: SystemInput, O> SystemHandle<I, O> {
    /// Create a handle from given [`SystemId`].
    #[inline(always)]
    pub const fn new(id: SystemId) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }
}

impl<I: SystemInput, O> From<SystemId> for SystemHandle<I, O> {
    #[inline(always)]
    fn from(id: SystemId) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }
}

impl<I, O> SystemHandle<I, O>
where
    I: SystemInput + 'static,
    O: 'static,
{
    /// Create a handle from given [`System`].
    #[inline(always)]
    pub fn from_system(system: &dyn System<Input = I, Output = O>) -> Self {
        let id = system.id();
        Self {
            id,
            _marker: PhantomData,
        }
    }
}

// -----------------------------------------------------------------------------
// SystemCache

type SystemBox<I, O> = Box<dyn System<Input = I, Output = O>>;

struct SystemSlot<I, O> {
    system: SystemBox<I, O>,
    #[cfg(feature = "trace")]
    span: zlim_log::Span,
}

type SystemMap<I, O> = HashMap<SystemId, Option<SystemSlot<I, O>>, NoopState>;

/// Type-erased cache of system instances, keyed by their [`SystemId`].
///
/// Each concrete `(I, O)` signature gets its own internal map, stored behind
/// a [`TypeId`]-keyed entry, so a single `Systems` value can hold systems
/// with different input/output types.  Every [`World`] owns one.
///
/// [`World`]: crate::world::World
pub(crate) struct SystemCache {
    mapper: TypeMap<Box<dyn Any + Send + Sync + 'static>>,
}

impl Debug for SystemCache {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SystemCache").finish_non_exhaustive()
    }
}

impl SystemCache {
    pub(crate) const fn new() -> Self {
        Self {
            mapper: TypeMap::new(),
        }
    }
}

impl SystemCache {
    #[inline]
    fn with<I, O>(&mut self) -> &mut SystemMap<I, O>
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        match self.mapper.entry(TypeId::of::<SystemHandle<I, O>>()) {
            Entry::Occupied(entry) => {
                let erased = entry.into_mut();
                unsafe { erased.downcast_mut().debug_checked_unwrap() }
            }
            Entry::Vacant(entry) => {
                let map = SystemMap::<I, O>::with_hasher(NoopState);
                let erased = entry.insert(Box::new(map));
                unsafe { erased.downcast_mut().debug_checked_unwrap() }
            }
        }
    }

    #[inline]
    fn try_with<I, O>(&mut self) -> Option<&mut SystemMap<I, O>>
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        let erased = self.mapper.get_mut(TypeId::of::<SystemHandle<I, O>>())?;
        unsafe { Some(erased.downcast_mut().debug_checked_unwrap()) }
    }

    /// Returns the cache slot for `handle`, creating the per-signature map
    /// if needed.
    #[inline(never)]
    fn entry<I, O>(&mut self, handle: SystemHandle<I, O>) -> &mut Option<SystemSlot<I, O>>
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        self.with::<I, O>().entry(handle.id).or_default()
    }

    /// Caches `system`, returning the previously cached instance with the
    /// same id, if any.
    #[inline(never)]
    fn insert<I, O>(&mut self, id: SystemId, system: SystemSlot<I, O>) -> Option<SystemSlot<I, O>>
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        self.with::<I, O>().insert(id, Some(system))?
    }

    /// Removes and returns the cached instance for `handle`, if present.
    #[inline(never)]
    fn remove<I, O>(&mut self, handle: SystemHandle<I, O>) -> Option<SystemSlot<I, O>>
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        self.try_with::<I, O>()?.remove(&handle.id)?
    }
}

// -----------------------------------------------------------------------------

use crate::system::IntoSystem;
use crate::system::SystemError;
use crate::system::SystemFlags;
use crate::world::NonSendWorld;
use crate::world::World;

use zlim_log as log;

// -----------------------------------------------------------------------------
// BoxedSystem

type BoxedSystem<I, O> = Box<dyn System<Input = I, Output = O>>;

// -----------------------------------------------------------------------------
// World

impl World {
    /// Inserts a system into the world's cache and returns a handle to it.
    ///
    /// The system is keyed by its [`SystemId`]: inserting the same system
    /// type twice is a no-op and returns an equivalent handle.  The cached
    /// instance keeps its internal state (e.g. `Local` parameters) across
    /// [`World::invoke_handle`] calls.
    ///
    /// Initialization is deferred: the cached instance is initialized the
    /// first time it is run.
    ///
    /// [`SystemId`]: crate::system::SystemId
    #[inline]
    pub fn insert_system<I, O, M>(
        &mut self,
        system: impl IntoSystem<I, O, M> + 'static,
    ) -> SystemHandle<I, O>
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        let handle: SystemHandle<I, O> = system.system_handle();
        let entry = self.system_cache.entry(handle);

        if entry.is_none() {
            *entry = Some(SystemSlot::<I, O> {
                system: Box::new(IntoSystem::into_system(system)),
                #[cfg(feature = "trace")]
                span: zlim_log::info_span!(parent: None, "system", name=?handle.id),
            });
            // Deferred — the system instance is initialized on its first run.
        }

        handle
    }

    /// Removes a cached system and returns its instance, if present.
    ///
    /// Returns `None` if the handle was never inserted (or was already
    /// removed).
    #[inline]
    pub fn remove_system<I, O>(&mut self, handle: SystemHandle<I, O>) -> Option<BoxedSystem<I, O>>
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        self.system_cache.remove(handle).map(|x| x.system)
    }

    /// Runs a system with the given input without caching it.
    ///
    /// A fresh system instance is built, initialized, executed once, and
    /// discarded — internal state (e.g. `Local` parameters) does not
    /// persist between calls.
    ///
    /// Use [`World::invoke`] when state should survive across runs.
    ///
    /// Because the system may be required executing on the main thread,
    /// both input and output need to support Send.
    ///
    /// # Change detection
    ///
    /// Systems invoked through `invoke` directly follow the `World`'s own
    /// change detection: before each run the instance's baseline is reset
    /// to [`World::last_run`], so it observes **every** change since the
    /// world baseline — not just changes since its previous run. Advance the
    /// world baseline with [`World::clear_trackers`] to control what direct
    /// invocations report as changed.
    #[inline]
    pub fn invoke_once<'a, I, O, M>(
        &mut self,
        system: impl IntoSystem<I, O, M> + 'static,
        input: I::Data<'a>,
    ) -> Result<O, SystemError>
    where
        I: SystemInput<Data<'a>: Send> + Send + 'static,
        O: Send + 'static,
    {
        self.flush();

        let mut system = IntoSystem::into_system(system);
        let nonsend = system.flags().intersects(SystemFlags::NON_SEND);

        let func = || {
            #[cfg(feature = "trace")]
            let _span = zlim_log::info_span!(parent: None, "system", name=?system.id()).entered();

            system.initialize(self);
            system.set_last_run(self.last_run);
            let world = self.cell();
            let result = unsafe { system.run_raw(input, world) };
            system.apply_deferred(self);
            result
        };

        if nonsend {
            zlim_task::invoke_on_main(func)
        } else {
            func()
        }
    }

    /// Runs the given system, caching its instance for later runs.
    ///
    /// If a system with the same [`SystemId`] is already cached, that cached
    /// instance is used, so its internal state (e.g. `Local` parameters)
    /// persists across runs; otherwise a fresh instance is built and
    /// cached.  The instance is (re)initialized and executed against the
    /// world, then put back into the cache.
    ///
    /// Use [`World::invoke_once`] for an uncached single-shot run.
    ///
    /// Because the system may be required executing on the main thread,
    /// both input and output need to support Send.
    ///
    /// # Change detection
    ///
    /// Systems invoked through `invoke` directly follow the `World`'s own
    /// change detection: before each run the cached instance's baseline is
    /// reset to [`World::last_run`], so it observes **every** change since
    /// the world baseline — not just changes since its previous run. Advance
    /// the world baseline with [`World::clear_trackers`] to control what
    /// direct invocations report as changed.
    ///
    /// [`SystemId`]: crate::system::SystemId
    #[inline]
    pub fn invoke<'a, I, O, M>(
        &mut self,
        system: impl IntoSystem<I, O, M> + 'static,
        input: I::Data<'a>,
    ) -> Result<O, SystemError>
    where
        I: SystemInput<Data<'a>: Send> + Send + 'static,
        O: Send + 'static,
    {
        self.flush();

        let handle = system.system_handle();

        let mut slot = self
            .system_cache
            .remove(handle)
            .unwrap_or_else(|| SystemSlot::<I, O> {
                system: Box::new(IntoSystem::into_system(system)),
                #[cfg(feature = "trace")]
                span: zlim_log::info_span!(parent: None, "system", name=?handle.id),
            });

        let nonsend = slot.system.flags().intersects(SystemFlags::NON_SEND);

        let func = || {
            #[cfg(feature = "trace")]
            let _span = slot.span.enter();
            slot.system.initialize(self);
            slot.system.set_last_run(self.last_run);
            let world = self.cell();
            let result = unsafe { slot.system.run_raw(input, world) };
            slot.system.apply_deferred(self);
            result
        };

        let result = if nonsend {
            zlim_task::invoke_on_main(func)
        } else {
            func()
        };

        if self.system_cache.insert(handle.id, slot).is_some() {
            ::core::hint::cold_path();
            log::warn!("The same System `{handle:?}` was inserted during the execution.");
        }

        result
    }

    /// Runs the cached system identified by `handle` with the given input.
    ///
    /// The system must have been cached first via [`World::insert_system`];
    /// returns [`SystemError::Unregistered`] otherwise.  The cached instance
    /// is (re)initialized and executed against the world, then put back into
    /// the cache, so its internal state persists between runs.
    ///
    /// Because the system may be required executing on the main thread,
    /// both input and output need to support Send.
    ///
    /// # Change detection
    ///
    /// Systems invoked through `invoke` directly follow the `World`'s own
    /// change detection: before each run the cached instance's baseline is
    /// reset to [`World::last_run`], so it observes **every** change since
    /// the world baseline — not just changes since its previous run. Advance
    /// the world baseline with [`World::clear_trackers`] to control what
    /// direct invocations report as changed.
    ///
    /// [`SystemError::Unregistered`]: crate::system::SystemError::Unregistered
    #[inline]
    pub fn invoke_handle<'a, I, O>(
        &mut self,
        handle: SystemHandle<I, O>,
        input: I::Data<'a>,
    ) -> Result<O, SystemError>
    where
        I: SystemInput<Data<'a>: Send> + Send + 'static,
        O: Send + 'static,
    {
        self.flush();

        let Some(mut slot) = self.system_cache.remove(handle) else {
            return Err(SystemError::Unregistered(handle.id()));
        };

        let nonsend = slot.system.flags().intersects(SystemFlags::NON_SEND);

        let func = || {
            #[cfg(feature = "trace")]
            let _span = slot.span.enter();
            slot.system.initialize(self);
            slot.system.set_last_run(self.last_run);
            let world = self.cell();
            let result = unsafe { slot.system.run_raw(input, world) };
            slot.system.apply_deferred(self);
            result
        };

        let result = if nonsend {
            zlim_task::invoke_on_main(func)
        } else {
            func()
        };

        if self.system_cache.insert(handle.id, slot).is_some() {
            ::core::hint::cold_path();
            log::warn!("The same System `{handle:?}` was inserted during the execution.");
        }

        result
    }
}

// -----------------------------------------------------------------------------

impl NonSendWorld {
    /// Runs a system with the given input without caching it.
    ///
    /// A fresh system instance is built, initialized, executed once, and
    /// discarded — internal state (e.g. `Local` parameters) does not
    /// persist between calls.
    ///
    /// Use [`NonSendWorld::invoke`] when state should survive across runs.
    ///
    /// Because we have `NonSendWorld`, the system input and output type does not need `Send`.
    ///
    /// # Change detection
    ///
    /// Systems invoked through `invoke` directly follow the `World`'s own
    /// change detection: before each run the instance's baseline is reset
    /// to [`World::last_run`], so it observes **every** change since the
    /// world baseline — not just changes since its previous run. Advance the
    /// world baseline with [`World::clear_trackers`] to control what direct
    /// invocations report as changed.
    #[inline]
    pub fn invoke_once<'a, I, O, M>(
        &mut self,
        system: impl IntoSystem<I, O, M> + 'static,
        input: I::Data<'a>,
    ) -> Result<O, SystemError>
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        self.flush();

        let mut system = IntoSystem::into_system(system);

        #[cfg(feature = "trace")]
        let _span = zlim_log::info_span!(parent: None, "system", name=?system.id()).entered();

        system.initialize(self);
        system.set_last_run(self.last_run);
        let result = unsafe { system.run_raw(input, self.cell()) };
        system.apply_deferred(self);

        result
    }

    /// Runs the given system, caching its instance for later runs.
    ///
    /// If a system with the same [`SystemId`] is already cached, that cached
    /// instance is used, so its internal state (e.g. `Local` parameters)
    /// persists across runs; otherwise a fresh instance is built and
    /// cached.  The instance is (re)initialized and executed against the
    /// world, then put back into the cache.
    ///
    /// Use [`NonSendWorld::invoke_once`] for an uncached single-shot run.
    ///
    /// Because we have `NonSendWorld`, the system input and output type does not need `Send`.
    ///
    /// # Change detection
    ///
    /// Systems invoked through `invoke` directly follow the `World`'s own
    /// change detection: before each run the cached instance's baseline is
    /// reset to [`World::last_run`], so it observes **every** change since
    /// the world baseline — not just changes since its previous run. Advance
    /// the world baseline with [`World::clear_trackers`] to control what
    /// direct invocations report as changed.
    ///
    /// [`SystemId`]: crate::system::SystemId
    #[inline]
    pub fn invoke<'a, I, O, M>(
        &mut self,
        system: impl IntoSystem<I, O, M> + 'static,
        input: I::Data<'a>,
    ) -> Result<O, SystemError>
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        self.flush();

        let handle = system.system_handle();

        let mut slot = self
            .system_cache
            .remove(handle)
            .unwrap_or_else(|| SystemSlot::<I, O> {
                system: Box::new(IntoSystem::into_system(system)),
                #[cfg(feature = "trace")]
                span: zlim_log::info_span!(parent: None, "system", name=?handle.id),
            });

        #[cfg(feature = "trace")]
        let _span = slot.span.enter();

        slot.system.initialize(self);
        slot.system.set_last_run(self.last_run);
        let world = self.cell();
        let result = unsafe { slot.system.run_raw(input, world) };
        slot.system.apply_deferred(self);

        #[cfg(feature = "trace")]
        ::core::mem::drop(_span);

        if self.system_cache.insert(handle.id, slot).is_some() {
            ::core::hint::cold_path();
            log::warn!("The same System `{handle:?}` was inserted during the execution.");
        }

        result
    }

    /// Runs the cached system identified by `handle` with the given input.
    ///
    /// The system must have been cached first via [`World::insert_system`];
    /// returns [`SystemError::Unregistered`] otherwise.  The cached instance
    /// is (re)initialized and executed against the world, then put back into
    /// the cache, so its internal state persists between runs.
    ///
    /// Because we have `NonSendWorld`, the system input and output type does not need `Send`.
    ///
    /// # Change detection
    ///
    /// Systems invoked through `invoke` directly follow the `World`'s own
    /// change detection: before each run the cached instance's baseline is
    /// reset to [`World::last_run`], so it observes **every** change since
    /// the world baseline — not just changes since its previous run. Advance
    /// the world baseline with [`World::clear_trackers`] to control what
    /// direct invocations report as changed.
    ///
    /// [`SystemError::Unregistered`]: crate::system::SystemError::Unregistered
    #[inline]
    pub fn invoke_handle<'a, I, O>(
        &mut self,
        handle: SystemHandle<I, O>,
        input: I::Data<'a>,
    ) -> Result<O, SystemError>
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        self.flush();

        let Some(mut slot) = self.system_cache.remove(handle) else {
            return Err(SystemError::Unregistered(handle.id()));
        };

        #[cfg(feature = "trace")]
        let _span = slot.span.enter();

        slot.system.initialize(self);
        slot.system.set_last_run(self.last_run);
        let world = self.cell();
        let result = unsafe { slot.system.run_raw(input, world) };
        slot.system.apply_deferred(self);

        #[cfg(feature = "trace")]
        ::core::mem::drop(_span);

        if self.system_cache.insert(handle.id, slot).is_some() {
            ::core::hint::cold_path();
            log::warn!("The same System `{handle:?}` was inserted during the execution.");
        }

        result
    }
}
