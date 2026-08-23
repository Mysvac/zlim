use zlim_utils::debug::DebugLocation;

use crate::entity::EntityId;
use crate::world::WorldCell;

/// RAII guard that forgets an entity if unwinding crosses a critical mutation.
///
/// World mutation paths create this guard before partially-committed
/// operations.  `Drop` always performs the forget — it fires on **any** drop
/// (normal scope exit, `?` early return, or unwinding), not just on panic.
/// On the success path the caller must disarm the guard with
/// [`core::mem::forget`] once the mutation has fully committed; otherwise the
/// entity is forgotten even though the operation succeeded.
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
