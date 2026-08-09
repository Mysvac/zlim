use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::ptr::NonNull;

use super::World;

// -----------------------------------------------------------------------------
// WorldCell
// -----------------------------------------------------------------------------

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct WorldCell<'a> {
    world: NonNull<World>,
    _marker: PhantomData<&'a UnsafeCell<World>>,
}

unsafe impl Send for WorldCell<'_> {}
unsafe impl Sync for WorldCell<'_> {}

// -----------------------------------------------------------------------------
// From
// -----------------------------------------------------------------------------

impl<'a> From<&'a World> for WorldCell<'a> {
    /// Creates an [`WorldCell`] from a shared world reference.
    #[inline(always)]
    fn from(value: &'a World) -> Self {
        WorldCell {
            world: NonNull::from_ref(value),
            _marker: PhantomData,
        }
    }
}

impl<'a> From<&'a mut World> for WorldCell<'a> {
    /// Creates an [`WorldCell`] from an exclusive world reference.
    #[inline(always)]
    fn from(value: &'a mut World) -> Self {
        WorldCell {
            world: NonNull::from_mut(value),
            _marker: PhantomData,
        }
    }
}

impl World {
    /// Returns a raw-access handle to this world.
    ///
    /// This does not grant any additional guarantees by itself. Safety must be
    /// enforced by the code that later dereferences the returned handle.
    #[inline(always)]
    pub const fn cell(&self) -> WorldCell<'_> {
        WorldCell {
            world: NonNull::from_ref(self),
            _marker: PhantomData,
        }
    }
}

// -----------------------------------------------------------------------------
// Methods
// -----------------------------------------------------------------------------

impl<'a> WorldCell<'a> {
    /// Reinterprets this handle as a shared world reference.
    ///
    /// # Safety
    /// - Access must remain read-only for the duration of the borrow.
    /// - The caller must ensure no concurrent mutable access that would violate
    ///   Rust aliasing rules.
    #[inline(always)]
    pub const unsafe fn read_only(self) -> &'a World {
        unsafe { &*self.world.as_ptr() }
    }

    /// Reinterprets this handle as a mutable world reference for data mutation.
    ///
    /// This mode exists to express "data mutability" under a stronger contract:
    /// mutate existing values, but do not perform structural world changes.
    ///
    /// # Safety
    /// - The caller must ensure exclusive mutable access according to Rust
    ///   aliasing rules.
    /// - Only data-level mutation is allowed:
    ///   - do not add/remove entities or resources,
    ///   - do not register new types,
    ///   - do not allocate new ids.
    #[inline(always)]
    pub const unsafe fn data_mut(self) -> &'a mut World {
        unsafe { &mut *self.world.as_ptr() }
    }

    /// Reinterprets this handle as a fully mutable world reference.
    ///
    /// Use this when structural mutation is required.
    ///
    /// # Safety
    /// - There must be no other active borrows (shared or mutable) that alias
    ///   this world for the returned lifetime.
    #[inline(always)]
    pub const unsafe fn full_mut(self) -> &'a mut World {
        unsafe { &mut *self.world.as_ptr() }
    }
}

// -----------------------------------------------------------------------------
