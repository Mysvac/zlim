//! [`WorldCell`] — a `Copy`-able, unsafe raw handle to a [`World`].
//!
//! [`WorldCell`] provides an escape hatch for performance-sensitive internals
//! that need to temporarily split access patterns in ways that the borrow
//! checker cannot express (e.g. reading component metadata while mutating a
//! cached entity handle).
//!
//! # Access levels
//!
//! Three explicit, `unsafe` access levels model the ECS world's internal
//! invariants:
//!
//! | Method | Level | Allowed mutations |
//! |--------|-------|-------------------|
//! | [`read_only`] | Shared read | None |
//! | [`data_mut`] | Data-only mutation | Modify existing component/resource values; no structural changes |
//! | [`full_mut`] | Full mutation | Add/remove entities/resources, register types, allocate IDs |
//!
//! These are `unsafe` because misuse can violate Rust aliasing rules or
//! ECS world invariants.
//!
//! [`read_only`]: WorldCell::read_only
//! [`data_mut`]: WorldCell::data_mut
//! [`full_mut`]: WorldCell::full_mut

use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::ptr::NonNull;

use super::World;

// -----------------------------------------------------------------------------
// WorldCell
// -----------------------------------------------------------------------------

/// A copyable raw handle to [`World`] with manually enforced borrow rules.
///
/// `WorldCell` is used in performance-sensitive internals where temporarily
/// splitting access patterns is necessary (for example: read-only world access
/// plus mutable access to cached state). It behaves like an unchecked pointer:
/// the compiler can no longer enforce aliasing and thread-safety rules for you.
///
/// # Access Modes
///
/// `WorldCell` exposes three explicit access modes:
///
/// - [`Self::read_only`]: shared world access for read paths.
///   Typical use: inspect metadata while mutating separate local caches.
///
/// - [`Self::data_mut`]: mutable access for data-only updates without structural
///   changes.
///   Typical use: mutate existing component/resource values under externally
///   guaranteed disjointness.
///
/// - [`Self::full_mut`]: fully mutable access including structural mutations.
///   Typical use: add/remove entities/resources, register types.
///
/// The exposed methods are `unsafe` because the caller must uphold the borrow
/// invariants required by Rust and by ECS world semantics.
///
/// # Examples
///
/// ```ignore
/// use zlim_core::prelude::*;
///
/// let mut world = World::alloc();
/// let entity = world.spawn_empty(None).id();
///
/// // `WorldCell` is `Copy` — cheap to pass around and store.
/// let cell = world.cell();
///
/// // Shared read access.
/// // SAFETY: no other borrow of `world` is alive.
/// let read = unsafe { cell.read_only() };
/// assert!(read.entities().contains(entity));
///
/// // Data-only mutable access (no structural changes).
/// // SAFETY: `world` is exclusively borrowed for the duration of this call.
/// {
///     let _data = unsafe { cell.data_mut() };
/// }
///
/// // Full mutable access, including structural mutations.
/// // SAFETY: no other borrows of `world` are alive.
/// unsafe { cell.full_mut() }.despawn(entity).unwrap();
/// ```
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
    /// Creates a [`WorldCell`] from a shared world reference.
    #[inline(always)]
    fn from(value: &'a World) -> Self {
        WorldCell {
            world: NonNull::from_ref(value),
            _marker: PhantomData,
        }
    }
}

impl<'a> From<&'a mut World> for WorldCell<'a> {
    /// Creates a [`WorldCell`] from an exclusive world reference.
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
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use zlim_core::prelude::*;
    ///
    /// let world = World::alloc();
    /// let cell = world.cell();
    ///
    /// // SAFETY: the returned reference is only used for reads.
    /// let read = unsafe { cell.read_only() };
    /// println!("{} entities", read.entity_count());
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use zlim_core::prelude::*;
    ///
    /// #[derive(Component, TypePath, Clone)]
    /// struct Health(f32);
    ///
    /// let mut world = World::alloc();
    /// let entity = world.spawn((Health(100.0),), None).id();
    /// let cell = world.cell();
    ///
    /// // SAFETY: exclusive access; only existing component values are mutated.
    /// let world = unsafe { cell.data_mut() };
    /// world.entity_mut(entity).get_mut::<Health>().unwrap().0 -= 10.0;
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let cell = world.cell();
    ///
    /// // SAFETY: `world` is exclusively borrowed for the duration of this call.
    /// let entity = unsafe { cell.full_mut() }.spawn_empty(None);
    /// assert!(entity.is_spawned());
    /// ```
    #[inline(always)]
    pub const unsafe fn full_mut(self) -> &'a mut World {
        unsafe { &mut *self.world.as_ptr() }
    }
}

// -----------------------------------------------------------------------------
