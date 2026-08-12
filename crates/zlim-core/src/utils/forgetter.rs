use zlim_utils::debug::DebugLocation;

use crate::entity::EntityId;
use crate::world::WorldCell;

/// RAII guard that forgets an entity if unwinding crosses a critical mutation.
///
/// World mutation paths create this guard before partially-committed operations.
/// If a panic occurs before the guard is forgotten, `Drop` triggers
/// `World::forget_with_caller` to keep world indices/storage in a recoverable
/// state.
pub(crate) struct ForgetEntityOnPanic<'a> {
    /// The entity to forget if a panic occurs during mutation.
    pub entity: EntityId,
    /// Raw world handle used to drive the forget operation.
    pub world: WorldCell<'a>,
    /// Debug call-site information for diagnostics.
    pub caller: DebugLocation,
}

impl Drop for ForgetEntityOnPanic<'_> {
    #[cold]
    #[inline(never)]
    fn drop(&mut self) {
        unsafe {
            let world = self.world.full_mut();
            world.forget_with_caller(self.entity, self.caller);
        }
    }
}
