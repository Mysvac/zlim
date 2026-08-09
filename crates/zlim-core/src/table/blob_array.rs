use core::alloc::Layout;
use core::num::NonZeroUsize;
use core::ptr::{self, NonNull};
use std::alloc as malloc;

use zlim_ptr::{OwningPtr, Ptr, PtrMut};

use crate::utils::Dropper;

// -----------------------------------------------------------------------------
// DropGuard

/// A guard used to terminate a process
/// when `drop_fn` panicked.
struct AbortOnDropFail;

impl Drop for AbortOnDropFail {
    #[cold]
    #[inline(never)]
    fn drop(&mut self) {
        log::error!("Aborting due to drop component panicked.");
        ::std::process::abort();
    }
}

// -----------------------------------------------------------------------------
// BlobArray

/// A contiguous array storage for components of the same type.
///
/// This type handles raw byte-level memory management and supports both
/// types with and without drop logic.
///
/// The internal elements can be either dense or sparse.
#[derive(Debug)]
pub(super) struct BlobArray {
    item_layout: Layout,
    data: NonNull<u8>,
    dropper: Option<Dropper>,
}

impl BlobArray {
    /// Returns `true` if this array stores zero-sized types.
    #[inline(always)]
    const fn is_zst(&self) -> bool {
        self.item_layout.size() == 0
    }

    /// Returns the layout of individual items.
    #[inline(always)]
    pub const fn layout(&self) -> Layout {
        self.item_layout
    }

    /// Returns the drop function for items, if any.
    #[inline(always)]
    pub const fn dropper(&self) -> Option<Dropper> {
        self.dropper
    }

    /// Creates a new uninitialized `BlobArray`.
    ///
    /// # Safety
    /// - `item_layout` must correctly represent the type that will be stored
    /// - If provided, `drop_fn` must correctly drop an item of the stored type
    #[inline(always)]
    pub const unsafe fn new(item_layout: Layout, dropper: Option<Dropper>) -> Self {
        let align = unsafe { NonZeroUsize::new_unchecked(item_layout.align()) };

        Self {
            // Pad is required to support special layout types.
            item_layout: item_layout.pad_to_align(),
            dropper,
            data: NonNull::without_provenance(align),
        }
    }

    /// Allocates memory for the specified capacity.
    ///
    /// # Safety
    /// - The array must not be already allocated
    /// - The allocated memory is uninitialized
    pub unsafe fn alloc(&mut self, capacity: NonZeroUsize) {
        if !self.is_zst() {
            let new_layout = array_layout(self.item_layout, capacity.get());

            self.data = NonNull::new(unsafe { malloc::alloc(new_layout) })
                .unwrap_or_else(|| malloc::handle_alloc_error(new_layout));
        }
    }

    /// Reallocates memory from current capacity to new capacity.
    ///
    /// # Safety
    /// - The array must be already allocated with `current_capacity`
    /// - The contents are preserved up to `min(current_capacity, new_capacity)`
    /// - Any additional memory is uninitialized
    pub unsafe fn realloc(&mut self, current_capacity: NonZeroUsize, new_capacity: NonZeroUsize) {
        if !self.is_zst() {
            unsafe {
                let ptr: *mut u8 = self.data.as_ptr();
                let layout = array_layout_unchecked(self.item_layout, current_capacity.get());
                let new_layout = array_layout(self.item_layout, new_capacity.get());
                let new_size = new_layout.size();
                self.data = NonNull::new(malloc::realloc(ptr, layout, new_size))
                    .unwrap_or_else(|| malloc::handle_alloc_error(new_layout));
            }
        }
    }

    /// Deallocates memory, zero capacity is valid.
    ///
    /// Note that this function does **not** call `drop`.
    ///
    /// # Safety
    /// - `current_capacity` must be the current allocated capacity
    /// - All items in this array must be properly dropped
    pub unsafe fn dealloc(&mut self, current_capacity: usize) {
        if current_capacity != 0 && !self.is_zst() {
            unsafe {
                let layout = array_layout_unchecked(self.item_layout, current_capacity);
                malloc::dealloc(self.data.as_ptr(), layout);
            }
        }
    }

