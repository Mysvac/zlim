use core::cell::UnsafeCell;
use core::fmt::Debug;
use core::marker::PhantomData;
use core::ptr::{self, NonNull};
use core::slice;

// -----------------------------------------------------------------------------
// ThinSlice and ThinSliceMut

/// A thin reference to a slice that stores only the pointer (no length).
///
/// This type is useful when the slice length is known from context and storing
/// it separately would waste memory. It provides shared access to the elements.
///
/// # Examples
///
/// ```
/// # use zlim_ptr::ThinSlice;
/// let data = [1, 2, 3, 4, 5];
/// let thin = ThinSlice::from_ref(&data);
///
/// // The length must be provided when accessing
/// unsafe {
///     assert_eq!(thin.deref(5), &[1, 2, 3, 4, 5]);
///     assert_eq!(thin.get(2), &3);
/// }
/// ```
#[repr(transparent)]
pub struct ThinSlice<'a, T> {
    ptr: NonNull<T>,
    _marker: PhantomData<&'a [T]>,
}

/// A thin mutable reference to a slice that stores only the pointer (no length).
///
/// This type is useful when the slice length is known from context and storing
/// it separately would waste memory. It provides exclusive access to the elements.
///
/// # Examples
///
/// ```
/// # use zlim_ptr::ThinSliceMut;
/// let mut data = [1, 2, 3, 4, 5];
/// let thin = ThinSliceMut::from_mut(&mut data);
///
/// unsafe {
///     // Read and write elements
///     assert_eq!(thin.read(0), 1);
///     thin.write(0, 10);
///     assert_eq!(thin.get(0), &10);
///     
///     // Get as a slice
///     assert_eq!(thin.deref(5), &[10, 2, 3, 4, 5]);
/// }
/// ```
#[repr(transparent)]
pub struct ThinSliceMut<'a, T> {
    ptr: NonNull<T>,
    _marker: PhantomData<&'a mut [T]>,
}

// -----------------------------------------------------------------------------
// Basic

impl<T> Copy for ThinSlice<'_, T> {}

impl<T> Clone for ThinSlice<'_, T> {
    #[inline(always)]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Debug for ThinSlice<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Slice").field(&self.ptr).finish()
    }
}

impl<T> Debug for ThinSliceMut<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("ThinSliceMut").field(&self.ptr).finish()
    }
}

// -----------------------------------------------------------------------------
// From

impl<'a, T> From<&'a [T]> for ThinSlice<'a, T> {
    #[inline]
    fn from(slice: &'a [T]) -> Self {
        Self::from_ref(slice)
    }
}

impl<'a, T> From<&'a mut [T]> for ThinSlice<'a, T> {
    #[inline]
    fn from(slice: &'a mut [T]) -> Self {
        Self::from_mut(slice)
    }
}

impl<'a, T> From<&'a mut [T]> for ThinSliceMut<'a, T> {
    #[inline]
    fn from(slice: &'a mut [T]) -> Self {
        Self::from_mut(slice)
    }
}

impl<'a, T> From<&'a [UnsafeCell<T>]> for ThinSliceMut<'a, T> {
    #[inline]
    fn from(slice: &'a [UnsafeCell<T>]) -> Self {
        unsafe { Self::from_raw(NonNull::new_unchecked(slice.as_ptr() as *mut T)) }
    }
}

impl<'a, T> From<&'a UnsafeCell<[T]>> for ThinSliceMut<'a, T> {
    #[inline]
    fn from(slice: &'a UnsafeCell<[T]>) -> Self {
        unsafe { Self::from_raw(NonNull::new_unchecked(slice.get() as *mut T)) }
    }
}

impl<'a, T> From<ThinSliceMut<'a, T>> for ThinSlice<'a, T> {
    #[inline(always)]
    fn from(value: ThinSliceMut<'a, T>) -> Self {
        Self {
            ptr: value.ptr,
            _marker: PhantomData,
        }
    }
}

