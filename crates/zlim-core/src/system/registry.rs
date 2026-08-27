//! The world's system cache: [`SystemHandle`]s and the [`Systems`] registry
//! that stores cached system instances keyed by their [`SystemId`].

use core::any::Any;
use core::any::TypeId;
use core::fmt::Debug;
use core::hash::Hash;
use core::marker::PhantomData;

use zlim_utils::ext::TypeMap;
use zlim_utils::hash::map::Entry;
use zlim_utils::hash::{HashMap, NoopState};

use crate::system::System;
use crate::system::SystemId;
use crate::system::SystemInput;
use crate::utils::DebugCheckedUnwrap;

// -----------------------------------------------------------------------------
// SystemHandle

/// A cheap, type-safe handle to a system cached in the world.
///
/// Obtained from [`World::insert_system`] and passed to
/// [`World::invoke_handle`].  The `I`/`O` type parameters tie the
/// handle to the system's input/output signature so cache lookups stay
/// type-checked.
///
/// Handles are `Copy` and compare by the underlying [`SystemId`].
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// fn tick() {}
///
/// let mut world = World::alloc();
///
/// // Insert the system once, then run it through its handle.
/// let handle = world.insert_system(tick);
/// assert_eq!(handle.id(), tick.system_id());
/// world.invoke_handle(handle, ()).unwrap();
///
/// // Handles are `Copy`, so the same handle can be reused.
/// world.invoke_handle(handle, ()).unwrap();
/// ```
///
/// [`World::insert_system`]: crate::world::World::insert_system
/// [`World::invoke_handle`]: crate::world::World::invoke_handle
pub struct SystemHandle<I, O> {
    id: SystemId,
    _marker: PhantomData<(I, O)>,
}

unsafe impl<I, O> Sync for SystemHandle<I, O> {}
unsafe impl<I, O> Send for SystemHandle<I, O> {}

impl<I, O> Hash for SystemHandle<I, O> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.id.type_id().hash(state);
    }
}

impl<I, O> Debug for SystemHandle<I, O> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&self.id, f)
    }
}

impl<I, O> PartialEq for SystemHandle<I, O> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<I, O> Eq for SystemHandle<I, O> {}

impl<I, O> Copy for SystemHandle<I, O> {}

impl<I, O> Clone for SystemHandle<I, O> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<I, O> SystemHandle<I, O> {
    /// Returns the underlying [`SystemId`].
    #[inline(always)]
    pub const fn id(self) -> SystemId {
        self.id
    }
}

impl<I, O> From<SystemHandle<I, O>> for SystemId {
    #[inline(always)]
    fn from(value: SystemHandle<I, O>) -> Self {
        value.id
    }
}

impl<I: SystemInput, O> SystemHandle<I, O> {
    /// Create a handle from given [`SystemId`].
    #[inline(always)]
    pub const fn new(id: SystemId) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }
}

impl<I: SystemInput, O> From<SystemId> for SystemHandle<I, O> {
    #[inline(always)]
    fn from(id: SystemId) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }
}

impl<I, O> SystemHandle<I, O>
where
    I: SystemInput + 'static,
    O: 'static,
{
    /// Create a handle from given [`System`].
    #[inline(always)]
    pub fn from_system(system: &dyn System<Input = I, Output = O>) -> Self {
        let id = system.id();
        Self {
            id,
            _marker: PhantomData,
        }
    }
}

// -----------------------------------------------------------------------------
// Systems

type SystemMap<I, O> = HashMap<SystemId, Option<Box<dyn System<Input = I, Output = O>>>, NoopState>;
type SystemBox<I, O> = Box<dyn System<Input = I, Output = O>>;

/// Type-erased cache of system instances, keyed by their [`SystemId`].
///
/// Each concrete `(I, O)` signature gets its own internal map, stored behind
/// a [`TypeId`]-keyed entry, so a single `Systems` value can hold systems
/// with different input/output types.  Every [`World`] owns one.
///
/// [`World`]: crate::world::World
pub(crate) struct SystemCache {
    mapper: TypeMap<Box<dyn Any + Send + Sync + 'static>>,
}

impl Debug for SystemCache {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SystemCache").finish_non_exhaustive()
    }
}

impl SystemCache {
    pub(crate) const fn new() -> Self {
        Self {
            mapper: TypeMap::new(),
        }
    }
}

impl SystemCache {
    #[inline]
    fn with<I, O>(&mut self) -> &mut SystemMap<I, O>
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        match self.mapper.entry(TypeId::of::<SystemHandle<I, O>>()) {
            Entry::Occupied(entry) => {
                let erased = entry.into_mut();
                unsafe { erased.downcast_mut().debug_checked_unwrap() }
            }
            Entry::Vacant(entry) => {
                let map = SystemMap::<I, O>::with_hasher(NoopState);
                let erased = entry.insert(Box::new(map));
                unsafe { erased.downcast_mut().debug_checked_unwrap() }
            }
        }
    }

    #[inline]
    fn try_with<I, O>(&mut self) -> Option<&mut SystemMap<I, O>>
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        let erased = self.mapper.get_mut(TypeId::of::<SystemHandle<I, O>>())?;
        unsafe { Some(erased.downcast_mut().debug_checked_unwrap()) }
    }

    /// Returns the cache slot for `handle`, creating the per-signature map
    /// if needed.
    #[inline(never)]
    pub fn entry<I, O>(&mut self, handle: SystemHandle<I, O>) -> &mut Option<SystemBox<I, O>>
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        self.with::<I, O>().entry(handle.id).or_default()
    }

    /// Caches `system`, returning the previously cached instance with the
    /// same id, if any.
    #[inline(never)]
    pub fn insert<I, O>(&mut self, system: SystemBox<I, O>) -> Option<SystemBox<I, O>>
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        let id = system.id();
        self.with::<I, O>().insert(id, Some(system))?
    }

    /// Removes and returns the cached instance for `handle`, if present.
    #[inline(never)]
    pub fn remove<I, O>(&mut self, handle: SystemHandle<I, O>) -> Option<SystemBox<I, O>>
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        self.try_with::<I, O>()?.remove(&handle.id)?
    }
}

// -----------------------------------------------------------------------------
