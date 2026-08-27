//! Single-resource storage slot and the per-world [`Resources`] container.

use core::alloc::Layout;
use core::any::TypeId;
use core::cell::UnsafeCell;
use core::fmt::{Debug, Formatter};
use core::num::NonZeroUsize;
use core::panic::{RefUnwindSafe, UnwindSafe};
use core::ptr::{self, NonNull};
use std::alloc as malloc;
use zlim_log as log;
use zlim_ptr::{OwningPtr, Ptr, PtrMut};
use zlim_utils::ext::TypeMap;
use zlim_utils::mem::Global;

use super::{Resource, ResourceDB};
use crate::borrow::{UntypedMut, UntypedRef};
use crate::tick::{Tick, TicksMut, TicksRef};
use crate::utils::Dropper;

// -----------------------------------------------------------------------------
// ResourceCell
// -----------------------------------------------------------------------------

/// A single resource slot: runtime type info ([`ResourceDB`]), change
/// detection ticks, and a pointer to the resource data.
///
/// # Slot lifetime
///
/// An interesting design choice is that once a slot for a specific type is
/// allocated, it remains for the entire lifetime of the program. Subsequent
/// resource insertions and removals only modify the internal data pointer
/// within the slot, rather than deallocating the slot itself.
///
/// Therefore resources are stored as `&'static UnsafeCell<ResourceCell>`
/// within the [`Resources`] container. That enables `SystemParam` to cache
/// the static reference directly, eliminating the need for runtime lookups
/// via [`TypeId`] or `ResourceId`.
///
/// # Access
///
/// A slot is *present* when its data pointer is non-null and *absent*
/// otherwise ([`is_present`](Self::is_present) /
/// [`is_absent`](Self::is_absent)).  All access is mediated by
/// [`Resources`], which upholds the shared/exclusive discipline through
/// `&self` / `&mut self`.
///
/// # Safety
///
/// The data pointer is only valid while the slot is present, and raw
/// insertion/removal (`insert_untyped`, `remove`, `clear`, `take_raw`,
/// `from_raw`) requires the caller to uphold the type/layout and (for
/// `NonSend` resources) thread requirements documented on each method.
#[repr(C, align(64))]
pub struct ResourceCell {
    /// Pointer to the resource data; null while the slot is absent.
    data: *mut u8,
    /// Tick at which the resource was last inserted.
    added: Tick,
    /// Tick at which the resource was last mutated.
    changed: Tick,
    /// Static metadata of the resource type.
    datebase: &'static ResourceDB,
    // Cached layout and dropper.
    layout: Layout,
    dropper: Option<Dropper>,
}

// Safety: Cell access is mediated by `Resources::get`/`get_mut` which
// enforce exclusive-access discipline through `&self`/`&mut self`.
unsafe impl Sync for ResourceCell {}
unsafe impl Send for ResourceCell {}

impl UnwindSafe for ResourceCell {}
impl RefUnwindSafe for ResourceCell {}

impl Debug for ResourceCell {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Cell")
            .field("name", &self.datebase.type_name)
            .field("present", &!self.data.is_null())
            .finish()
    }
}

// ---------------------------------------------------------------------
// CTOR

impl ResourceCell {
    #[inline]
    pub(crate) fn new(database: &'static ResourceDB) -> Self {
        Self {
            data: ptr::null_mut(),
            added: Tick::new(0),
            changed: Tick::new(0),
            datebase: database,
            layout: database.layout,
            dropper: database.dropper,
        }
    }
}

// ---------------------------------------------------------------------
// ResourceCell Accessors

impl ResourceCell {
    /// Returns the [`TypeId`] of this resource.
    #[inline(always)]
    pub fn type_id(&self) -> TypeId {
        self.datebase.type_id
    }

