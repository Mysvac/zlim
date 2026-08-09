use core::ops::Deref;

use super::{World, WorldCell};

// -----------------------------------------------------------------------------
// DeferredWorld
// -----------------------------------------------------------------------------

pub struct DeferredWorld<'w>(WorldCell<'w>);

impl<'w> Deref for DeferredWorld<'w> {
    type Target = World;

    fn deref(&self) -> &Self::Target {
        unsafe { self.0.read_only() }
    }
}

// -----------------------------------------------------------------------------
// From
// -----------------------------------------------------------------------------

impl<'w> From<&'w mut World> for DeferredWorld<'w> {
    fn from(value: &'w mut World) -> Self {
        DeferredWorld(value.cell())
    }
}

impl World {
    /// Creates a [`DeferredWorld`] view from `&mut World`.
    ///
    /// This is the ergonomic entry point for deferred command-oriented flows.
    #[inline(always)]
    pub const fn deferred(&mut self) -> DeferredWorld<'_> {
        DeferredWorld(self.cell())
    }
}

impl<'w> WorldCell<'w> {
    /// Reinterprets this unsafe world view as a [`DeferredWorld`].
    ///
    /// # Safety
    ///
    /// Caller must uphold the aliasing and lifetime guarantees required by
    /// [`WorldCell`].
    #[inline(always)]
    pub const unsafe fn deferred(self) -> DeferredWorld<'w> {
        DeferredWorld(self)
    }
}

// -----------------------------------------------------------------------------
// Methods
// -----------------------------------------------------------------------------

impl<'w> DeferredWorld<'w> {
    #[inline(always)]
    pub const fn reborrow(&mut self) -> DeferredWorld<'_> {
        DeferredWorld(self.0)
    }
}
