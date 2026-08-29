//! Column-level storage primitives.
//!
//! A [`Column`] stores raw component bytes plus parallel
//! added/changed tick arrays to support change detection.

use super::blob_array::BlobArray;
use super::tick_array::TickArray;

use core::alloc::Layout;
use core::num::NonZeroUsize;

use zlim_ptr::{OwningPtr, Ptr, PtrMut, ThinSlice};

use crate::borrow::{UntypedMut, UntypedRef};
use crate::borrow::{UntypedSliceMut, UntypedSliceRef};
use crate::tick::{Tick, TicksMut, TicksRef};
use crate::tick::{TicksSliceMut, TicksSliceRef};
use crate::utils::Dropper;

// -----------------------------------------------------------------------------
// Column

/// A column storing component data and their associated change detection ticks.
///
/// Each column contains three parallel arrays:
/// - `data`: The actual component values
/// - `added`: Tick when each component was added
/// - `changed`: Tick when each component was last modified
///
/// `Column` is the low-level untyped primitive that backs dense table
/// storage ([`Table`]).  Higher-level storage layers are responsible for
/// ensuring row validity and aliasing discipline before calling these
/// methods.
///
/// [`Table`]: crate::table::Table
#[derive(Debug)]
pub struct Column {
    data: BlobArray,
    added: TickArray,
    changed: TickArray,
}

// -----------------------------------------------------------------------------
// Basic methods

impl Column {
    /// Creates a new empty column.
    ///
    /// # Safety
    /// - `item_layout` must correctly represent the type that will be stored
    /// - If provided, `drop_fn` must correctly drop an item of the stored type
    #[inline(always)]
    pub const unsafe fn new(item_layout: Layout, dropper: Option<Dropper>) -> Self {
        Self {
            data: unsafe { BlobArray::new(item_layout, dropper) },
            added: TickArray::new(),
            changed: TickArray::new(),
        }
    }

    /// Returns the layout of individual items stored in this column.
    #[inline(always)]
    pub const fn item_layout(&self) -> Layout {
        self.data.layout()
    }

    /// Returns the drop function for items in this column, if any.
    #[inline(always)]
    pub const fn dropper(&self) -> Option<Dropper> {
        self.data.dropper()
    }

    /// Allocates memory for the specified capacity.
    ///
    /// # Safety
    /// - The column must not be already allocated
    /// - The allocated memory is uninitialized
    #[inline]
    pub unsafe fn alloc(&mut self, new_capacity: NonZeroUsize) {
        unsafe {
            self.data.alloc(new_capacity);
            self.added.alloc(new_capacity);
            self.changed.alloc(new_capacity);
        }
    }

    /// Reallocates memory from current capacity to new capacity.
    ///
    /// # Safety
    /// - The column must be already allocated with `current_capacity`
    /// - The contents are preserved up to `min(current_capacity, new_capacity)`
    /// - Any additional memory is uninitialized
    #[inline]
    pub unsafe fn realloc(&mut self, current_capacity: NonZeroUsize, new_capacity: NonZeroUsize) {
        unsafe {
            self.data.realloc(current_capacity, new_capacity);
            self.added.realloc(current_capacity, new_capacity);
            self.changed.realloc(current_capacity, new_capacity);
        }
    }

    /// Deallocates memory.
    ///
    /// Note that this function does **not** call `drop`.
    ///
    /// # Safety
    /// - `current_capacity` must be the current allocated capacity
    /// - All items in this array must be properly dropped
    #[inline]
    pub unsafe fn dealloc(&mut self, current_capacity: usize) {
        unsafe {
            self.data.dealloc(current_capacity);
            self.added.dealloc(current_capacity);
            self.changed.dealloc(current_capacity);
        }
    }

