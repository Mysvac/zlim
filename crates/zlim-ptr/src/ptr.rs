use core::fmt;
use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::ptr::{self, NonNull};

// -----------------------------------------------------------------------------
// Common methods

macro_rules! impl_ptr {
    ($ptr:ident) => {
        impl From<$ptr<'_>> for NonNull<u8> {
            #[inline(always)]
            fn from(ptr: $ptr<'_>) -> Self {
                ptr.0
            }
        }

        impl $ptr<'_> {
            /// Gets the underlying pointer, erasing the associated lifetime.
            #[inline(always)]
            pub const fn into_inner(self) -> NonNull<u8> {
                self.0
            }

            /// Check if the pointer is aligned to type `T`.
            ///
            /// This function is not const because the address of pointer
            /// cannot be read in compile-time.
            #[inline]
            pub fn is_aligned<T>(&self) -> bool {
                // We do not use `ptr::is_align` because `align_of` ensure that
                // result is power of two, we can reduce one judgement.
                let align: usize = ::core::mem::align_of::<T>();
                self.0.as_ptr().addr() & (align - 1) == 0
            }

            /// A function that only checks alignment in debug mode.
            ///
            /// Ensure zero overhead in release mode.
            #[cfg_attr(debug_assertions, track_caller)]
            #[cfg_attr(not(debug_assertions), inline(always))]
            pub fn debug_assert_aligned<T>(&self) {
                #[cfg(debug_assertions)]
                assert!(
                    self.is_aligned::<T>(),
                    "pointer is not aligned. Address {:p} does not have alignment {} for type {}",
                    self.0,
                    ::core::mem::align_of::<T>(),
                    ::core::any::type_name::<T>(),
                );
            }

            /// Calculates the offset from a pointer.
            ///
            /// As the pointer is type-erased, `count` parameter is in raw bytes.
            ///
            /// # Safety
            /// - The offset cannot make the existing ptr null or invalid target.
            /// - The resulting pointer must outlive the lifetime of this pointer.
            #[inline]
            pub const unsafe fn byte_offset(self, count: isize) -> Self {
                Self(
                    // SAFETY: The caller upholds safety for `offset`
                    // and ensures the result is not null.
                    unsafe { self.0.offset(count) },
                    PhantomData,
                )
            }

            /// Calculates the offset from a pointer.
            ///
            /// As the pointer is type-erased, `count` parameter is in raw bytes.
            ///
            /// # Safety
            /// - The offset cannot make the existing ptr null or invalid target.
            /// - The resulting pointer must outlive the lifetime of this pointer.
            #[inline]
            pub const unsafe fn byte_add(self, count: usize) -> Self {
                Self(
                    // SAFETY: The caller upholds safety for `add`
                    // and ensures the result is not null.
                    unsafe { self.0.add(count) },
                    PhantomData,
                )
            }
        }

        impl fmt::Pointer for $ptr<'_> {
            #[inline]
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Pointer::fmt(&self.0, f)
            }
        }

        impl fmt::Debug for $ptr<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({:?})", stringify!($ptr), self.0)
            }
        }
    };
}

// -----------------------------------------------------------------------------
// Ptr

/// A fully type-erased pointer, similar to `&'a dyn Any`.
///
/// # type-erased
///
/// Due to type-erased, we cannot confirm whether it meets the alignment requirements.
/// But when you use this to access targets, you should ensure it is aligned.
///
/// # borrow-like
///
/// - It must always point to a valid value of whatever the pointee type is.
/// - The lifetime `'a` accurately represents how long the pointer is valid for.
///
/// # immutable
///
/// Its target must not be changed while this pointer is alive.
///
/// Usually, Rust's borrow checker can ensure this through their lifetime.
///
/// # Examples
///
/// ```
/// # use zlim_ptr::Ptr;
/// let x = 8i32;
/// let ptr = Ptr::from_ref(&x);
///
/// let rx: &i32 = unsafe { ptr.deref() };
/// assert_eq!(*rx, 8);
/// ```
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Ptr<'a>(NonNull<u8>, PhantomData<&'a u8>);

