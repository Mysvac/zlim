//! Contiguous change-detection tick storage.

use core::alloc::Layout;
use core::fmt::{Debug, Formatter};
use core::num::NonZeroUsize;
use core::ptr::{self, NonNull};
use std::alloc as malloc;

use zlim_ptr::{Slice, SliceMut};

use crate::tick::Tick;

// -----------------------------------------------------------------------------
// TickArray

/// A contiguous array storage for tick values.
///
/// This type provides efficient storage and manipulation of [`Tick`] values,
/// optimized for the specific requirements of ECS change detection.
#[repr(transparent)]
pub(super) struct TickArray {
    data: NonNull<Tick>,
}

impl Debug for TickArray {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&self.data.cast::<u8>(), f)
    }
}

impl TickArray {
    /// Creates a new empty `TickArray`.
    ///
    /// The array is initially unallocated and must be allocated before use.
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            data: NonNull::dangling(),
        }
    }

    /// Allocates memory for the specified capacity.
    ///
    /// # Safety
    /// - The array must not be already allocated
    /// - The allocated memory is uninitialized
    pub unsafe fn alloc(&mut self, capacity: NonZeroUsize) {
        let new_layout = Layout::array::<Tick>(capacity.get()).unwrap();

        self.data = NonNull::new(unsafe { malloc::alloc(new_layout) })
            .unwrap_or_else(|| malloc::handle_alloc_error(new_layout))
            .cast();
    }

    /// Reallocates memory from current capacity to new capacity.
    ///
    /// # Safety
    /// - The array must be already allocated with `current_capacity`
    /// - The contents are preserved up to `min(current_capacity, new_capacity)`
    /// - Any additional memory is uninitialized
    pub unsafe fn realloc(&mut self, current_capacity: NonZeroUsize, new_capacity: NonZeroUsize) {
        unsafe {
            let ptr = self.data.as_ptr() as *mut u8;
            let layout = Layout::array::<Tick>(current_capacity.get()).unwrap_unchecked();
            let new_layout = Layout::array::<Tick>(new_capacity.get()).unwrap();
            let new_size = new_layout.size();
            self.data = NonNull::new(malloc::realloc(ptr, layout, new_size))
                .unwrap_or_else(|| malloc::handle_alloc_error(new_layout))
                .cast();
        }
    }

    /// Deallocates the memory.
    ///
    /// # Safety
    /// - `current_capacity` must be the current allocated capacity
    pub unsafe fn dealloc(&mut self, current_capacity: usize) {
        if current_capacity != 0 {
            unsafe {
                let layout = Layout::array::<Tick>(current_capacity).unwrap_unchecked();
                malloc::dealloc(self.data.as_ptr().cast(), layout);
            }
        }
    }

    /// Initializes a tick at the specified index.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    #[inline(always)]
    pub unsafe fn set(&mut self, index: usize, value: Tick) {
        unsafe {
            ptr::write(self.data.as_ptr().add(index), value);
        }
    }

    /// Returns a copy of the tick at the specified index.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    #[inline(always)]
    pub unsafe fn get(&self, index: usize) -> Tick {
        unsafe { *self.data.as_ptr().add(index) }
    }

    /// Returns a shared reference to the tick at the specified index.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    #[inline(always)]
    pub unsafe fn get_ref(&self, index: usize) -> &Tick {
        unsafe { &*self.data.as_ptr().add(index) }
    }

    /// Returns a mutable reference to the tick at the specified index.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    #[inline(always)]
    pub unsafe fn get_mut(&mut self, index: usize) -> &mut Tick {
        unsafe { &mut *self.data.as_ptr().add(index) }
    }

    /// Returns a shared slice of ticks.
    ///
    /// # Safety
    /// The returned [`Slice`] is only valid until the array is reallocated
    /// or deallocated; the caller must enforce that.
    #[inline(always)]
    pub unsafe fn get_slice(&self) -> Slice<'_, Tick> {
        unsafe { Slice::from_raw(self.data) }
    }

    /// Returns a mutable slice of ticks.
    ///
    /// # Safety
    /// The returned [`SliceMut`] is only valid until the array is
    /// reallocated or deallocated, and must not alias any other reference
    /// into the array; the caller must enforce that.
    #[inline(always)]
    pub unsafe fn get_slice_mut(&mut self) -> SliceMut<'_, Tick> {
        unsafe { SliceMut::from_raw(self.data) }
    }

    /// Copies the tick at `last` to `to`, without returning the moved value.
    ///
    /// This is the tick-array counterpart of a swap-remove: it leaves a
    /// duplicate at `last`.  The caller is responsible for updating the
    /// logical length afterwards.
    ///
    /// # Safety
    /// - `to` must be < `last` (nonoverlapping)
    /// - Both `to` and `last` must be within bounds
    #[inline(always)]
    pub unsafe fn move_last_to(&mut self, last: usize, to: usize) {
        let base_ptr = self.data.as_ptr();

        unsafe {
            let src = base_ptr.add(last);
            let dst = base_ptr.add(to);

            ptr::copy_nonoverlapping::<Tick>(src, dst, 1);
        }
    }
}