    /// Returns a shared pointer to the item at `index`.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The item at `index` must be properly initialized
    #[inline(always)]
    pub const unsafe fn get(&self, index: usize) -> Ptr<'_> {
        let size = self.item_layout.size();
        unsafe { Ptr::new(self.data.add(index * size)) }
    }

    /// Returns a mutable pointer to the item at `index`.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The item at `index` must be properly initialized
    #[inline(always)]
    pub const unsafe fn get_mut(&mut self, index: usize) -> PtrMut<'_> {
        let size = self.item_layout.size();
        unsafe { PtrMut::new(self.data.add(index * size)) }
    }

    /// Initializes an item at the specified index.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The slot at `index` must be uninitialized
    /// - `value` must be a valid instance of the stored type
    #[inline(always)]
    pub unsafe fn init_item(&mut self, index: usize, value: OwningPtr<'_>) {
        let size = self.item_layout.size();
        unsafe {
            let dst = self.data.as_ptr().byte_add(index * size);
            ptr::copy_nonoverlapping::<u8>(value.as_ptr(), dst, size);
        }
    }

    /// Replaces an existing item at the specified index.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The slot at `index` must be properly initialized
    /// - `value` must be a valid instance of the stored type
    #[inline]
    pub unsafe fn replace_item(&mut self, index: usize, value: OwningPtr<'_>) {
        let size = self.item_layout.size();
        unsafe {
            let dst = self.data.byte_add(index * size);
            if let Some(dropper) = self.dropper {
                let drop_guard = AbortOnDropFail;

                dropper.call(OwningPtr::new(dst));

                ::core::mem::forget(drop_guard);
            }
            ptr::copy_nonoverlapping::<u8>(value.as_ptr(), dst.as_ptr(), size);
        }
    }

    /// Returns a pointer that represents ownership of the target.
    ///
    /// The function itself does not change any data.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The item at `index` must be properly initialized
    /// - The caller assumes ownership of the returned pointer
    #[inline(always)]
    #[must_use = "The returned pointer should be used"]
    pub unsafe fn remove_item(&mut self, index: usize) -> OwningPtr<'_> {
        unsafe { self.get_mut(index).promote() }
    }

    /// Forgets specified items but no effect on the allocated capacity.
    ///
    /// Actually, do nothing
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The item at `index` must be uninitialized
    #[inline(always)]
    pub unsafe fn forget_item(&mut self, _index: usize) {
        // nothing
    }

    /// Drops specified items but no effect on the allocated capacity.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The item at `index` must be properly initialized
    #[inline]
    pub unsafe fn drop_item(&mut self, index: usize) {
        if let Some(dropper) = self.dropper {
            let drop_guard = AbortOnDropFail;

            unsafe { dropper.call(self.get_mut(index).promote()) }

            ::core::mem::forget(drop_guard);
        }
    }

    /// Drops all items but no effect on the allocated capacity.
    ///
    /// # Safety
    /// - `len` must be the number of initialized items (<= capacity)
    /// - All items from `0..len` must be properly initialized
    #[inline]
    pub unsafe fn drop_slice(&mut self, len: usize) {
        if let Some(dropper) = self.dropper {
            let drop_guard = AbortOnDropFail;
            (0..len).for_each(|index| unsafe { dropper.call(self.get_mut(index).promote()) });
            ::core::mem::forget(drop_guard);
        }
    }

    /// Swaps the item at `index` with the last item and returns the moved item.
    ///
    /// # Safety
    /// - `index` must be < `last_index`
    /// - Both `index` and `last_index` must be within bounds
    /// - Both items must be properly initialized
    #[inline]
    #[must_use = "The returned pointer should be used"]
    pub unsafe fn swap_remove_not_last(
        &mut self,
        index: usize,
        last_index: usize,
    ) -> OwningPtr<'_> {
        let size = self.item_layout.size();
        unsafe {
            let item = self.data.as_ptr().byte_add(size * index);
            let last = self.data.byte_add(size * last_index);
            ptr::swap_nonoverlapping::<u8>(item, last.as_ptr(), size);

            OwningPtr::new(last)
        }
    }

    /// Swaps the item at `index` with the last item and forget the moved item.
    ///
    /// # Safety
    /// - `index` must be < `last_index`
    /// - Both `index` and `last_index` must be within bounds
    /// - Both items must be properly initialized
    #[inline]
    pub unsafe fn swap_forget_not_last(&mut self, index: usize, last_index: usize) {
        let size = self.item_layout.size();
        unsafe {
            let item = self.data.as_ptr().byte_add(size * index);
            let last = self.data.as_ptr().byte_add(size * last_index);
            ptr::copy_nonoverlapping::<u8>(last, item, size);
        }
    }

    /// Swaps the item at `index` with the last item and drops the moved item.
    ///
    /// # Safety
    /// - `index` must be < `last_index`
    /// - Both `index` and `last_index` must be within bounds
    /// - Both items must be properly initialized
    #[inline]
    pub unsafe fn swap_drop_not_last(&mut self, index: usize, last_index: usize) {
        let dropper = self.dropper;

        unsafe {
            let value = self.swap_remove_not_last(index, last_index);
            if let Some(dropper) = dropper {
                dropper.call(value);
            }
        }
    }
}

// -----------------------------------------------------------------------------
// alloc helper

/// Creates a layout for an array with `n` elements, checking for overflow.
#[inline]
const fn array_layout(layout: Layout, n: usize) -> Layout {
    #[cold]
    #[inline(never)]
    const fn invalid_size() -> ! {
        panic!("invalid size in `Layout::from_size_align`");
    }

    let Some(alloc_size) = layout.size().checked_mul(n) else {
        invalid_size();
    };

    if alloc_size > isize::MAX as usize {
        invalid_size();
    }

    unsafe { Layout::from_size_align_unchecked(alloc_size, layout.align()) }
}

/// Creates a layout for an array with `n` elements without checking.
///
/// # Safety
/// - `layout.size() * n` must not overflow
/// - The resulting size must be <= `isize::MAX`
#[inline]
const unsafe fn array_layout_unchecked(layout: Layout, n: usize) -> Layout {
    unsafe { Layout::from_size_align_unchecked(layout.size() * n, layout.align()) }
}