impl_ptr!(Ptr);

impl<'a> Ptr<'a> {
    /// Create a `Ptr` from a raw `NonNull<u8>` pointer.
    ///
    /// # Safety
    ///
    /// - The provided lifetime `'a` must be valid for the pointee.
    /// - `ptr` must point to a valid object of the intended pointee type.
    #[inline(always)]
    pub const unsafe fn new(ptr: NonNull<u8>) -> Ptr<'a> {
        Ptr(ptr, PhantomData)
    }

    /// Creates a `Ptr` from a reference with same lifetime.
    ///
    /// This is safe because the lifetime provided by the reference
    /// must be correct.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::Ptr;
    /// let x = 8i32;
    /// let ptr = Ptr::from_ref(&x);
    /// ```
    #[inline(always)]
    pub const fn from_ref<T: ?Sized>(val: &'a T) -> Ptr<'a> {
        Ptr(NonNull::from_ref(val).cast(), PhantomData)
    }

    /// Creates a `Ptr` from a mutable reference with same lifetime.
    ///
    /// This is safe because the lifetime provided by the reference
    /// must be correct.
    ///
    /// The Rust's borrow checker ensures that mutable references
    /// cannot be used when `Ptr` is active.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::Ptr;
    /// let mut x = 8i32;
    /// let ptr = Ptr::from_mut(&mut x);
    /// ```
    #[inline(always)]
    pub const fn from_mut<T: ?Sized>(r: &'a mut T) -> Ptr<'a> {
        Ptr(NonNull::from_mut(r).cast(), PhantomData)
    }

    /// Gets the underlying pointer, erasing the associated lifetime.
    #[inline(always)]
    pub const fn as_ptr(self) -> *const u8 {
        self.0.as_ptr()
    }

    /// Convert this [`Ptr`] into a `&T` with the original lifetime.
    ///
    /// The concrete pointee type is unknown at compile time.
    /// The caller must ensure the pointer is suitable for `T`.
    ///
    /// It is recommended to use [`debug_assert_aligned`] to check
    /// alignment before calling.
    ///
    /// [`debug_assert_aligned`]: Self::debug_assert_aligned
    ///
    /// # Safety
    ///
    /// - `Ptr` points to a valid object.
    /// - `T` must match the actual type of the pointee.
    /// - `Ptr` must be properly aligned for `T`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::Ptr;
    /// let x = 8i32;
    /// let ptr = Ptr::from_ref(&x);
    ///
    /// ptr.debug_assert_aligned::<i32>();
    ///
    /// let rx = unsafe { ptr.deref::<i32>() };
    /// assert_eq!(*rx, 8);
    /// ```
    #[inline(always)]
    pub const unsafe fn deref<T>(self) -> &'a T {
        // SAFETY: Type correct, ptr aligned and pointee valid object.
        unsafe { &*self.0.as_ptr().cast::<T>() }
    }
}

impl<'a, T: ?Sized> From<&'a T> for Ptr<'a> {
    #[inline(always)]
    fn from(val: &'a T) -> Self {
        Self::from_ref(val)
    }
}

impl<'a, T: ?Sized> From<&'a mut T> for Ptr<'a> {
    #[inline(always)]
    fn from(val: &'a mut T) -> Self {
        Self::from_mut(val)
    }
}

impl<'a> From<PtrMut<'a>> for Ptr<'a> {
    #[inline(always)]
    fn from(val: PtrMut<'a>) -> Self {
        Ptr(val.0, PhantomData)
    }
}

// -----------------------------------------------------------------------------
// PtrMut