    /// Returns the [`ResourceDB`] of this resource.
    #[inline(always)]
    pub fn database(&self) -> &'static ResourceDB {
        self.datebase
    }

    /// Returns `true` if the resource is uninitialised.
    #[inline(always)]
    pub fn is_absent(&self) -> bool {
        self.data.is_null()
    }

    /// Returns `true` if the resource is already initialised.
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
        self.is_present().then_some(self.added)
    }

    /// Returns the `changed` tick, or `None` if the resource is absent.
    #[inline]
    pub fn get_changed(&self) -> Option<Tick> {
        self.is_present().then_some(self.changed)
    }

    /// Clamps the stored ticks to prevent wrap-around.
    ///
    /// See [`Tick::clamp_with`] for details.
    #[inline]
    pub fn clamp_ticks(&mut self, now: Tick) {
        self.added.clamp_with(now);
        self.changed.clamp_with(now);
    }

    /// Try fetch an untyped shared reference with change-detection metadata.
    ///
    /// Return `None` if the resource is absent.
    ///
    /// The returned [`UntypedRef`] carries the data's `added`/`changed`
    /// ticks together with the caller-supplied `last_run` / `this_run`
    /// ticks, enabling change detection.
    #[inline]
    pub fn get_ref(&self, last_run: Tick, this_run: Tick) -> Option<UntypedRef<'_>> {
        let data = NonNull::new(self.data)?;
        let value = unsafe { Ptr::new(data) };
        let ticks = TicksRef {
            added: &self.added,
            changed: &self.changed,
            last_run,
            this_run,
        };
        Some(UntypedRef { value, ticks })
    }

    /// Try fetch an untyped exclusive reference with change-detection.
    ///
    /// Return `None` if the resource is absent.
    ///
    /// The returned [`UntypedMut`] carries the data's `added`/`changed`
    /// ticks together with the caller-supplied `last_run` / `this_run`
    /// ticks, enabling change detection.
    #[inline]
    pub fn get_mut(&mut self, last_run: Tick, this_run: Tick) -> Option<UntypedMut<'_>> {
        let data = NonNull::new(self.data)?;
        let value = unsafe { PtrMut::new(data) };
        let ticks = TicksMut {
            added: &mut self.added,
            changed: &mut self.changed,
            last_run,
            this_run,
        };
        Some(UntypedMut { value, ticks })
    }
}

// ---------------------------------------------------------------------
// Insert & Remove

struct AbortOnDropFail;

impl Drop for AbortOnDropFail {
    #[cold]
    #[inline(never)]
    fn drop(&mut self) {
        log::error!("Aborting due to drop resource panicked.");
        std::process::abort();
    }
}

impl ResourceCell {
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
            self.data = ptr::null_mut(); // Avoid multiple drop after Panic
            let guard = AbortOnDropFail;
            unsafe {
                if let Some(dropper) = self.dropper {
                    dropper.call(OwningPtr::new(data));
                }
                if self.layout.size() != 0 {
                    malloc::dealloc(data.as_ptr(), self.layout);
                }
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
    /// # Panic
    ///
    /// May panic if `TypeId != self.database.type_id`.
    ///
    /// # Safety
    ///
    /// - `T` must match the resource's layout.
    ///
    /// [`World`]: crate::world::World
    pub unsafe fn remove<T: Resource>(&mut self) -> Option<T> {
        if self.data.is_null() {
            return None;
        }

        debug_assert_eq!(TypeId::of::<T>(), self.datebase.type_id);

        let ptr = self.data;
        self.data = ptr::null_mut();

        let ret = unsafe { ptr::read::<T>(ptr as *mut T) };

        if self.layout.size() != 0 {
            unsafe { malloc::dealloc(ptr, self.layout) };
        }

        Some(ret)
    }

    /// Inserts a new resource value of type `T`.
    ///
    /// This is the typed counterpart of [`insert_untyped`]:
    /// the value is moved into the slot's storage.  If a value
    /// already exists it is dropped first.
    ///
    /// # Panic
    ///
    /// May panic if `TypeId != self.database.type_id`.
    ///
    /// # Safety
    ///
    /// - `tick` must be a valid epoch tick.
    /// - `T` must match the resource's type.
    ///
    /// [`insert_untyped`]: Self::insert_untyped
    pub unsafe fn insert<T: Resource>(&mut self, value: T, tick: Tick) {
        debug_assert_eq!(TypeId::of::<T>(), self.datebase.type_id);
        zlim_ptr::into_owning!(value);
        unsafe { self.insert_untyped(value, tick) };
    }

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
        let layout = self.layout;

        if let Some(data) = NonNull::new(self.data) {
            // Only drop, do not dealloc memory, do not reset pointers.
            if let Some(dropper) = self.dropper {
                let guard = AbortOnDropFail;
                unsafe { dropper.call(OwningPtr::new(data)) };
                ::core::mem::forget(guard);
            }
        } else {
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
            ptr::copy_nonoverlapping::<u8>(value.as_ptr(), self.data, layout.size());
        }
    }
}

