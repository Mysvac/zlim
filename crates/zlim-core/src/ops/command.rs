//! Deferred command application (`apply_commands` / `flush`) on `World`.

use crate::command::{Commands, EntityCommands};
use crate::ops::{Entity, EntityOwned};
use crate::world::{DeferredWorld, World, WorldCell};

impl World {
    /// Drains and executes queued deferred commands.
    ///
    /// Currently, this function simply drains and executes the world's
    /// queued commands; additional behaviors may be added in the future.
    #[inline]
    pub fn flush(&mut self) {
        if self.command_start < self.command_queue.len() {
            ::core::hint::cold_path();
            crate::command::flush_world(self); // not inline
        }
    }

    /// Returns a [`Commands`] interface bound to this world and command queue.
    ///
    /// Commands are enqueued first and applied later by the normal flush path.
    #[inline]
    pub fn commands(&mut self) -> Commands<'_, '_> {
        let cell = self.cell();
        let world = unsafe { cell.read_only() };
        let queue = unsafe { &mut cell.data_mut().command_queue };
        Commands::new(world, queue)
    }
}

impl DeferredWorld<'_> {
    /// Returns a [`Commands`] interface bound to this world and command queue.
    ///
    /// Commands are enqueued first and applied later by the normal flush path.
    #[inline]
    pub fn commands(&mut self) -> Commands<'_, '_> {
        let cell = self.cell();
        let world = unsafe { cell.read_only() };
        let queue = unsafe { &mut cell.data_mut().command_queue };
        Commands::new(world, queue)
    }
}

impl EntityOwned<'_> {
    /// Returns a [`Commands`] interface bound to this world and command queue.
    ///
    /// Commands are enqueued first and applied later by the normal flush path.
    #[inline]
    pub fn commands(&mut self) -> Commands<'_, '_> {
        let cell: WorldCell<'_> = self.world;
        let world = unsafe { cell.read_only() };
        let queue = unsafe { &mut cell.data_mut().command_queue };
        Commands::new(world, queue)
    }
}

impl Entity<'_> {
    /// Returns an [`EntityCommands`] interface bound to this world and command queue.
    ///
    /// Commands are enqueued first and applied later by the normal flush path.
    #[inline]
    pub fn commands(&mut self) -> EntityCommands<'_> {
        use core::mem::transmute;

        unsafe {
            let id = self.id;
            let cell: WorldCell<'_> = self.world;
            let world = cell.read_only();
            let queue = &mut cell.data_mut().command_queue;
            transmute::<EntityCommands, EntityCommands>(Commands::new(world, queue).with_entity(id))
        }
    }
}
