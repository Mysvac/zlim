#![expect(clippy::module_inception, reason = "For better structure.")]

use core::alloc::Layout;
use core::any::TypeId;
use core::fmt::Debug;
use core::num::NonZeroUsize;
use core::panic::{RefUnwindSafe, UnwindSafe};
use core::ptr::{self, NonNull};
use std::alloc as malloc;

use zlim_ptr::{OwningPtr, Ptr, PtrMut};

use crate::borrow::{UntypedMut, UntypedRef};
use crate::resource::{Resource, ResourceId, Resources};
use crate::tick::{Tick, TicksMut, TicksRef};
use crate::utils::{DebugCheckedUnwrap, Dropper};

// -----------------------------------------------------------------------------
// AbortOnDropFail
// -----------------------------------------------------------------------------

/// Drop guard that aborts the process if a resource's drop implementation
/// panics during removal.
///
/// This mirrors the behaviour of [`AbortOnPanic`] in the table module.
///
/// [`AbortOnPanic`]: crate::table::AbortOnPanic
struct AbortOnDropFail;

impl Drop for AbortOnDropFail {
    #[cold]
    #[inline(never)]
    fn drop(&mut self) {
        log::error!("Aborting due to drop resource panicked.");
        std::process::abort();
    }
}

// -----------------------------------------------------------------------------
// Slot
// -----------------------------------------------------------------------------

/// Raw storage for a single resource instance.
///
/// Manages memory allocation, initialisation state, and change detection
/// ticks.  The data pointer is `null` when the resource is inactive (not
/// yet inserted, or already removed).
///
/// # Fields
///
/// | Field | Purpose |
/// |-------|---------|
/// | `data` | Heap pointer to the resource value (null if absent).  For ZSTs this is a well-aligned dangling pointer. |
/// | `id` | The registered [`ResourceId`] for this resource type. |
/// | `added` / `changed` | Change-detection ticks. |
/// | `type_id` | Runtime type identity for typed access. |
/// | `name` | Debug name (type name string) for diagnostics. |
/// | `layout` | Memory layout for allocation/deallocation. |
/// | `dropper` | Optional drop function for non-trivial types. |
#[repr(C)]
pub struct Slot {
    data: *mut u8,
    id: ResourceId,
    added: Tick,
    changed: Tick,
    type_id: TypeId,
    name: &'static str,
    layout: Layout,
    dropper: Option<Dropper>,
}

// -----------------------------------------------------------------------------
// Private Construction
// -----------------------------------------------------------------------------

impl Slot {
    /// Creates a new, empty slot from resource metadata.
    ///
    /// The slot starts with a null data pointer; memory is allocated
    /// lazily on the first [`insert_untyped`] call.
    ///
    /// # Safety
    ///
    /// - `id` must be a valid, registered [`ResourceId`].
    ///
    /// [`insert_untyped`]: Self::insert_untyped
    pub(super) unsafe fn new(resources: &Resources, id: ResourceId) -> Self {
        let db = unsafe { resources.get_by_id(id).debug_checked_unwrap() };
        Self {
            id: db.id,
            type_id: db.type_id,
            name: db.typa_name,
            layout: db.layout,
            dropper: db.dropper,
            data: ptr::null_mut(),
            added: Tick::new(0),
            changed: Tick::new(0),
        }
    }
}

// -----------------------------------------------------------------------------
// Basic Traits
// -----------------------------------------------------------------------------

impl Debug for Slot {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Slot")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("present", &self.is_present())
            .finish()
    }
}

// Safety: Slot access is mediated by `ResourceSlots::get`/`get_mut` which
// enforce exclusive-access discipline through `&self`/`&mut self`.
unsafe impl Sync for Slot {}
unsafe impl Send for Slot {}

impl UnwindSafe for Slot {}
impl RefUnwindSafe for Slot {}

// -----------------------------------------------------------------------------
// Accessors
// -----------------------------------------------------------------------------

impl Slot {
    /// Returns the registered [`ResourceId`] of this slot.
    #[inline(always)]
    pub fn id(&self) -> ResourceId {
        self.id
    }

    /// Returns the [`TypeId`] of the resource type stored in this slot.
    #[inline(always)]
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    /// Returns the debug name (type name) of this resource.
    #[inline(always)]
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Clamps the stored ticks to prevent wrap-around from causing false
    /// positives in change detection.
    ///
    /// See [`Tick::clamp`] for details.
    #[inline(always)]
    pub fn clamp_ticks(&mut self, now: Tick) {
        self.added.clamp(now);
        self.changed.clamp(now);
    }

    /// Returns `true` if the resource is currently initialised.
    #[inline(always)]
    pub fn is_present(&self) -> bool {
        !self.data.is_null()
    }