/// A fully type-erased pointer, similar to `&'a mut dyn Any`.
///
/// # type-erased
///
/// Due to type-erased, we cannot confirm whether it meets the alignment requirements.
/// But when you use this to access targets, you should ensure it is aligned.
///
/// # borrow-like
///
/// - It must always point to a valid value of whatever the pointee type is.
/// - The lifetime `'a` accurately represents how long the pointer is valid for.
///
/// # mutable and exclusive
///
/// It cannot be cloned, and the caller must comply with Rust alias rules.
///
/// Usually, Rust's borrow checker can ensure this through their lifetime.
///
/// # Examples
///
/// ```
/// # use zlim_ptr::PtrMut;
/// let mut x = 8i32;
/// let mut ptr = PtrMut::from_mut(&mut x);
///
/// let rx = unsafe { ptr.as_mut::<i32>() };
/// *rx += 2;
/// assert_eq!(*rx, 10);
/// ```
#[repr(transparent)]
pub struct PtrMut<'a>(NonNull<u8>, PhantomData<&'a u8>);

impl_ptr!(PtrMut);

impl<'a> PtrMut<'a> {
    /// Create a `PtrMut` from a raw `NonNull<u8>` pointer.
    ///
    /// # Safety
    ///
    /// - The data pointed to by this `ptr` must be valid for writes.
    /// - The provided lifetime `'a` must be valid for the pointee.
    /// - `ptr` must point to a valid object of the intended pointee type.
    #[inline(always)]
    pub const unsafe fn new(ptr: NonNull<u8>) -> PtrMut<'a> {
        PtrMut(ptr, PhantomData)
    }

    /// Creates a `PtrMut` from a mutable reference with same lifetime.
    ///
    /// This is safe because the lifetime provided by the reference
    /// must be correct.
    ///
    /// The Rust's borrow checker ensures that mutable references
    /// cannot be used when `PtrMut` is active.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::PtrMut;
    /// let mut x = 8i32;
    /// let ptr = PtrMut::from_mut(&mut x);
    /// ```
    #[inline(always)]
    pub const fn from_mut<T: ?Sized>(val: &'a mut T) -> PtrMut<'a> {
        PtrMut(NonNull::from_mut(val).cast(), PhantomData)
    }

    /// Gets the underlying pointer, erasing the associated lifetime.
    #[inline(always)]
    pub const fn as_ptr(&self) -> *mut u8 {
        self.0.as_ptr()
    }

    /// Get a `&T` from this [`PtrMut`] with a borrowing lifetime.
    ///
    /// The returned lifetime is tied to `&self`, not the original `'a`.
    ///
    /// Rust borrow checker ensures [`PtrMut`] cannot be used
    /// when returned reference is active.
    ///
    /// The concrete pointee type is unknown at compile time.
    /// The caller must ensure the pointer is suitable for `T`.
    ///
    /// It is recommended to use [`debug_assert_aligned`]
    /// to check alignment before calling.
    ///
    /// [`debug_assert_aligned`]: Self::debug_assert_aligned
    ///
    /// # Safety
    /// - Self must be properly aligned for type `T`.
    /// - `T` must be the correct compatible type pointed to by self.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::PtrMut;
    /// let mut x = 8;
    /// let ptr = PtrMut::from_mut(&mut x);
    ///
    /// let rx = unsafe{ ptr.as_ref::<i32>() };
    /// assert_eq!(*rx, 8);
    /// ```
    #[inline(always)]
    pub const unsafe fn as_ref<T>(&self) -> &'_ T {
        // '_ instead of 'a
        // SAFETY: Type correct, ptr aligned and pointee valid object.
        unsafe { &*self.0.as_ptr().cast::<T>() }
    }

    /// Get a `&mut T` from this [`PtrMut`] with a borrowing lifetime.
    ///
    /// The returned lifetime is tied to `&mut self`, not the original `'a`.
    ///
    /// Rust borrow checker ensures [`PtrMut`] cannot be used
    /// when returned reference is active.
    ///
    /// The concrete pointee type is unknown at compile time.
    /// The caller must ensure the pointer is suitable for `T`.
    ///
    /// It is recommended to use [`debug_assert_aligned`]
    /// to check alignment before calling.
    ///
    /// [`debug_assert_aligned`]: Self::debug_assert_aligned
    ///
    /// # Safety
    /// - Self must be properly aligned for type `T`.
    /// - `T` must be the correct compatible type pointed to by self.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::PtrMut;
    /// let mut x = 8;
    /// let mut ptr = PtrMut::from_mut(&mut x);
    ///
    /// let rx = unsafe{ ptr.as_mut::<i32>() };
    /// *rx += 2;
    /// assert_eq!(*rx, 10);
    /// ```
    #[inline(always)]
    pub const unsafe fn as_mut<T>(&mut self) -> &'_ mut T {
        // '_ instead of 'a
        // SAFETY: Type correct, ptr aligned and pointee valid object.
        unsafe { &mut *self.0.as_ptr().cast::<T>() }
    }

    /// Gets a [`Ptr`] from self with a borrowing lifetime.
    ///
    /// It's safe because borrow checker ensure [`PtrMut`]
    /// cannot be used when [`Ptr`] is active.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::{PtrMut, Ptr};
    /// fn foo(ptr: Ptr<'_>) { /* ... */ }
    ///
    /// let mut x = 5;
    /// let mut pm = PtrMut::from(&mut x);
    ///
    /// foo(pm.borrow());
    /// ```
    #[inline(always)]
    pub const fn borrow(&self) -> Ptr<'_> {
        // '_ instead of 'a
        Ptr(self.0, PhantomData)
    }

    /// Gets a [`PtrMut`] from self with a borrowing lifetime.
    ///
    /// It's safe because borrow checker ensure the old [`PtrMut`]
    /// cannot be used when new [`PtrMut`] is active.
    ///
    /// The pointer itself needs to be mutable,
    /// because we need to use borrow checker to ensure validity.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::PtrMut;
    /// fn foo(ptr: PtrMut<'_>) { /* ... */ }
    ///
    /// let mut x = 5;
    /// let mut pm = PtrMut::from(&mut x);
    ///
    /// foo(pm.reborrow());
    /// ```
    #[inline(always)]
    pub const fn reborrow(&mut self) -> PtrMut<'_> {
        // '_ instead of 'a
        PtrMut(self.0, PhantomData)
    }

    /// Convert this [`PtrMut`] into a `&mut T` with the **same** lifetime.
    ///
    /// If you need to reuse `PtrMut`, consider [`as_mut`](PtrMut::as_mut) or
    /// convert returned reference to a new `PtrMut`.
    ///
    /// The concrete pointee type is unknown at compile time.
    /// The caller must ensure the pointer is suitable for `T`.
    ///
    /// It is recommended to use [`debug_assert_aligned`]
    /// to check alignment before calling.
    ///
    /// [`debug_assert_aligned`]: Self::debug_assert_aligned
    ///
    /// # Safety
    /// - Self must be properly aligned for type `T`.
    /// - `T` must be the correct compatible type pointed to by self.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::PtrMut;
    /// let mut x = 8;
    /// let mut ptr = PtrMut::from_mut(&mut x);
    ///
    /// let rx: &mut i32 = unsafe{ ptr.deref() };
    /// *rx += 2;
    /// assert_eq!(*rx, 10);
    /// ```
    #[inline(always)]
    pub const unsafe fn deref<T>(self) -> &'a mut T {
        // SAFETY: Type correct, ptr aligned and pointee valid object.
        unsafe { &mut *self.0.as_ptr().cast::<T>() }
    }

    /// Convert this [`PtrMut`] into a [`OwningPtr`] with the **same** lifetime.
    ///
    /// This is typically used for dropping data.
    ///
    /// # Safety
    /// - The caller needs to avoid double-dropped.
    #[inline(always)]
    pub const unsafe fn promote(self) -> OwningPtr<'a> {
        OwningPtr(self.0, PhantomData)
    }
}

