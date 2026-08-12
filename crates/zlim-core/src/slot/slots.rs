use core::any::TypeId;
use core::cell::UnsafeCell;
use core::fmt::Debug;
use core::iter::FusedIterator;
use core::panic::{RefUnwindSafe, UnwindSafe};

use zlim_utils::ext::TypeMap;
use zlim_utils::mem::Global;

use super::Slot;
use crate::resource::{ResourceId, Resources};
use crate::utils::DebugCheckedUnwrap;

// -----------------------------------------------------------------------------
// ResourceSlots
// -----------------------------------------------------------------------------

/// A collection of all resources in the world.
///
/// Resources are stored in a sparse vector indexed by [`ResourceId`], with
/// lazy initialisation: each slot is `None` until the corresponding type is
/// first inserted.  A secondary [`TypeMap`] enables O(1) lookup by
/// [`TypeId`], which is the hot-path used by
/// `get_resource::<T>()` / `insert_resource`.
///
/// # Thread safety
///
/// Resources are stored behind `UnsafeCell` so that access discipline can
/// be enforced at a higher level.  `Send` and `NonSend` resources share the
/// same storage; the caller is responsible for enforcing thread-safety
/// invariants.
pub struct ResourceSlots {
    slots: Vec<Option<&'static UnsafeCell<Slot>>>,
    mapper: TypeMap<&'static UnsafeCell<Slot>>,
}

impl Default for ResourceSlots {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            mapper: TypeMap::new(),
        }
    }
}

impl Debug for ResourceSlots {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list()
            .entries(self.slots.iter().filter_map(Option::as_ref))
            .finish()
    }
}

// -----------------------------------------------------------------------------
// Register
// -----------------------------------------------------------------------------

impl ResourceSlots {
    /// Prepares a slot for the given resource type and returns a mutable
    /// reference to it.
    ///
    /// If the slot already exists it is returned directly; otherwise a new
    /// [`Slot`] is allocated from the resource metadata and stored in a
    /// process-lifetime ([`Global`]) allocation.  The `TypeMap` is updated
    /// for fast [`TypeId`]-based lookup.
    ///
    /// # Safety
    ///
    /// - `ident` must be a valid, registered [`ResourceId`].
    #[inline]
    pub(crate) fn register<'a>(
        &'a mut self,
        resources: &Resources,
        ident: ResourceId,
    ) -> &'a mut Slot {
        if self.slots.len() <= ident.index() {
            ::core::hint::cold_path();
            self.slots.resize_with(ident.index() + 1, || None);
            let cap = self.slots.capacity();
            self.slots.resize_with(cap, || None);
        }

        unsafe {
            let slot = self.slots.get_unchecked_mut(ident.index());
            if slot.is_none() {
                ::core::hint::cold_path();
                let s = Slot::new(resources, ident);
                let ty = s.type_id();
                let cs = UnsafeCell::new(s);
                let scs: &'static UnsafeCell<Slot> = Global::alloc_unchecked(cs);
                *slot = Some(scs);
                self.mapper.insert(ty, scs);
            }
            let s = slot.debug_checked_unwrap();
            &mut *s.get()
        }
    }
}

// -----------------------------------------------------------------------------
// Drop
// -----------------------------------------------------------------------------

impl Drop for ResourceSlots {
    /// Drops and deallocates all active resource slots.
    fn drop(&mut self) {
        for slot in self.slots.iter_mut().flatten() {
            unsafe { (*slot.get()).clear() };
        }
    }
}

// Safety: ResourceSlots mediates all internal Slot access through `&self`
// / `&mut self`, which provides the necessary mutual-exclusion guarantees.
unsafe impl Sync for ResourceSlots {}
unsafe impl Send for ResourceSlots {}

impl UnwindSafe for ResourceSlots {}
impl RefUnwindSafe for ResourceSlots {}

// -----------------------------------------------------------------------------
// Accessors
// -----------------------------------------------------------------------------

impl ResourceSlots {
    /// Returns `true` if no resources are currently active.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    /// Returns the number of active (inserted) resources.
    #[inline]
    pub fn len(&self) -> usize {
        self.slots.iter().filter_map(Option::as_ref).count()
    }

    /// Returns a shared reference to the slot for the given [`ResourceId`],
    /// or `None` if the slot has not been prepared.
    pub fn get(&self, id: ResourceId) -> Option<&Slot> {
        let s = *self.slots.get(id.index())?;
        unsafe { Some(&*(s?.get())) }
    }

    /// Returns a mutable reference to the slot for the given
    /// [`ResourceId`], or `None` if the slot has not been prepared.
    pub fn get_mut(&mut self, id: ResourceId) -> Option<&mut Slot> {
        let s = *self.slots.get_mut(id.index())?;
        unsafe { Some(&mut *(s?.get())) }
    }

    /// Returns a shared reference to the slot for the given [`TypeId`],
    /// or `None` if the resource type has not been prepared.
    ///
    /// This is the hot-path used by `get_resource::<T>()` — it avoids the
    /// intermediate `ResourceId` lookup.
    pub fn get_by_type(&self, id: TypeId) -> Option<&Slot> {
        let slot = *self.mapper.get(id)?;
        unsafe { Some(&*slot.get()) }
    }

    /// Returns a mutable reference to the slot for the given [`TypeId`],
    /// or `None` if the resource type has not been prepared.
    ///
    /// This is the hot-path used by `get_resource_mut::<T>()`.
    pub fn get_by_type_mut(&mut self, id: TypeId) -> Option<&mut Slot> {
        let slot = *self.mapper.get(id)?;
        unsafe { Some(&mut *slot.get()) }
    }

    /// Returns a shared reference to the slot for the given [`ResourceId`]
    /// without bounds checking.
    ///
    /// # Safety
    ///
    /// - The slot must have been prepared via `register`.
    ///
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, id: ResourceId) -> &Slot {
        debug_assert!(id.index() < self.slots.len());
        unsafe {
            let s = *self.slots.get(id.index()).debug_checked_unwrap();
            &*(s.debug_checked_unwrap().get())
        }
    }

    /// Returns a mutable reference to the slot for the given
    /// [`ResourceId`] without bounds checking.
    ///
    /// # Safety
    ///
    /// - The slot must have been prepared via `register`.
    ///
    #[inline(always)]
    pub unsafe fn get_unchecked_mut(&mut self, id: ResourceId) -> &mut Slot {
        debug_assert!(id.index() < self.slots.len());
        unsafe {
            let s = *self.slots.get_mut(id.index()).debug_checked_unwrap();
            &mut *(s.debug_checked_unwrap().get())
        }
    }

    /// Returns an iterator over all active (inserted) resources.
    #[inline]
    pub fn iter(&self) -> impl FusedIterator<Item = &'_ Slot> {
        self.slots
            .iter()
            .filter_map(Option::as_ref)
            .map(|s| unsafe { &*s.get() })
    }

    /// Returns an iterator that yields mutable references to all active
    /// resources.
    #[inline]
    pub fn iter_mut(&mut self) -> impl FusedIterator<Item = &'_ mut Slot> {
        self.slots
            .iter_mut()
            .filter_map(Option::as_mut)
            .map(|s| unsafe { &mut *s.get() })
    }
}