    /// Returns a shared pointer to the resource data, or `None` if the
    /// resource is absent.
    #[inline]
    pub fn get_data(&self) -> Option<Ptr<'_>> {
        unsafe { Some(Ptr::new(NonNull::new(self.data)?)) }
    }

    /// Returns a mutable pointer to the resource data, or `None` if the
    /// resource is absent.
    #[inline]
    pub fn get_data_mut(&mut self) -> Option<PtrMut<'_>> {
        unsafe { Some(PtrMut::new(NonNull::new(self.data)?)) }
    }

    /// Returns the `added` tick, or `None` if the resource is absent.
    #[inline]
    pub fn get_added(&self) -> Option<Tick> {
        if self.is_present() {
            Some(self.added)
        } else {
            None
        }
    }

    /// Returns the `changed` tick, or `None` if the resource is absent.
    #[inline]
    pub fn get_changed(&self) -> Option<Tick> {
        if self.is_present() {
            Some(self.changed)
        } else {
            None
        }
    }
}

// -----------------------------------------------------------------------------
// Borrows — change-aware typed access
// -----------------------------------------------------------------------------

impl Slot {
    /// Returns an untyped shared reference with change-detection metadata,
    /// or `None` if the resource is absent.
    #[inline]
    pub fn get_ref(&self, last_run: Tick, this_run: Tick) -> Option<UntypedRef<'_>> {
        let data = NonNull::new(self.data)?;
        Some(UntypedRef {
            value: unsafe { Ptr::new(data) },
            ticks: TicksRef {
                added: &self.added,
                changed: &self.changed,
                last_run,
                this_run,
            },
        })
    }

    /// Returns an untyped exclusive reference with change-detection
    /// metadata, or `None` if the resource is absent.
    #[inline]
    pub fn get_mut(&mut self, last_run: Tick, this_run: Tick) -> Option<UntypedMut<'_>> {
        let data = NonNull::new(self.data)?;
        Some(UntypedMut {
            value: unsafe { PtrMut::new(data) },
            ticks: TicksMut {
                added: &mut self.added,
                changed: &mut self.changed,
                last_run,
                this_run,
            },
        })
    }
}

// -----------------------------------------------------------------------------
// Insert & Remove
// -----------------------------------------------------------------------------

impl Slot {
    /// Inserts a new resource value from an [`OwningPtr`].
    ///
    /// Allocates heap memory on first insertion.  If a value already
    /// exists, the old value is dropped first.
    ///
    /// # Safety
    ///
    /// - `value` must match the resource's layout.
    /// - `tick` must be a valid epoch tick.
    #[inline(never)]
    pub unsafe fn insert_untyped(&mut self, value: OwningPtr<'_>, tick: Tick) {
        if let Some(data) = NonNull::new(self.data) {
            if let Some(dropper) = self.dropper {
                let guard = AbortOnDropFail;
                unsafe {
                    dropper.call(OwningPtr::new(data));
                }
                ::core::mem::forget(guard);
            }
        } else {
            let layout = self.layout;
            if layout.size() == 0 {
                let align = NonZeroUsize::new(layout.align()).unwrap();
                self.data = NonNull::without_provenance(align).as_ptr();
            } else {
                self.data = NonNull::new(unsafe { malloc::alloc(layout) })
                    .unwrap_or_else(|| malloc::handle_alloc_error(layout))
                    .as_ptr();
            };
            self.added = tick;
        }

        unsafe {
            self.changed = tick;
            ptr::copy_nonoverlapping::<u8>(value.as_ptr(), self.data, self.layout.size());
        }
    }

    /// Inserts a new resource value of type `T`.
    ///
    /// # Safety
    ///
    /// - `tick` must be a valid epoch tick.
    pub unsafe fn insert<T: Resource>(&mut self, value: T, tick: Tick) {
        debug_assert_eq!(Layout::new::<T>(), self.layout);
        zlim_ptr::into_owning!(value);
        unsafe { self.insert_untyped(value, tick) };
    }

    /// Drops and deallocates the resource data.
    ///
    /// After calling this, the slot returns to the uninitialised state
    /// (`is_present()` returns `false`).
    ///
    /// # Safety
    ///
    /// If the resource type is `NonSend`, the caller must ensure this
    /// runs on the correct thread.
    pub unsafe fn clear(&mut self) {
        if let Some(data) = NonNull::new(self.data) {
            let guard = AbortOnDropFail;
            unsafe {
                if let Some(dropper) = self.dropper {
                    dropper.call(OwningPtr::new(data));
                }
                if self.layout.size() != 0 {
                    malloc::dealloc(self.data, self.layout);
                }
                self.data = ptr::null_mut();
            }
            ::core::mem::forget(guard);
        }
    }

    /// Removes the resource and returns ownership of its data.
    ///
    /// Returns `None` if the resource is not active.
    ///
    /// After removal, the slot returns to the uninitialised state.
    ///
    /// # Safety
    ///
    /// - `T` must match the resource's layout.
    pub unsafe fn remove<T: Resource>(&mut self) -> Option<T> {
        if self.data.is_null() {
            return None;
        }

        let ret = unsafe { ptr::read::<T>(self.data as *mut T) };

        if self.layout.size() != 0 {
            unsafe { malloc::dealloc(self.data, self.layout) };
        }
        self.data = ptr::null_mut();

        Some(ret)
    }
}
