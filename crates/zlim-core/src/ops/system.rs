//! Running systems directly against a [`World`].
//!
//! Systems can be executed outside the schedule machinery in two ways:
//!
//! - [`World::run_once`] — builds a fresh system instance,
//!   initializes it, runs it once, and discards it.
//! - [`World::insert_system`] + [`World::run_system_handle`] — inserts a
//!   system into the world's cache once, so its internal state (e.g.
//!   `Local` parameters and change-detection baselines) persists across
//!   runs.  [`World::run_system`] combines cache lookup-or-insert with a
//!   single run.
//!
//! [`World`]: crate::world::World

use crate::system::IntoSystem;
use crate::system::System;
use crate::system::SystemError;
use crate::system::SystemHandle;
use crate::system::SystemInput;
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
    /// [`World::run_system_handle`] calls.
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
        let entry = self.systems.entry(handle);

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
        self.systems.remove(handle)
    }

    /// Runs a system with the given input without caching it.
    ///
    /// A fresh system instance is built, initialized, executed once, and
    /// discarded — internal state (e.g. `Local` parameters) does not
    /// persist between calls.  Use [`World::insert_system`] +
    /// [`World::run_system_handle`] when state should survive across runs.
    #[inline]
    pub fn run_once<I, O, M>(
        &mut self,
        system: impl IntoSystem<I, O, M> + 'static,
        input: I::Data<'_>,
    ) -> Result<O, SystemError>
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        IntoSystem::into_system(system).run(input, self)
    }

    /// Runs the given system, caching its instance for later runs.
    ///
    /// If a system with the same [`SystemId`] is already cached, that cached
    /// instance is used, so its internal state (e.g. `Local` parameters)
    /// persists across runs; otherwise a fresh instance is built and
    /// cached.  The instance is (re)initialized and executed against the
    /// world, then put back into the cache.
    ///
    /// Use [`World::run_once`] for an uncached single-shot run.
    ///
    /// [`SystemId`]: crate::system::SystemId
    #[inline]
    pub fn run_system<I, O, M>(
        &mut self,
        system: impl IntoSystem<I, O, M> + 'static,
        input: I::Data<'_>,
    ) -> Result<O, SystemError>
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        let handle = system.system_handle();

        let mut system = self
            .systems
            .remove(handle)
            .unwrap_or_else(|| Box::new(IntoSystem::into_system(system)));

        let result = system.run(input, self);

        if self.systems.insert(system).is_some() {
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
    /// [`SystemError::Unregistered`]: crate::system::SystemError::Unregistered
    #[inline]
    pub fn run_system_handle<I, O>(
        &mut self,
        handle: SystemHandle<I, O>,
        input: I::Data<'_>,
    ) -> Result<O, SystemError>
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        let Some(mut system) = self.systems.remove(handle) else {
            return Err(SystemError::Unregistered(handle.id()));
        };

        let result = system.run(input, self);

        if self.systems.insert(system).is_some() {
            ::core::hint::cold_path();
            log::warn!("The same System `{handle:?}` was inserted during the execution.");
        }

        result
    }
}

// -----------------------------------------------------------------------------