impl<'a, T: ?Sized> From<&'a mut T> for PtrMut<'a> {
    #[inline(always)]
    fn from(val: &'a mut T) -> Self {
        Self::from_mut(val)
    }
}

// -----------------------------------------------------------------------------
// OwningPtr

/// A fully type-erased pointer, similar to `&'a mut ManuallyDrop<dyn Any>`.
///
/// # Ownership
///
/// This pointer is **not** responsible for freeing the memory pointed to by this pointer,
/// as it usually be pointing to an element in a `Vec` or to a local in a function etc.
///
/// Conceptually represents ownership of whatever data is being pointed to.
/// Therefore, users need to ensure its [`Drop::drop`] will be called
/// (readout the data or call [`drop_as`](Self::drop_as) manually).
///
/// # type-erased
///
/// Due to type-erased, we cannot confirm whether it meets the alignment requirements.
/// But when you use this to access targets, you should ensure it is aligned.
///
/// # borrow-like
///
/// - It must always point to a valid value of whatever the pointee type is.
/// - The lifetime `'a` accurately represents how long the pointer is valid for.
///
/// # mutable and exclusive
///
/// It cannot be cloned, and the caller must comply with Rust alias rules.
///
/// Usually, Rust's borrow checker can ensure this through their lifetime.
///
/// # Examples
///
/// ```
/// # use zlim_ptr::OwningPtr;
/// use core::mem::ManuallyDrop;
///
/// // use ManuallyDrop to avoid duplicate drop
/// let mut x = ManuallyDrop::new("hello".to_string());
/// let mut ptr = OwningPtr::from_value(&mut x);
///
/// ptr.debug_assert_aligned::<String>();
/// let rx = unsafe { ptr.as_mut::<String>() };
/// rx.push_str(" world");
///
/// // readout ownership
/// let x = unsafe { ptr.read::<String>() };
/// assert_eq!(x, "hello world");
/// ```
#[repr(transparent)]
pub struct OwningPtr<'a>(NonNull<u8>, PhantomData<&'a u8>);