impl<'a, T> From<ThinSlice<'a, UnsafeCell<T>>> for ThinSliceMut<'a, T> {
    #[inline(always)]
    fn from(value: ThinSlice<'a, UnsafeCell<T>>) -> Self {
        Self {
            ptr: value.ptr.cast(),
            _marker: PhantomData,
        }
    }
}

// -----------------------------------------------------------------------------
// Methods

impl<'a, T> ThinSlice<'a, T> {
    /// Creates a `Slice` from a raw pointer.
    ///
    /// # Safety
    /// - The pointer must be valid for reads for the lifetime `'a`
    /// - The caller must ensure proper bounds when accessing elements
    #[inline(always)]
    pub const unsafe fn from_raw(ptr: NonNull<T>) -> Self {
        Self {
            _marker: PhantomData,
            ptr,
        }
    }

    /// Returns the underlying pointer.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::ThinSlice;
    /// let data = [1, 2, 3];
    /// let thin = ThinSlice::from_ref(&data);
    /// let ptr = thin.into_inner();
    /// unsafe {
    ///     assert_eq!(*ptr.as_ref(), 1);
    /// }
    /// ```
    #[inline(always)]
    pub const fn into_inner(self) -> NonNull<T> {
        self.ptr
    }

    /// Creates a `Slice` from a shared slice reference.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::ThinSlice;
    /// let data = [1, 2, 3];
    /// let thin = ThinSlice::from_ref(&data);
    /// unsafe {
    ///     assert_eq!(thin.deref(3), &[1, 2, 3]);
    /// }
    /// ```
    #[inline(always)]
    pub const fn from_ref(r: &'a [T]) -> Self {
        Self {
            _marker: PhantomData,
            ptr: NonNull::from_ref(r).cast(),
        }
    }

    /// Creates a `Slice` from a mutable slice reference.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::ThinSlice;
    /// let mut data = [1, 2, 3];
    /// let thin = ThinSlice::from_mut(&mut data);
    /// unsafe {
    ///     assert_eq!(thin.deref(3), &[1, 2, 3]);
    /// }
    /// ```
    #[inline(always)]
    pub const fn from_mut(r: &'a mut [T]) -> Self {
        Self {
            _marker: PhantomData,
            ptr: NonNull::from_ref(r).cast(),
        }
    }

    /// Returns a shared reference to the element at `index`.
    ///
    /// # Safety
    /// - `index` must be within bounds
    /// - The element must be properly initialized
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::ThinSlice;
    /// let data = [100, 200, 300];
    /// let thin = ThinSlice::from_ref(&data);
    /// unsafe {
    ///     assert_eq!(thin.get(0), &100);
    ///     assert_eq!(thin.get(2), &300);
    /// }
    /// ```
    #[inline(always)]
    pub const unsafe fn get(self, index: usize) -> &'a T {
        unsafe { &*self.ptr.as_ptr().add(index) }
    }

    /// Consumes itself and returns a slice with the same lifetime.
    ///
    /// # Safety
    /// - All elements in `0..len` must be properly initialized
    /// - `len` must not exceed the actual allocation size
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::ThinSlice;
    /// let data = [42, 43, 44];
    /// let thin = ThinSlice::from_ref(&data);
    /// let slice = unsafe { thin.deref(3) };
    /// assert_eq!(slice, &[42, 43, 44]);
    /// ```
    #[inline(always)]
    pub const unsafe fn deref(self, len: usize) -> &'a [T] {
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), len) }
    }

    /// Reads a copy of the element at `index`.
    ///
    /// # Safety
    /// - `index` must be within bounds
    /// - The element must be properly initialized
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::ThinSlice;
    /// let data = [5, 6, 7];
    /// let thin = ThinSlice::from_ref(&data);
    /// unsafe {
    ///     assert_eq!(thin.read(1), 6);
    ///     assert_eq!(thin.read(2), 7);
    /// }
    /// ```
    #[inline(always)]
    pub const unsafe fn read(self, index: usize) -> T
    where
        T: Copy,
    {
        unsafe { ptr::read(self.ptr.as_ptr().add(index)) }
    }
}