    /// Returns a shared untyped pointer to component data at `index`.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The item at `index` must be properly initialized
    #[inline(always)]
    pub unsafe fn get_data(&self, index: usize) -> Ptr<'_> {
        unsafe { self.data.get(index) }
    }

    /// Returns a mutable untyped pointer to component data at `index`.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The item at `index` must be properly initialized
    #[inline(always)]
    pub unsafe fn get_data_mut(&mut self, index: usize) -> PtrMut<'_> {
        unsafe { self.data.get_mut(index) }
    }

    /// Returns the `added` tick at `index`.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The item at `index` must be properly initialized
    #[inline(always)]
    pub unsafe fn get_added(&self, index: usize) -> Tick {
        unsafe { self.added.get(index) }
    }

    /// Returns the `changed` tick at `index`.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The item at `index` must be properly initialized
    #[inline(always)]
    pub unsafe fn get_changed(&self, index: usize) -> Tick {
        unsafe { self.changed.get(index) }
    }

    /// Returns a mutable reference to the `added` tick at `index`.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The item at `index` must be properly initialized
    #[inline(always)]
    pub unsafe fn get_added_mut(&mut self, index: usize) -> &mut Tick {
        unsafe { self.added.get_mut(index) }
    }

    /// Returns a mutable reference to the `changed` tick at `index`.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The item at `index` must be properly initialized
    #[inline(always)]
    pub unsafe fn get_changed_mut(&mut self, index: usize) -> &mut Tick {
        unsafe { self.changed.get_mut(index) }
    }

    /// Returns a thin slice of all added ticks.
    ///
    /// # Safety
    /// The caller must ensure proper bounds when accessing the slice.
    #[inline(always)]
    pub unsafe fn get_added_slice(&self) -> ThinSlice<'_, Tick> {
        unsafe { self.added.get_slice() }
    }

    /// Returns a thin slice of all changed ticks.
    ///
    /// # Safety
    /// The caller must ensure proper bounds when accessing the slice.
    #[inline(always)]
    pub unsafe fn get_changed_slice(&self) -> ThinSlice<'_, Tick> {
        unsafe { self.changed.get_slice() }
    }

    /// Initializes an item at the specified index with data and ticks.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The slot at `index` must be uninitialized
    /// - `data` must be a valid instance of the stored type
    #[inline]
    pub unsafe fn init_item(&mut self, index: usize, data: OwningPtr<'_>, tick: Tick) {
        unsafe {
            self.data.init_item(index, data);
            self.added.set(index, tick);
            self.changed.set(index, tick);
        }
    }

    /// Replaces an existing item at the specified index.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The slot at `index` must be properly initialized
    /// - `data` must be a valid instance of the stored type
    #[inline]
    pub unsafe fn replace_item(&mut self, index: usize, data: OwningPtr<'_>, tick: Tick) {
        unsafe {
            self.data.replace_item(index, data);
            self.changed.set(index, tick);
        }
    }

    /// Returns a pointer that conceptually represents ownership of the data.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The item must be properly initialized
    #[inline]
    #[must_use = "The returned pointer should be used."]
    pub unsafe fn remove_item(&mut self, index: usize) -> OwningPtr<'_> {
        unsafe { self.data.remove_item(index) }
    }

    /// Marks the item at `index` as forgotten.
    ///
    /// This is a no-op: the item's bytes are left in place and the
    /// allocation is unchanged.  The caller guarantees that the item does
    /// not need to be dropped here (e.g. it was already moved out or is a
    /// zero-sized type).
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The item must not require dropping at this site
    #[inline]
    pub unsafe fn forget_item(&mut self, index: usize) {
        unsafe {
            self.data.forget_item(index);
        }
    }

    /// Drops the item in place.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The item must be properly initialized
    #[inline]
    pub unsafe fn drop_item(&mut self, index: usize) {
        unsafe {
            self.data.drop_item(index);
        }
    }

    /// Drops all items but no effect on the allocated capacity.
    ///
    /// # Safety
    /// - `len` must be the number of initialized items (<= capacity)
    /// - All items from `0..len` must be properly initialized
    #[inline]
    pub unsafe fn drop_slice(&mut self, len: usize) {
        unsafe {
            self.data.drop_slice(len);
        }
    }

    /// Swaps the item at `index` with the last item and forget the moved item.
    ///
    /// Actually, this function does not swap data.
    ///
    /// # Safety
    /// - `index` must be < `last_index`
    /// - Both `index` and `last_index` must be within bounds
    /// - Both items must be properly initialized
    #[inline]
    pub unsafe fn swap_forget_not_last(&mut self, index: usize, last_index: usize) {
        unsafe {
            self.added.move_last_to(last_index, index);
            self.changed.move_last_to(last_index, index);
            self.data.swap_forget_not_last(index, last_index);
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
        unsafe {
            self.added.move_last_to(last_index, index);
            self.changed.move_last_to(last_index, index);
            self.data.swap_drop_not_last(index, last_index);
        }
    }

    /// Move a element from `self[src]` to `other[dst]`.
    ///
    /// This function does not drop the elements in `other[dst]`.
    ///
    /// # Safety
    /// - `self != other`
    /// - `src` must be within bounds (0..self.capacity)
    /// - `dst` must be within bounds (0..other.capacity)
    /// - The item at `src` must be properly initialized
    /// - The item at `dst` must be uninitialized
    #[inline]
    pub unsafe fn move_item_to(&mut self, other: &mut Self, src: usize, dst: usize) {
        unsafe {
            other.data.init_item(dst, self.data.remove_item(src));
            other.added.set(dst, self.added.get(src));
            other.changed.set(dst, self.changed.get(src));
        }
    }

    /// Check the ticks of all components and ensure they are valid.
    ///
    /// # Safety
    /// - `len` must be the number of initialized items
    #[inline]
    pub unsafe fn clamp_ticks(&mut self, len: usize, now: Tick) {
        use crate::utils::clamp_tick_slice;
        unsafe {
            clamp_tick_slice(self.added.get_slice_mut().as_mut(len), now);
            clamp_tick_slice(self.changed.get_slice_mut().as_mut(len), now);
        }
    }

    /// Returns a typed shared reference to the component at `index`.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The item at `index` must be properly initialized
    #[inline]
    pub unsafe fn get_ref(&self, index: usize, last_run: Tick, this_run: Tick) -> UntypedRef<'_> {
        unsafe {
            UntypedRef {
                value: self.data.get(index),
                ticks: TicksRef {
                    added: self.added.get_ref(index),
                    changed: self.changed.get_ref(index),
                    last_run,
                    this_run,
                },
            }
        }
    }

    /// Returns a typed mutable reference to the component at `index`.
    ///
    /// # Safety
    /// - `index` must be within bounds (0..capacity)
    /// - The item at `index` must be properly initialized
    #[inline]
    pub unsafe fn get_mut(
        &mut self,
        index: usize,
        last_run: Tick,
        this_run: Tick,
    ) -> UntypedMut<'_> {
        unsafe {
            UntypedMut {
                value: self.data.get_mut(index),
                ticks: TicksMut {
                    added: self.added.get_mut(index),
                    changed: self.changed.get_mut(index),
                    last_run,
                    this_run,
                },
            }
        }
    }

    /// Returns a shared reference to a slice of components.
    ///
    /// # Safety
    /// - `len` must be <= capacity
    /// - All items in `0..len` must be properly initialized
    #[inline]
    pub unsafe fn get_slice_ref(
        &self,
        len: usize,
        last_run: Tick,
        this_run: Tick,
    ) -> UntypedSliceRef<'_> {
        unsafe {
            UntypedSliceRef {
                value: self.data.get(0),
                ticks: TicksSliceRef {
                    length: len,
                    added: self.added.get_slice(),
                    changed: self.changed.get_slice(),
                    last_run,
                    this_run,
                },
            }
        }
    }

    /// Returns a mutable reference to a slice of components.
    ///
    /// # Safety
    /// - `len` must be <= capacity
    /// - All items in `0..len` must be properly initialized
    #[inline]
    pub unsafe fn get_slice_mut(
        &mut self,
        len: usize,
        last_run: Tick,
        this_run: Tick,
    ) -> UntypedSliceMut<'_> {
        unsafe {
            UntypedSliceMut {
                value: self.data.get_mut(0),
                ticks: TicksSliceMut {
                    length: len,
                    added: self.added.get_slice_mut(),
                    changed: self.changed.get_slice_mut(),
                    last_run,
                    this_run,
                },
            }
        }
    }

    /// Shrink capacity to given value.
    ///
    /// # Safety
    /// - `new` must be <= capacity
    /// - `current_capacity` must be the current allocated capacity
    /// - The objects of the removed slots must be dropped.
    pub(super) unsafe fn shrink_to(&mut self, current: NonZeroUsize, new: usize) {
        match NonZeroUsize::new(new) {
            Some(new) => unsafe {
                self.data.realloc(current, new);
                self.added.realloc(current, new);
                self.changed.realloc(current, new);
            },
            None => unsafe {
                ::core::hint::cold_path();
                self.data.dealloc(current.get());
                self.added.dealloc(current.get());
                self.changed.dealloc(current.get());
            },
        }
    }
}