impl_ptr!(OwningPtr);

impl<'a> OwningPtr<'a> {
    /// Create a `OwningPtr` from a raw `NonNull<u8>` pointer.
    ///
    /// # Safety
    ///
    /// - The data pointed to by this `ptr` must be valid for writes.
    /// - The provided lifetime `'a` must be valid for the pointee.
    /// - `ptr` must point to a valid object of the intended pointee type.
    /// - The data pointed to should be wrapped by [`ManuallyDrop`].
    /// - The caller needs to drop the data correctly through `OwningPtr`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::OwningPtr;
    /// # use core::{ptr::NonNull, mem::ManuallyDrop};
    /// let mut x = "1234".to_string();
    /// let mut x = ManuallyDrop::new(x);
    ///
    /// let ptr: OwningPtr<'_> = unsafe {
    ///     OwningPtr::new(NonNull::from_mut(&mut x).cast())
    /// };
    ///
    /// // do something
    ///
    /// unsafe{ ptr.drop_as::<String>(); }
    /// ```
    #[inline(always)]
    pub const unsafe fn new(inner: NonNull<u8>) -> OwningPtr<'a> {
        Self(inner, PhantomData)
    }

    /// Creates a `OwningPtr` from a mutable reference with same lifetime.
    ///
    /// This is safe because the pointee type is wrapped by [`ManuallyDrop`],
    /// will not be released again.
    /// And the lifetime provided by the reference must be correct.
    ///
    /// The Rust's borrow checker ensures that mutable references
    /// cannot be used when `OwningPtr` is active.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::OwningPtr;
    /// # use core::mem::ManuallyDrop;
    /// let mut x = ManuallyDrop::new("123".to_string());
    /// let ptr = OwningPtr::from_value(&mut x);
    /// ```
    #[inline(always)]
    pub const fn from_value<T>(r: &'a mut ManuallyDrop<T>) -> OwningPtr<'a> {
        Self(NonNull::from_mut(r).cast(), PhantomData)
    }

    /// Consumes the [`OwningPtr`] to drop the underlying data of type `T`.
    ///
    /// The caller must ensure the pointer is suitable for `T`.
    ///
    /// It is recommended to use [`debug_assert_aligned`]
    /// to check alignment before calling.
    ///
    /// [`debug_assert_aligned`]: Self::debug_assert_aligned
    ///
    /// # Safety
    /// - `ptr` must be properly aligned for type `T`.
    /// - `T` must be the erased pointee type for this [`OwningPtr`].
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::OwningPtr;
    /// # use core::{ptr::NonNull, mem::ManuallyDrop};
    /// let mut x = ManuallyDrop::new("1234".to_string());
    /// let ptr = OwningPtr::from_value(&mut x);
    ///
    /// // do something
    ///
    /// unsafe{ ptr.drop_as::<String>(); }
    /// ```
    #[inline(always)]
    pub unsafe fn drop_as<T>(self) {
        // SAFETY: see function docs.
        unsafe { ptr::drop_in_place::<T>(self.0.as_ptr().cast::<T>()) }
    }

    /// Consumes the [`OwningPtr`] to obtain ownership of the
    /// underlying data of type `T`.
    ///
    /// The caller must ensure the pointer is suitable for `T`.
    ///
    /// It is recommended to use [`debug_assert_aligned`]
    /// to check alignment before calling.
    ///
    /// [`debug_assert_aligned`]: Self::debug_assert_aligned
    ///
    /// # Safety
    /// - `ptr` must be properly aligned for type `T`.
    /// - `T` must be the erased pointee type for this [`OwningPtr`].
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::OwningPtr;
    /// # use core::{ptr::NonNull, mem::ManuallyDrop};
    /// let mut x = ManuallyDrop::new("1234".to_string());
    /// let ptr = OwningPtr::from_value(&mut x);
    ///
    /// // do something
    ///
    /// let x = unsafe{ ptr.read::<String>() };
    /// ```
    #[inline(always)]
    pub const unsafe fn read<T>(self) -> T {
        // SAFETY: see function docs.
        unsafe { ptr::read(self.0.as_ptr() as *mut T) }
    }

    /// Writes `value` into the memory pointed to by this pointer.
    ///
    /// This uses `ptr::write`, so it does **not** read or drop any existing value.
    ///
    /// # Safety
    /// - `self` must be valid for writes of a `T`.
    /// - The pointer must be properly aligned for `T`.
    /// - The pointee type must be `T`.
    /// - The caller must ensure no double-drop or leak of any overwritten value.
    #[inline(always)]
    pub const unsafe fn write<T>(&mut self, value: T) {
        unsafe {
            ptr::write(self.0.as_ptr() as *mut T, value);
        }
    }

    /// Creates an `OwningPtr` to a field at `offset` bytes from this pointer.
    ///
    /// The offset is in raw bytes because the pointer is type-erased.
    ///
    /// # Safety
    /// - `offset` must keep the pointer within the same allocated object.
    /// - The resulting pointer must be valid and properly aligned for its pointee.
    /// - The caller must ensure the pointee type at the offset is correct.
    /// - The resulting pointer must not outlive the original allocation.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::OwningPtr;
    /// struct Foo {
    ///     id: u32,
    ///     data: [u8; 10],
    /// }
    ///
    /// OwningPtr::make(Foo { id: 1, data: [0; 10] }, |mut ptr| unsafe {
    ///     ptr.take_field(core::mem::offset_of!(Foo, data)).read::<[u8; 10]>()
    /// });
    /// ```
    #[inline(always)]
    pub const unsafe fn take_field(&mut self, offset: usize) -> OwningPtr<'_> {
        unsafe { OwningPtr(self.0.byte_add(offset), PhantomData) }
    }

    /// Consumes a value and creates an [`OwningPtr`] to it
    /// while ensuring a double drop does not happen.
    ///
    /// This function cannot be used to create and return [`OwningPtr`],
    /// because the pointee value will be consumed within the function.
    ///
    /// We use inlining to avoid copying large data.
    ///
    /// # Safety
    /// - `OwningPtr` should be consumed in function `f`.
    /// - `drop` or `read` should be manually called.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_ptr::OwningPtr;
    /// let s = "123".to_string();
    ///
    /// let ret = OwningPtr::make(s, |ptr| {
    ///      unsafe{ ptr.read::<String>()  + "X" }
    /// });
    /// assert_eq!(ret, "123X");
    /// ```
    #[inline(always)]
    pub fn make<T, F: FnOnce(OwningPtr<'_>) -> R, R>(val: T, f: F) -> R {
        let mut val = ManuallyDrop::new(val);
        f(OwningPtr(NonNull::from_mut(&mut val).cast(), PhantomData))
    }

    /// Gets the underlying pointer, erasing the associated lifetime.
    #[inline(always)]
    pub const fn as_ptr(&self) -> *mut u8 {
        self.0.as_ptr()
    }

    /// Gets an [`Ptr`] from self with **smaller** lifetime.
    ///
    /// Lifetime will be consistent with `&OwningPtr`, not generic `'a`.
    ///
    /// It's safe because borrow checker ensure the old [`OwningPtr`]
    /// cannot be used when new [`Ptr`] is active.
    #[inline(always)]
    pub const fn borrow(&self) -> Ptr<'_> {
        Ptr(self.0, PhantomData)
    }

    /// Gets a [`PtrMut`] from self with **smaller** lifetime.
    ///
    /// Lifetime will be consistent with `&mut OwningPtr`, not generic `'a`.
    ///
    /// It's safe because borrow checker ensure the old [`OwningPtr`]
    /// cannot be used when new [`PtrMut`] is active.
    #[inline(always)]
    pub const fn borrow_mut(&mut self) -> PtrMut<'_> {
        PtrMut(self.0, PhantomData)
    }

    /// Get a `&T` from this [`OwningPtr`] with a borrowing lifetime.
    ///
    /// Lifetime will be consistent with `&OwningPtr`, not generic `'a`.
    ///
    /// Rust borrow checker ensures [`OwningPtr`] cannot be used
    /// when returned reference is active.
    ///
    /// The caller must ensure the pointer is suitable for `T`.
    ///
    /// It is recommended to use [`debug_assert_aligned`]
    /// to check alignment before calling.
    ///
    /// [`debug_assert_aligned`]: Self::debug_assert_aligned
    ///
    /// # Safety
    /// - Self must be properly aligned for type `T`.
    /// - `T` must be the correct compatible type pointed to by self.
    #[inline(always)]
    pub const unsafe fn as_ref<T>(&self) -> &'_ T {
        // '_ instead of 'a
        // SAFETY: Type correct, ptr aligned and pointee valid object.
        unsafe { &*self.0.as_ptr().cast::<T>() }
    }

    /// Get a `&mut T` from this [`OwningPtr`] with a borrowing lifetime.
    ///
    /// Lifetime will be consistent with `&mut OwningPtr`, not generic `'a`.
    ///
    /// Rust borrow checker ensures [`OwningPtr`] cannot be used
    /// when returned reference is active.
    ///
    /// The caller must ensure the pointer is suitable for `T`.
    ///
    /// It is recommended to use [`debug_assert_aligned`]
    /// to check alignment before calling.
    ///
    /// [`debug_assert_aligned`]: Self::debug_assert_aligned
    ///
    /// # Safety
    /// - Self must be properly aligned for type `T`.
    /// - `T` must be the correct compatible type pointed to by self.
    #[inline(always)]
    pub const unsafe fn as_mut<T>(&mut self) -> &'_ mut T {
        // '_ instead of 'a
        // SAFETY: Type correct, ptr aligned and pointee valid object.
        unsafe { &mut *self.0.as_ptr().cast::<T>() }
    }
}

// -----------------------------------------------------------------------------
// Macros

/// An auxiliary macro that wraps the target value with
/// `ManuallyDrop` and uses `OwningPtr` to point to it.
///
/// This can be used as a replacement for `make` to reduce
/// compilation overhead.
///
/// # Examples
///
/// ```
/// use zlim_ptr::{OwningPtr, into_owning};
///
/// let value = 7_i32;
/// into_owning!(value as ptr);
/// let out = unsafe { ptr.read::<i32>() };
/// assert_eq!(out, 7);
/// ```
#[macro_export]
macro_rules! into_owning {
    ($data:ident) => {
        let mut $data = ::core::mem::ManuallyDrop::new($data);
        let $data = $crate::OwningPtr::from_value(&mut $data);
    };
    ($data:ident as $ptr:ident) => {
        let mut $data = ::core::mem::ManuallyDrop::new($data);
        let $ptr = $crate::OwningPtr::from_value(&mut $data);
    };
}
