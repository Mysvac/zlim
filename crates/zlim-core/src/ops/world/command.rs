use core::ptr::NonNull;

use crate::world::World;

impl World {
    /// Drains and executes queued deferred commands.
    ///
    /// Each command failure is forwarded to the active default error handler,
    /// and command application continues with remaining commands.
    #[inline]
    pub fn apply_commands(&mut self) {
        unsafe {
            let world = Some(NonNull::from_mut(self));
            self.command_queue.raw().apply_or_drop(world);
        }
    }

    /// Drains and executes queued deferred commands.
    ///
    /// Currently, this function is equivalent to [`World::apply_commands`],
    /// but additional behaviors may be added in the future.
    #[inline]
    pub fn flush(&mut self) {
        self.apply_commands();
    }
}
