//! [`DeferredWorld`] — a restricted world handle for command/deferred
//! mutation workflows.
//!
//! While `&mut World` gives full mutable access, many ECS hooks and
//! operations only need read access plus the ability to enqueue deferred
//! structural changes.  [`DeferredWorld`] wraps [`WorldCell`] behind a
//! `Deref<Target = World>` facade, providing convenient read access while
//! still allowing limited mutable operations through its own API.
//!
//! [`WorldCell`]: super::WorldCell

use core::ops::Deref;

use super::{World, WorldCell};

// -----------------------------------------------------------------------------
// DeferredWorld
// -----------------------------------------------------------------------------

/// A restricted mutable world handle for deferred mutation workflows.
///
/// `DeferredWorld` is designed for contexts where you need to:
///
/// - read world data immediately
/// - enqueue structural changes through `Commands`
/// - perform limited direct mutable access to entities/resources
///
/// Conceptually, it wraps an [`WorldCell`] and exposes an API surface that is
/// convenient for command/deferred execution paths.
///
/// Key properties:
///
/// - Dereferences to `&World` for read-only operations.
///
/// - Can still obtain selected mutable handles (for example, resource/entity
///   accessors and command queue writes).
///
/// - Works well in places where query initialization or full structural setup
///   should happen earlier on `&mut World`, while runtime code only consumes
///   pre-registered state.
#[repr(transparent)]
pub struct DeferredWorld<'w>(WorldCell<'w>);

// -----------------------------------------------------------------------------
// Deref
// -----------------------------------------------------------------------------

impl<'w> Deref for DeferredWorld<'w> {
    type Target = World;
    #[inline(always)]
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
    /// Returns a raw-access [`WorldCell`] handle to this world.
    ///
    /// This is useful when a caller needs to pass the world through an
    /// interface that accepts [`WorldCell`] rather than
    /// [`DeferredWorld`].
    #[inline(always)]
    pub const fn cell(&self) -> WorldCell<'_> {
        self.0
    }

    /// Creates a shorter-lived reborrow of this deferred world handle.
    ///
    /// Useful when splitting borrows across helper calls.
    #[inline(always)]
    pub const fn reborrow(&mut self) -> DeferredWorld<'_> {
        DeferredWorld(self.0)
    }
}