impl ResourceCell {
    /// Return the underlying raw pointer and set resource to inactive.
    ///
    /// # Safety
    /// - If the data is `NonSend`, the function must be called on the correct thread.
    /// - Caller becomes responsible for later restoring or deallocating returned data.
    #[must_use]
    pub unsafe fn take_raw(&mut self) -> Option<(NonNull<u8>, Tick, Tick)> {
        let ptr = self.data;
        self.data = ptr::null_mut();
        NonNull::new(ptr).map(|ptr| (ptr, self.added, self.changed))
    }

    /// Reinsert raw pointer and change detection ticks, clear the old data.
    ///
    /// Unlike `insert_untyped`, this function is faster because it replace the pointer
    /// directly, no need to copy data. But in other words, the given pointer must be
    /// created from [`Self::take_raw`].
    ///
    /// # Safety
    /// - If the data is `NonSend`, the function must be called on the correct thread.
    /// - Caller must ensure that the type is correct and the pointer is valid.
    #[inline(never)]
    pub unsafe fn from_raw(&mut self, ptr: NonNull<u8>, added: Tick, changed: Tick) {
        if let Some(data) = NonNull::new(self.data) {
            ::core::hint::cold_path();
            self.data = ptr::null_mut(); // Avoid multiple drop after Panic
            let guard = AbortOnDropFail;
            unsafe {
                if let Some(dropper) = self.dropper {
                    dropper.call(OwningPtr::new(data));
                }
                if self.layout.size() != 0 {
                    malloc::dealloc(data.as_ptr(), self.layout);
                }
            }
            ::core::mem::forget(guard);
        }

        self.data = ptr.as_ptr();
        self.added = added;
        self.changed = changed;
    }
}

// -----------------------------------------------------------------------------
// Resources

/// The world's resource storage: a type-erased map from [`TypeId`] to a
/// single-resource slot.
///
/// # Structure
///
/// ```text
/// Resources {
///     storages: TypeMap<TypeId, &'static UnsafeCell<ResourceCell>>,
/// }
/// ```
///
/// Each [`ResourceCell`] slot is allocated **once per resource type** and
/// then lives for the entire program lifetime (see [`ResourceCell`] for the
/// rationale).  Inserting or removing a resource only swaps the data
/// pointer inside its slot; the slot itself is never deallocated.
///
/// # Lookup
///
/// Lookup is O(1) by [`TypeId`]: [`Resources::get`] / [`Resources::get_mut`]
/// return the slot for one type, [`Resources::entry`] additionally
/// allocates it on first use, and [`Resources::iter`] /
/// [`Resources::iter_mut`] visit every slot.
///
/// [`Resources::entry`] returns a `&'static` slot handle, which can be
/// cached (e.g. by `SystemParam`) to avoid repeated lookups; see
/// [`Resources::get_cell`] for the underlying raw handle.
///
/// # Access model
///
/// `Resources` itself is a plain container: data access is mediated by the
/// [`World`](crate::world::World) borrow rules.  Shared access goes through
/// `&self` methods (`get`, `get_cell`, `iter`), exclusive access through
/// `&mut self` methods (`get_mut`, `iter_mut`, `entry`).  The
/// `UnsafeCell` interior is only ever touched through these methods, so
/// the interior `unsafe` blocks are sound as long as that
/// shared/exclusive discipline is upheld.
///
/// # Safety
///
/// `Resources` is `Send + Sync` and implements [`UnwindSafe`] /
/// [`RefUnwindSafe`]: these are safe because the cell contents are only
/// accessed under the `&self`/`&mut self` discipline above, and slot
/// pointers are never shared across threads without proper synchronization.
pub struct Resources {
    /// Data storage, O(1) lookup by [`TypeId`].
    storages: TypeMap<&'static UnsafeCell<ResourceCell>>,
}