impl<'a, T> ThinSliceMut<'a, T> {
    /// Creates a `ThinSliceMut` from a raw pointer.
    ///
    /// # Safety
    /// - The pointer must be valid for reads and writes for the lifetime `'a`
    /// - No other references to the same memory must exist
    /// - The caller must ensure proper bounds when accessing elements
    #[inline(always)]
    pub const unsafe fn from_raw(ptr: NonNull<T>) -> Self {
        Self {
            _marker: PhantomData,
            ptr,
        }
    }

    /// Returns the underlying pointer.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::ThinSliceMut;
    /// let mut data = [1, 2];
    /// let thin = ThinSliceMut::from_mut(&mut data);
    /// let ptr = thin.into_inner();
    /// unsafe {
    ///     assert_eq!(*ptr.as_ref(), 1);
    /// }
    /// ```
    #[inline(always)]
    pub const fn into_inner(self) -> NonNull<T> {
        self.ptr
    }

    /// Creates a `ThinSliceMut` from a mutable slice reference.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::ThinSliceMut;
    /// let mut data = [1, 2, 3];
    /// let thin = ThinSliceMut::from_mut(&mut data);
    /// unsafe {
    ///     assert_eq!(thin.deref(3), &[1, 2, 3]);
    /// }
    /// ```
    #[inline(always)]
    pub const fn from_mut(r: &'a mut [T]) -> Self {
        Self {
            _marker: PhantomData,
            ptr: NonNull::from_ref(r).cast(),
        }
    }

