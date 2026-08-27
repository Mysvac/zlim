//! Running systems directly against a [`World`].
//!
//! Systems can be executed outside the schedule machinery in two ways:
//!
//! - [`World::invoke_once`] — builds a fresh system instance,
//!   initializes it, runs it once, and discards it.
//! - [`World::insert_system`] + [`World::invoke_handle`] — inserts a
//!   system into the world's cache once, so its internal state (e.g.
//!   `Local` parameters and change-detection baselines) persists across
//!   runs.  [`World::invoke`] combines cache lookup-or-insert with a
//!   single run.
//!
//! [`World`]: crate::world::World

use crate::system::IntoSystem;
use crate::system::System;
use crate::system::SystemError;
use crate::system::SystemFlags;
use crate::system::SystemHandle;
use crate::system::SystemInput;
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
            *entry = Some(Box::new(IntoSystem::into_system(system)));
            // Deferred — the instance is initialized on its first run.
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
        self.system_cache.remove(handle)
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
        let mut system = IntoSystem::into_system(system);
        let nonsend = system.flags().intersects(SystemFlags::NON_SEND);

        let func = || {
            system.initialize(self);
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
        let handle = system.system_handle();

        let mut system = self
            .system_cache
            .remove(handle)
            .unwrap_or_else(|| Box::new(IntoSystem::into_system(system)));

        let nonsend = system.flags().intersects(SystemFlags::NON_SEND);

        let func = || {
            system.initialize(self);
            let world = self.cell();
            let result = unsafe { system.run_raw(input, world) };
            system.apply_deferred(self);
            result
        };

        let result = if nonsend {
            zlim_task::invoke_on_main(func)
        } else {
            func()
        };

        if self.system_cache.insert(system).is_some() {
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
        let Some(mut system) = self.system_cache.remove(handle) else {
            return Err(SystemError::Unregistered(handle.id()));
        };

        let nonsend = system.flags().intersects(SystemFlags::NON_SEND);

        let func = || {
            system.initialize(self);
            let world = self.cell();
            let result = unsafe { system.run_raw(input, world) };
            system.apply_deferred(self);
            result
        };

        let result = if nonsend {
            zlim_task::invoke_on_main(func)
        } else {
            func()
        };

        if self.system_cache.insert(system).is_some() {
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
    /// Each run resets the system's change-detection baseline to the world's
    /// own [`World::last_run`] tick before executing, so the system observes
    /// **every** change since the world baseline — not just changes since its
    /// previous run.  Advance the world baseline with [`World::clear_trackers`]
    /// to control what direct invocations report as changed.
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
        let mut system = IntoSystem::into_system(system);

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
    /// Each run resets the system's change-detection baseline to the world's
    /// own [`World::last_run`] tick before executing, so the system observes
    /// **every** change since the world baseline — not just changes since its
    /// previous run (even though the instance is cached).  Advance the world
    /// baseline with [`World::clear_trackers`] to control what direct
    /// invocations report as changed.
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
        let handle = system.system_handle();

        let mut system = self
            .system_cache
            .remove(handle)
            .unwrap_or_else(|| Box::new(IntoSystem::into_system(system)));

        system.initialize(self);
        system.set_last_run(self.last_run);
        let result = unsafe { system.run_raw(input, self.cell()) };
        system.apply_deferred(self);

        if self.system_cache.insert(system).is_some() {
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
    /// Each run resets the system's change-detection baseline to the world's
    /// own [`World::last_run`] tick before executing, so the system observes
    /// **every** change since the world baseline — not just changes since its
    /// previous run (even though the instance is cached).  Advance the world
    /// baseline with [`World::clear_trackers`] to control what direct
    /// invocations report as changed.
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
        let Some(mut system) = self.system_cache.remove(handle) else {
            return Err(SystemError::Unregistered(handle.id()));
        };

        system.initialize(self);
        system.set_last_run(self.last_run);
        let result = unsafe { system.run_raw(input, self.cell()) };
        system.apply_deferred(self);

        if self.system_cache.insert(system).is_some() {
            ::core::hint::cold_path();
            log::warn!("The same System `{handle:?}` was inserted during the execution.");
        }

        result
    }
}