unsafe impl Sync for Resources {}
unsafe impl Send for Resources {}

impl UnwindSafe for Resources {}
impl RefUnwindSafe for Resources {}

impl Debug for Resources {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut debugger = f.debug_map();
        for cell in self.storages.values() {
            let cell = unsafe { &*cell.get() };
            debugger.entry(&cell.datebase.type_path, &cell.is_present());
        }
        debugger.finish()
    }
}

impl Resources {
    pub(crate) const fn new() -> Self {
        Self {
            storages: TypeMap::new(),
        }
    }

    pub(crate) fn insert(&mut self, ty: TypeId, cell: &'static UnsafeCell<ResourceCell>) -> bool {
        self.storages.try_insert(ty, || cell)
    }
}

impl Resources {
    /// Returns the slot for `ty` with shared access, or `None` if no slot
    /// for that type has been allocated yet.
    ///
    /// Use [`Resources::entry`] to allocate the slot on first access.
    #[inline]
    pub fn get(&self, ty: TypeId) -> Option<&ResourceCell> {
        let cell = self.storages.get(ty)?;
        unsafe { Some(&*cell.get()) }
    }

    /// Returns the slot for `ty` with exclusive access, or `None` if no
    /// slot for that type has been allocated yet.
    ///
    /// Use [`Resources::entry`] to allocate the slot on first access.
    #[inline]
    pub fn get_mut(&mut self, ty: TypeId) -> Option<&mut ResourceCell> {
        let cell = self.storages.get(ty)?;
        unsafe { Some(&mut *cell.get()) }
    }

    /// Returns the raw `'static` slot cell for `ty`, or `None` if no slot
    /// for that type has been allocated yet.
    ///
    /// The returned [`UnsafeCell`] can be accessed without going through
    /// `Resources` again (e.g. by `SystemParam` caching); the caller must
    /// uphold the same shared/exclusive access discipline as the
    /// `&self`/`&mut self` methods.
    #[inline]
    pub fn get_cell(&self, ty: TypeId) -> Option<&'static UnsafeCell<ResourceCell>> {
        Some(*self.storages.get(ty)?)
    }

    /// Iterates over every allocated slot with shared access.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &'_ ResourceCell> {
        self.storages.values().map(|c| unsafe { &*c.get() })
    }

    /// Iterates over every allocated slot with exclusive access.
    pub fn iter_mut(&mut self) -> impl ExactSizeIterator<Item = &'_ mut ResourceCell> {
        self.storages.values().map(|c| unsafe { &mut *c.get() })
    }

    /// Returns the slot for resource type `R`, allocating it on first use.
    ///
    /// The returned handle is `'static` and stays valid for the entire
    /// program lifetime, so it can be cached (e.g. by `SystemParam`) to
    /// avoid repeated [`TypeId`] lookups.
    pub fn entry<R: Resource>(&mut self) -> &'static UnsafeCell<ResourceCell> {
        use zlim_utils::ext::type_map::TypeMapEntry;
        match self.storages.entry(TypeId::of::<R>()) {
            TypeMapEntry::Occupied(entry) => entry.into_mut(),
            TypeMapEntry::Vacant(entry) => {
                ::core::hint::cold_path();
                let database = ResourceDB::of::<R>();
                let cell = ResourceCell::new(database);
                let ucell = UnsafeCell::new(cell);
                let reference = unsafe { Global::alloc_unchecked(ucell) };
                entry.insert(reference)
            }
        }
    }
}