    /// Borrow this pointer with a shorter lifetime.
    ///
    /// This is useful when a helper function needs temporary
    /// immutable access without consuming the original `Slice`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::{ThinSliceMut, ThinSlice};
    /// fn foo(ptr: ThinSlice<'_, i32>) { /* ... */ }
    ///
    /// let mut data = [10, 20, 30];
    /// let thin = ThinSliceMut::from_mut(&mut data);
    /// foo(thin.borrow());
    ///
    /// // `thin` is still usable here
    /// unsafe {
    ///     thin.write(0, 99);
    /// }
    /// ```
    #[inline(always)]
    pub const fn borrow(&self) -> ThinSlice<'_, T> {
        ThinSlice {
            _marker: PhantomData,
            ptr: self.ptr,
        }
    }

    /// Reborrow this pointer with a shorter lifetime.
    ///
    /// This is useful when a helper function needs temporary
    /// mutable access without consuming the original `ThinSliceMut`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::ThinSliceMut;
    /// fn foo(ptr: ThinSliceMut<'_, i32>) { /* ... */ }
    ///
    /// let mut data = [10, 20, 30];
    /// let mut thin = ThinSliceMut::from_mut(&mut data);
    /// foo(thin.reborrow());
    ///
    /// // `thin` is still usable here
    /// unsafe {
    ///     thin.write(0, 99);
    /// }
    /// ```
    #[inline(always)]
    pub const fn reborrow(&mut self) -> ThinSliceMut<'_, T> {
        ThinSliceMut {
            _marker: PhantomData,
            ptr: self.ptr,
        }
    }

    /// Returns a shared reference to the element at `index`.
    ///
    /// # Safety
    /// - `index` must be within bounds
    /// - The element must be properly initialized
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::ThinSliceMut;
    /// let mut data = [100, 200];
    /// let thin = ThinSliceMut::from_mut(&mut data);
    /// unsafe {
    ///     assert_eq!(thin.get(0), &100);
    ///     assert_eq!(thin.get(1), &200);
    /// }
    /// ```
    #[inline(always)]
    pub const unsafe fn get(&self, index: usize) -> &'_ T {
        unsafe { &*self.ptr.as_ptr().add(index) }
    }

    /// Returns a mutable reference to the element at `index`.
    ///
    /// # Safety
    /// - `index` must be within bounds
    /// - The element must be properly initialized
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::ThinSliceMut;
    /// let mut data = [10, 20, 30];
    /// let mut thin = ThinSliceMut::from_mut(&mut data);
    /// unsafe {
    ///     let elem = thin.get_mut(1);
    ///     *elem = 99;
    ///     assert_eq!(*elem, 99);
    ///     assert_eq!(thin.read(1), 99);
    /// }
    /// ```
    #[inline(always)]
    pub const unsafe fn get_mut(&mut self, index: usize) -> &'_ mut T {
        unsafe { &mut *self.ptr.as_ptr().add(index) }
    }

    /// Returns a shared slice with the given length.
    ///
    /// # Safety
    /// - All elements in `0..len` must be properly initialized
    /// - `len` must not exceed the actual allocation size
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::ThinSliceMut;
    /// let mut data = [1, 2, 3, 4];
    /// let thin = ThinSliceMut::from_mut(&mut data);
    /// unsafe {
    ///     let slice = thin.as_ref(3);
    ///     assert_eq!(slice, &[1, 2, 3]);
    /// }
    /// ```
    #[inline(always)]
    pub const unsafe fn as_ref(&self, len: usize) -> &'_ [T] {
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), len) }
    }

    /// Returns a mutable slice with the given length.
    ///
    /// # Safety
    /// - All elements in `0..len` must be properly initialized
    /// - `len` must not exceed the actual allocation size
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::ThinSliceMut;
    /// let mut data = [1, 2, 3, 4];
    /// let mut thin = ThinSliceMut::from_mut(&mut data);
    /// unsafe {
    ///     let slice = thin.as_mut(2);
    ///     slice[0] = 99;
    ///     slice[1] = 88;
    ///     assert_eq!(thin.deref(4), &[99, 88, 3, 4]);
    /// }
    /// ```
    #[inline(always)]
    pub const unsafe fn as_mut(&mut self, len: usize) -> &'_ mut [T] {
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), len) }
    }

    /// Consumes itself and returns a slice with the same lifetime.
    ///
    /// # Safety
    /// - All elements in `0..len` must be properly initialized
    /// - `len` must not exceed the actual allocation size
    #[inline(always)]
    pub const unsafe fn deref(self, len: usize) -> &'a mut [T] {
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), len) }
    }

    /// Reads a copy of the element at `index`.
    ///
    /// # Safety
    /// - `index` must be within bounds
    /// - The element must be properly initialized
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::ThinSliceMut;
    /// let mut data = [42, 43, 44];
    /// let thin = ThinSliceMut::from_mut(&mut data);
    /// unsafe {
    ///     assert_eq!(thin.read(0), 42);
    ///     assert_eq!(thin.read(2), 44);
    /// }
    /// ```
    #[inline(always)]
    pub const unsafe fn read(&self, index: usize) -> T
    where
        T: Copy,
    {
        unsafe { ptr::read(self.ptr.as_ptr().add(index)) }
    }

    /// Writes a copy of the value to the element at `index`.
    ///
    /// # Safety
    /// - `index` must be within bounds
    /// - The input value must be properly initialized.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::ThinSliceMut;
    /// let mut data = [1, 2, 3];
    /// let thin = ThinSliceMut::from_mut(&mut data);
    /// unsafe {
    ///     thin.write(1, 99);
    ///     assert_eq!(thin.read(1), 99);
    /// }
    /// ```
    #[inline(always)]
    pub const unsafe fn write(&self, index: usize, value: T)
    where
        T: Copy,
    {
        unsafe { ptr::write(self.ptr.as_ptr().add(index), value) }
    }
}
