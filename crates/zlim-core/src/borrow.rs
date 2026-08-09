use core::fmt::{Debug, Formatter};
use core::iter::FusedIterator;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;

use zlim_ptr::Slice as ThinSlice;
use zlim_ptr::SliceMut as ThinSliceMut;
use zlim_ptr::{Ptr, PtrMut};

use crate::tick::{DetectChanges, DetectChangesMut};
use crate::tick::{Tick, TicksMut, TicksRef};
use crate::tick::{TicksSliceMut, TicksSliceRef};

use crate::resource::Resource;

// -----------------------------------------------------------------------------
// Untyped & UntypedSlice
// -----------------------------------------------------------------------------

// --------------------------------------------------------------------
// Untyped

/// An untyped shared reference to a component or resource.
///
/// Provides read-only access without knowing the concrete type.
///
/// This wrapper contains raw pointer metadata plus change ticks. Fields are
/// intentionally public to support custom system-parameter implementations.
///
/// These pointers are typically obtained from table accessors.
pub struct UntypedRef<'w> {
    pub value: Ptr<'w>,
    pub ticks: TicksRef<'w>,
}

// --------------------------------------------------------------------
// UntypedMut

/// An untyped exclusive reference to a component or resource.
///
/// Provides mutable access without knowing the concrete type.
///
/// This wrapper contains raw pointer metadata plus change ticks. Fields are
/// intentionally public to support custom system-parameter implementations.
///
/// These pointers are typically obtained from table accessors.
pub struct UntypedMut<'w> {
    pub value: PtrMut<'w>,
    pub ticks: TicksMut<'w>,
}

// --------------------------------------------------------------------
// UntypedSliceRef

/// An untyped shared reference to a slice of components.
///
/// Provides read-only access to multiple components without knowing their type.
///
/// This is currently used by type-erased access paths that still need slice
/// semantics and per-element change tracking.
pub struct UntypedSliceRef<'w> {
    pub value: Ptr<'w>,
    pub ticks: TicksSliceRef<'w>,
}

// --------------------------------------------------------------------
// UntypedSliceMut

/// An untyped exclusive reference to a slice of components.
///
/// Provides mutable access to multiple components without knowing their type.
///
/// This is currently used by type-erased mutable access paths that still need
/// slice semantics and per-element change tracking.
pub struct UntypedSliceMut<'w> {
    pub value: PtrMut<'w>,
    pub ticks: TicksSliceMut<'w>,
}

// --------------------------------------------------------------------
// UntypedRef : Method Implementation

impl<'w> UntypedRef<'w> {
    /// Consumes `self` and returns the inner [`Ptr`].
    #[inline(always)]
    pub fn into_inner(self) -> Ptr<'w> {
        self.value
    }

    /// Creates a copy with the same lifetime.
    #[inline(always)]
    pub fn reborrow(&self) -> UntypedRef<'w> {
        Self {
            value: self.value,
            ticks: self.ticks,
        }
    }
}

impl Debug for UntypedRef<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("UntypedRef")
            .field(&self.value.as_ptr())
            .finish()
    }
}

// --------------------------------------------------------------------
// UntypedMut : Method Implementation

impl<'w> UntypedMut<'w> {
    /// Consumes `self` and returns the inner [`PtrMut`].
    ///
    /// This function does not set the changed flag.
    #[inline(always)]
    pub fn into_inner(self) -> PtrMut<'w> {
        self.value
    }

    /// Returns a shorter-lived version of self.
    ///
    /// This function does not set the changed flag.
    #[inline(always)]
    pub fn reborrow(&mut self) -> UntypedMut<'_> {
        UntypedMut {
            value: self.value.reborrow(),
            ticks: TicksMut {
                added: self.ticks.added,
                changed: self.ticks.changed,
                last_run: self.ticks.last_run,
                this_run: self.ticks.this_run,
            },
        }
    }
}

impl Debug for UntypedMut<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("UntypedMut")
            .field(&self.value.as_ptr())
            .finish()
    }
}

// --------------------------------------------------------------------
// UntypedSliceRef : Method Implementation

impl<'w> UntypedSliceRef<'w> {
    /// Consumes `self` and returns the inner [`Ptr`].
    #[inline]
    pub fn into_inner(self) -> Ptr<'w> {
        self.value
    }

    /// Returns the length of the slice.
    #[inline]
    pub fn len(&self) -> usize {
        self.ticks.length
    }

    /// Returns `true` if the slice is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.ticks.length == 0
    }
}

impl Debug for UntypedSliceRef<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UntypedSliceRef")
            .field("ptr", &self.value.as_ptr())
            .field("len", &self.ticks.length)
            .finish()
    }
}

// --------------------------------------------------------------------
// UntypedSliceMut : Method Implementation

impl<'w> UntypedSliceMut<'w> {
    /// Consumes `self` and returns the inner [`PtrMut`].
    #[inline]
    pub fn into_inner(self) -> PtrMut<'w> {
        self.value
    }

    /// Returns the length of the slice.
    #[inline]
    pub fn len(&self) -> usize {
        self.ticks.length
    }

    /// Returns `true` if the slice is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.ticks.length == 0
    }
}

impl Debug for UntypedSliceMut<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UntypedSliceMut")
            .field("ptr", &self.value.as_ptr())
            .field("len", &self.ticks.length)
            .finish()
    }
}

// --------------------------------------------------------------------
// From

impl<'w> From<UntypedMut<'w>> for UntypedRef<'w> {
    #[inline]
    fn from(other: UntypedMut<'w>) -> Self {
        UntypedRef {
            value: other.value.into(),
            ticks: other.ticks.into(),
        }
    }
}

impl<'w> From<UntypedSliceMut<'w>> for UntypedSliceRef<'w> {
    #[inline]
    fn from(other: UntypedSliceMut<'w>) -> Self {
        UntypedSliceRef {
            value: other.value.into(),
            ticks: other.ticks.into(),
        }
    }
}

// --------------------------------------------------------------------
// Change Detection

macro_rules! impl_untyped_change_detection {
    ($name:ident) => {
        impl<'w> DetectChanges for $name<'w> {
            #[inline]
            fn is_added(&self) -> bool {
                self.ticks
                    .added
                    .is_newer_than(self.ticks.last_run, self.ticks.this_run)
            }

            #[inline]
            fn is_changed(&self) -> bool {
                self.ticks
                    .changed
                    .is_newer_than(self.ticks.last_run, self.ticks.this_run)
            }

            #[inline(always)]
            fn changed_tick(&self) -> Tick {
                *self.ticks.changed
            }

            #[inline(always)]
            fn added_tick(&self) -> Tick {
                *self.ticks.added
            }
        }
    };
}

impl_untyped_change_detection!(UntypedRef);
impl_untyped_change_detection!(UntypedMut);

impl<'w> DetectChangesMut for UntypedMut<'w> {
    type Value<'a>
        = PtrMut<'a>
    where
        Self: 'a;

    #[inline(always)]
    fn bypass(&mut self) -> Self::Value<'_> {
        self.value.reborrow()
    }

    #[inline(always)]
    fn set_added(&mut self) {
        *self.ticks.added = self.ticks.this_run;
    }

    #[inline(always)]
    fn set_changed(&mut self) {
        *self.ticks.changed = self.ticks.this_run;
    }
}

// -----------------------------------------------------------------------------
// Ref & Mut
// -----------------------------------------------------------------------------

// --------------------------------------------------------------------
// Ref

/// A generic shared reference to a component or resource.
///
/// Provides read-only access with change detection.
pub struct Ref<'w, T: ?Sized> {
    pub(crate) value: &'w T,
    pub(crate) ticks: TicksRef<'w>,
}

// --------------------------------------------------------------------
// Mut

/// A generic exclusive reference to a component or resource.
///
/// Provides mutable access with change detection.
pub struct Mut<'w, T: ?Sized> {
    pub(crate) value: &'w mut T,
    pub(crate) ticks: TicksMut<'w>,
}

// --------------------------------------------------------------------
// SliceMut

/// A shared reference to a slice of components.
///
/// Provides read-only access to multiple components of the same type.
///
/// This is currently a low-level wrapper used by storage/query internals.
/// It exposes contiguous read access plus change ticks for each element.
pub struct SliceRef<'w, T> {
    pub(crate) value: ThinSlice<'w, T>,
    pub(crate) ticks: TicksSliceRef<'w>,
}

// --------------------------------------------------------------------
// SliceRef

/// An exclusive reference to a slice of components.
///
/// Provides mutable access to multiple components of the same type.
///
/// This is currently a low-level wrapper used by storage/query internals.
/// It exposes contiguous mutable access plus change ticks for each element.
pub struct SliceMut<'w, T> {
    pub(crate) value: ThinSliceMut<'w, T>,
    pub(crate) ticks: TicksSliceMut<'w>,
}

// --------------------------------------------------------------------
// From Untyped

impl<'w> UntypedRef<'w> {
    /// Specifies the reference type and converts self to a [`Ref`].
    ///
    /// # Safety
    /// `T` must be the erased pointee type for this [`UntypedRef`].
    #[inline(always)]
    pub unsafe fn with_type<T>(self) -> Ref<'w, T> {
        self.value.debug_assert_aligned::<T>();
        Ref {
            value: unsafe { self.value.deref::<T>() },
            ticks: self.ticks,
        }
    }
}

impl<'w> UntypedMut<'w> {
    /// Specifies the reference type and converts `self` to a [`Mut`].
    ///
    /// This function does not set the changed flag.
    ///
    /// # Safety
    /// `T` must be the erased pointee type for this [`UntypedMut`].
    #[inline(always)]
    pub unsafe fn with_type<T>(self) -> Mut<'w, T> {
        self.value.debug_assert_aligned::<T>();
        Mut {
            value: unsafe { self.value.deref::<T>() },
            ticks: self.ticks,
        }
    }
}

impl<'w> UntypedSliceRef<'w> {
    /// Specifies the reference type and converts `self` to a [`SliceRef`].
    ///
    /// # Safety
    /// `T` must be the erased pointee type for this [`UntypedSliceRef`].
    #[inline]
    pub unsafe fn with_type<T>(self) -> SliceRef<'w, T> {
        self.value.debug_assert_aligned::<T>();
        SliceRef {
            value: unsafe { ThinSlice::from_raw(self.value.into_inner().cast::<T>()) },
            ticks: self.ticks,
        }
    }
}

impl<'w> UntypedSliceMut<'w> {
    /// Specifies the reference type and converts `self` to a [`SliceMut`].
    ///
    /// # Safety
    /// `T` must be the erased pointee type for this [`UntypedSliceMut`].
    #[inline]
    pub unsafe fn with_type<T>(self) -> SliceMut<'w, T> {
        self.value.debug_assert_aligned::<T>();
        SliceMut {
            value: unsafe { ThinSliceMut::from_raw(self.value.into_inner().cast::<T>()) },
            ticks: self.ticks,
        }
    }
}

// --------------------------------------------------------------------
// Debug

impl<T: Debug + ?Sized> Debug for Ref<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Ref").field(&self.value).finish()
    }
}

impl<T: Debug + ?Sized> Debug for Mut<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Mut").field(&self.value).finish()
    }
}

impl<T: Debug> Debug for SliceRef<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("SliceRef")
            .field(&unsafe { self.value.deref(self.ticks.length) })
            .finish()
    }
}

impl<T: Debug> Debug for SliceMut<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("SliceMut")
            .field(&unsafe { self.value.as_ref(self.ticks.length) })
            .finish()
    }
}

// --------------------------------------------------------------------
// From

impl<'w, T: ?Sized> From<Mut<'w, T>> for Ref<'w, T> {
    #[inline]
    fn from(other: Mut<'w, T>) -> Self {
        Self {
            value: other.value,
            ticks: other.ticks.into(),
        }
    }
}

impl<'w, T: ?Sized> From<Ref<'w, T>> for UntypedRef<'w> {
    #[inline]
    fn from(other: Ref<'w, T>) -> Self {
        UntypedRef {
            value: other.value.into(),
            ticks: other.ticks,
        }
    }
}

impl<'w, T: ?Sized> From<Mut<'w, T>> for UntypedMut<'w> {
    #[inline]
    fn from(other: Mut<'w, T>) -> Self {
        UntypedMut {
            value: other.value.into(),
            ticks: other.ticks,
        }
    }
}

impl<'w, T> From<SliceMut<'w, T>> for SliceRef<'w, T> {
    #[inline]
    fn from(other: SliceMut<'w, T>) -> Self {
        SliceRef {
            value: other.value.into(),
            ticks: other.ticks.into(),
        }
    }
}

// --------------------------------------------------------------------
// Ref Methods

impl<'w, 'a, T> IntoIterator for &'a Ref<'w, T>
where
    &'a T: IntoIterator,
{
    type Item = <&'a T as IntoIterator>::Item;
    type IntoIter = <&'a T as IntoIterator>::IntoIter;
    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.value.into_iter()
    }
}

impl<'w, T: ?Sized> Ref<'w, T> {
    /// Consumes self and returns the inner reference `&T` with the same lifetime.
    #[inline(always)]
    pub fn into_inner(self) -> &'w T {
        self.value
    }

    /// Creates a copy with the same lifetime.
    #[inline]
    pub fn reborrow(&self) -> Self {
        Self {
            value: self.value,
            ticks: self.ticks,
        }
    }

    /// Transforms the reference type via a function, preserving the lifetime.
    ///
    /// Returns the generic [`Ref`] container.
    #[inline(always)]
    pub fn map_type<U: ?Sized>(self, f: impl FnOnce(&T) -> &U) -> Ref<'w, U> {
        Ref {
            value: f(self.value),
            ticks: self.ticks,
        }
    }

    /// Transforms the reference type via a function, preserving the lifetime.
    ///
    /// Returns the generic [`Ref`] container, or an error if the transformation fails.
    #[inline]
    pub fn try_map_type<U: ?Sized, E>(
        self,
        f: impl FnOnce(&T) -> Result<&U, E>,
    ) -> Result<Ref<'w, U>, E> {
        let value = f(self.value)?;
        Ok(Ref {
            value,
            ticks: self.ticks,
        })
    }

    /// Dereferences the inner type, e.g., converts `Ref<'a, Box<T>>` to `Ref<'a, T>`.
    ///
    /// Returns the generic [`Ref`] container.
    #[inline]
    pub fn into_deref(self) -> Ref<'w, <T as Deref>::Target>
    where
        T: Deref,
    {
        Ref {
            value: Deref::deref(self.value),
            ticks: self.ticks,
        }
    }
}

// --------------------------------------------------------------------
// Mut Methods

impl<'w, 'a, T> IntoIterator for &'a Mut<'w, T>
where
    &'a T: IntoIterator,
{
    type Item = <&'a T as IntoIterator>::Item;
    type IntoIter = <&'a T as IntoIterator>::IntoIter;
    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.value.into_iter()
    }
}

impl<'w, 'a, T> IntoIterator for &'a mut Mut<'w, T>
where
    &'a mut T: IntoIterator,
{
    type Item = <&'a mut T as IntoIterator>::Item;
    type IntoIter = <&'a mut T as IntoIterator>::IntoIter;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        *self.ticks.changed = self.ticks.this_run;
        self.value.into_iter()
    }
}

impl<'w, T: ?Sized> Mut<'w, T> {
    /// Consumes self and returns the inner reference `&mut T` with the
    /// same lifetime, marking the target as changed.
    #[inline]
    pub fn into_inner(self) -> &'w mut T {
        *self.ticks.changed = self.ticks.this_run;
        self.value
    }

    /// Returns a shorter-lived version of self, with borrow checker guarantees.
    ///
    /// This function does not mark the target as changed.
    #[inline]
    pub fn reborrow(&mut self) -> Mut<'_, T> {
        Mut {
            value: self.value,
            ticks: TicksMut {
                added: self.ticks.added,
                changed: self.ticks.changed,
                last_run: self.ticks.last_run,
                this_run: self.ticks.this_run,
            },
        }
    }

    /// Transforms the reference type via a function, preserving the lifetime.
    ///
    /// Returns the generic [`Mut`] container.
    ///
    /// This function is assumed to only change the type, not modify data.
    /// Modifying data through the mutable reference in the closure is undefined behavior
    /// (data may be modified without triggering change events).
    #[inline]
    pub fn map_type<U: ?Sized>(self, f: impl FnOnce(&mut T) -> &mut U) -> Mut<'w, U> {
        Mut {
            value: f(self.value),
            ticks: self.ticks,
        }
    }

    /// Transforms the reference type via a function, preserving the lifetime.
    ///
    /// Returns the generic [`Mut`] container, or an error if the transformation fails.
    ///
    /// This function is assumed to only change the type, not modify data.
    /// Modifying data through the mutable reference in the closure is undefined behavior
    /// (data may be modified without triggering change events).
    #[inline]
    pub fn try_map_type<U: ?Sized, E>(
        self,
        f: impl FnOnce(&mut T) -> Result<&mut U, E>,
    ) -> Result<Mut<'w, U>, E> {
        let value = f(self.value)?;
        Ok(Mut {
            value,
            ticks: self.ticks,
        })
    }

    /// Dereferences the inner type, e.g., converts `Mut<'a, Box<T>>` to `Mut<'a, T>`.
    ///
    /// Returns the generic [`Mut`] container.
    ///
    /// This function does not set the change flag.
    #[inline]
    pub fn into_deref(self) -> Mut<'w, <T as Deref>::Target>
    where
        T: DerefMut,
    {
        Mut {
            value: DerefMut::deref_mut(self.value),
            ticks: self.ticks,
        }
    }
}

macro_rules! impl_change_detection {
    ($name:ident < $($generics:tt),+ >) => {
        impl<$($generics),* : ?Sized> DetectChanges for $name<$($generics),*> {
            #[inline]
            fn is_added(&self) -> bool {
                self.ticks
                    .added
                    .is_newer_than(self.ticks.last_run, self.ticks.this_run)
            }

            #[inline]
            fn is_changed(&self) -> bool {
                self.ticks
                    .changed
                    .is_newer_than(self.ticks.last_run, self.ticks.this_run)
            }

            #[inline(always)]
            fn changed_tick(&self) -> Tick {
                *self.ticks.changed
            }

            #[inline(always)]
            fn added_tick(&self) -> Tick {
                *self.ticks.added
            }
        }
    };
}

impl_change_detection!(Ref<'w, T>);
impl_change_detection!(Mut<'w, T>);

impl<'w, T: ?Sized> DetectChangesMut for Mut<'w, T> {
    type Value<'a>
        = &'a T
    where
        Self: 'a;

    #[inline(always)]
    fn bypass(&mut self) -> &'_ T {
        self.value
    }

    #[inline(always)]
    fn set_added(&mut self) {
        *self.ticks.added = self.ticks.this_run;
    }

    #[inline(always)]
    fn set_changed(&mut self) {
        *self.ticks.changed = self.ticks.this_run;
    }
}

// --------------------------------------------------------------------
// SliceRef - Methods

impl<'w, T> SliceRef<'w, T> {
    /// Consumes self and returns the inner reference `&T` with the same lifetime.
    #[inline(always)]
    pub fn into_inner(self) -> &'w [T] {
        unsafe { self.value.deref(self.ticks.length) }
    }

    /// Creates a copy with the **same** lifetime.
    ///
    /// Since this is a shared reference, the original and copy do not interfere.
    #[inline(always)]
    pub fn reborrow(&self) -> SliceRef<'w, T> {
        Self {
            value: self.value,
            ticks: self.ticks,
        }
    }
}

impl<'w, T> Deref for SliceRef<'w, T> {
    type Target = [T];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        unsafe { self.value.deref(self.ticks.length) }
    }
}

impl<'w, T> AsRef<[T]> for SliceRef<'w, T> {
    #[inline(always)]
    fn as_ref(&self) -> &[T] {
        unsafe { self.value.deref(self.ticks.length) }
    }
}

/// An iterator over shared references to components in a slice.
pub struct SliceRefIter<'w, T> {
    len: usize,
    value: NonNull<T>,
    added: NonNull<Tick>,
    changed: NonNull<Tick>,
    last_run: Tick,
    this_run: Tick,
    _marker: PhantomData<&'w [T]>,
}

impl<'w, T> Iterator for SliceRefIter<'w, T> {
    type Item = Ref<'w, T>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        }

        unsafe {
            let ret: Ref<'w, T> = Ref {
                value: self.value.as_ref(),
                ticks: TicksRef {
                    added: self.added.as_ref(),
                    changed: self.changed.as_ref(),
                    last_run: self.last_run,
                    this_run: self.this_run,
                },
            };

            self.value = self.value.add(1);
            self.added = self.added.add(1);
            self.changed = self.changed.add(1);
            self.len -= 1;

            Some(ret)
        }
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<T> ExactSizeIterator for SliceRefIter<'_, T> {}
impl<T> FusedIterator for SliceRefIter<'_, T> {}

impl<'w, T> IntoIterator for SliceRef<'w, T> {
    type Item = Ref<'w, T>;
    type IntoIter = SliceRefIter<'w, T>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        SliceRefIter {
            len: self.ticks.length,
            value: self.value.into_inner(),
            added: self.ticks.added.into_inner(),
            changed: self.ticks.changed.into_inner(),
            last_run: self.ticks.last_run,
            this_run: self.ticks.this_run,
            _marker: PhantomData,
        }
    }
}

// --------------------------------------------------------------------
// SliceMut - Methods

impl<'w, T> SliceMut<'w, T> {
    fn mark_all_changed(&mut self) {
        let this_run = self.ticks.this_run;
        let slice = unsafe { self.ticks.changed.as_mut(self.ticks.length) };
        slice.iter_mut().for_each(|it| *it = this_run);
    }

    /// Consumes self and returns the inner reference `&T` with the same lifetime.
    #[inline]
    pub fn into_inner(mut self) -> &'w mut [T] {
        self.mark_all_changed();
        unsafe { self.value.deref(self.ticks.length) }
    }

    /// Returns a shorter-lived version of self, with borrow checker guarantees.
    ///
    /// This function does not mark the target as changed.
    #[inline]
    pub fn reborrow(&mut self) -> SliceMut<'_, T> {
        SliceMut {
            value: self.value.reborrow(),
            ticks: TicksSliceMut {
                length: self.ticks.length,
                added: self.ticks.added.reborrow(),
                changed: self.ticks.changed.reborrow(),
                last_run: self.ticks.last_run,
                this_run: self.ticks.this_run,
            },
        }
    }
}

impl<'w, T> Deref for SliceMut<'w, T> {
    type Target = [T];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        unsafe { self.value.as_ref(self.ticks.length) }
    }
}

impl<'w, T> DerefMut for SliceMut<'w, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.mark_all_changed();
        unsafe { self.value.as_mut(self.ticks.length) }
    }
}

impl<'w, T> AsRef<[T]> for SliceMut<'w, T> {
    #[inline(always)]
    fn as_ref(&self) -> &[T] {
        unsafe { self.value.as_ref(self.ticks.length) }
    }
}

impl<'w, T> AsMut<[T]> for SliceMut<'w, T> {
    #[inline]
    fn as_mut(&mut self) -> &mut [T] {
        self.mark_all_changed();
        unsafe { self.value.as_mut(self.ticks.length) }
    }
}

/// An iterator over mutable references to components in a slice.
pub struct SliceMutIter<'w, T> {
    len: usize,
    value: NonNull<T>,
    added: NonNull<Tick>,
    changed: NonNull<Tick>,
    last_run: Tick,
    this_run: Tick,
    _marker: PhantomData<&'w [T]>,
}

impl<'w, T> Iterator for SliceMutIter<'w, T> {
    type Item = Mut<'w, T>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        }

        unsafe {
            let ret: Mut<'w, T> = Mut {
                value: self.value.as_mut(),
                ticks: TicksMut {
                    added: self.added.as_mut(),
                    changed: self.changed.as_mut(),
                    last_run: self.last_run,
                    this_run: self.this_run,
                },
            };

            self.value = self.value.add(1);
            self.added = self.added.add(1);
            self.changed = self.changed.add(1);
            self.len -= 1;

            Some(ret)
        }
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<T> ExactSizeIterator for SliceMutIter<'_, T> {}
impl<T> FusedIterator for SliceMutIter<'_, T> {}

impl<'w, T> IntoIterator for SliceMut<'w, T> {
    type Item = Mut<'w, T>;
    type IntoIter = SliceMutIter<'w, T>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        SliceMutIter {
            len: self.ticks.length,
            value: self.value.into_inner(),
            added: self.ticks.added.into_inner(),
            changed: self.ticks.changed.into_inner(),
            last_run: self.ticks.last_run,
            this_run: self.ticks.this_run,
            _marker: PhantomData,
        }
    }
}

// -----------------------------------------------------------------------------
// Service & NonSend
// -----------------------------------------------------------------------------

// -----------------------------------------------------------------------------
// Res
// -----------------------------------------------------------------------------

/// A shared reference to a `Send + Sync` resource with change detection.
///
/// This is the read-only resource parameter for systems. `Res` always includes
/// change detection ticks — there is no separate thin variant.
///
/// # Examples
///
/// ```ignore
/// # use zlim_core::resource::Resource;
/// # use zlim_core::borrow::Res;
/// # use zlim_core::tick::DetectChanges;
/// struct Logger;
/// impl Resource for Logger {}
///
/// fn system_a(logger: Res<Logger>) {
///     if logger.is_changed() {
///         println!("resource was changed!");
///     }
/// }
/// ```
pub struct Res<'w, T: Resource + Sync> {
    pub(crate) value: &'w T,
    pub(crate) ticks: TicksRef<'w>,
}

// -----------------------------------------------------------------------------
// ResMut
// -----------------------------------------------------------------------------

/// An exclusive reference to a `Send` resource with change detection.
///
/// This is the mutable resource parameter for systems. Mutable access
/// participates in the ECS borrow rules, so no other system can read or
/// write the same resource at the same time.
///
/// # Examples
///
/// ```ignore
/// # use zlim_core::resource::Resource;
/// # use zlim_core::borrow::ResMut;
/// # use zlim_core::tick::DetectChanges;
/// struct Logger;
/// impl Resource for Logger {}
///
/// fn system_a(mut logger: ResMut<Logger>) {
///     let _inner: &mut Logger = &mut logger;
///     assert!(logger.is_changed());
/// }
/// ```
pub struct ResMut<'w, T: Resource + Send> {
    pub(crate) value: &'w mut T,
    pub(crate) ticks: TicksMut<'w>,
}

// -----------------------------------------------------------------------------
// NonSend
// -----------------------------------------------------------------------------

/// A shared reference to a `!Sync` resource with change detection.
///
/// `NonSend` parameters can only be fetched on the world's main thread.
/// Use this when the resource type cannot be sent across threads but
/// immutable access with change detection is needed.
pub struct NonSend<'w, T: Resource> {
    pub(crate) value: &'w T,
    pub(crate) ticks: TicksRef<'w>,
}

// -----------------------------------------------------------------------------
// NonSendMut
// -----------------------------------------------------------------------------

/// An exclusive reference to a `!Send` resource with change detection.
///
/// This mutable view is restricted to the main thread and prevents any
/// concurrent access to the same resource while the system runs.
pub struct NonSendMut<'w, T: Resource> {
    pub(crate) value: &'w mut T,
    pub(crate) ticks: TicksMut<'w>,
}

// -----------------------------------------------------------------------------
// impl_debug resource
// -----------------------------------------------------------------------------

macro_rules! impl_resource_debug {
    ($name:ident < $($generics:tt),+ > $($traits:ident)*) => {
        impl<$($generics),* : Resource $(+ $traits)*> Debug for $name<$($generics),*>
        where
            T: Debug,
        {
            fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
                f.debug_tuple(stringify!($name))
                    .field(&self.value)
                    .finish()
            }
        }
    };
}

impl_resource_debug!(Res<'w, T> Resource Sync);
impl_resource_debug!(ResMut<'w, T> Resource Send);
impl_resource_debug!(NonSend<'w, T> Resource);
impl_resource_debug!(NonSendMut<'w, T> Resource);

// -----------------------------------------------------------------------------
// impl_deref resource
// -----------------------------------------------------------------------------

macro_rules! impl_resource_deref {
    ($name:ident < $($generics:tt),+ > $($traits:ident)*) => {
        impl<$($generics),* : Resource $(+ $traits)*> Deref for $name<$($generics),*> {
            type Target = T;

            #[inline(always)]
            fn deref(&self) -> &Self::Target {
                self.value
            }
        }

        impl<$($generics),* : Resource $(+ $traits)*> AsRef<T> for $name<$($generics),*> {
            #[inline(always)]
            fn as_ref(&self) -> &T {
                self.value
            }
        }
    };
}

impl_resource_deref!(Res<'w, T> Resource Sync);
impl_resource_deref!(ResMut<'w, T> Resource Send);
impl_resource_deref!(NonSend<'w, T> Resource);
impl_resource_deref!(NonSendMut<'w, T> Resource);

// -----------------------------------------------------------------------------
// impl_deref_mut resource
// -----------------------------------------------------------------------------

macro_rules! impl_resource_deref_mut {
    ($name:ident < $($generics:tt),+ > $($traits:ident)*) => {
        impl<$($generics),* : Resource $(+ $traits)*> DerefMut for $name<$($generics),*> {
            #[inline(always)]
            fn deref_mut(&mut self) -> &mut Self::Target {
                *self.ticks.changed = self.ticks.this_run;
                self.value
            }
        }

        impl<$($generics),* : Resource $(+ $traits)*> AsMut<T> for $name<$($generics),*> {
            #[inline(always)]
            fn as_mut(&mut self) -> &mut T {
                *self.ticks.changed = self.ticks.this_run;
                self.value
            }
        }
    };
}

impl_resource_deref_mut!(ResMut<'w, T> Resource Send);
impl_resource_deref_mut!(NonSendMut<'w, T> Resource);

// -----------------------------------------------------------------------------
// impl_ref_methods resource
// -----------------------------------------------------------------------------

macro_rules! impl_resource_ref_methods {
    ($name:ident < $($generics:tt),+ > $($traits:ident)*) => {
        impl<$($generics),* : Resource $(+ $traits)*> $name<$($generics),*> {
            /// Consumes self and returns the inner reference with the same
            /// lifetime.
            #[inline(always)]
            pub fn into_inner(self) -> &'w T {
                self.value
            }

            /// Creates a copy with the same lifetime.
            #[inline]
            pub fn reborrow(&self) -> Self {
                Self {
                    value: self.value,
                    ticks: self.ticks,
                }
            }

            /// Transforms the reference type via a function, preserving the
            /// lifetime.
            ///
            /// Returns the generic [`Ref`] container.
            #[inline(always)]
            pub fn map_type<U: ?Sized>(
                self,
                f: impl FnOnce(&T) -> &U,
            ) -> Ref<'w, U> {
                Ref {
                    value: f(self.value),
                    ticks: self.ticks,
                }
            }

            /// Transforms the reference type via a function, preserving the
            /// lifetime.
            ///
            /// Returns the generic [`Ref`] container, or an error if the
            /// transformation fails.
            #[inline]
            pub fn try_map_type<U: ?Sized, E>(
                self,
                f: impl FnOnce(&T) -> Result<&U, E>,
            ) -> Result<Ref<'w, U>, E> {
                let value = f(self.value);
                value.map(|value| Ref { value, ticks: self.ticks })
            }

            /// Dereferences the inner type, e.g., converts
            /// `Res<'a, Box<T>>` to `Ref<'a, T>`.
            ///
            /// Returns the generic [`Ref`] container.
            #[inline]
            pub fn into_deref(self) -> Ref<'w, <T as Deref>::Target>
            where
                T: Deref,
            {
                Ref {
                    value: Deref::deref(self.value),
                    ticks: self.ticks,
                }
            }
        }
    };
}

impl_resource_ref_methods!(Res<'w, T> Resource Sync);
impl_resource_ref_methods!(NonSend<'w, T> Resource);

// -----------------------------------------------------------------------------
// impl_mut_methods resource
// -----------------------------------------------------------------------------

macro_rules! impl_resource_mut_methods {
    ($name:ident < $($generics:tt),+ > $($traits:ident)*) => {
        impl<$($generics),* : Resource $(+ $traits)*> $name<$($generics),*> {
            /// Consumes self and returns the inner mutable reference with the
            /// same lifetime, marking the target as changed.
            #[inline]
            pub fn into_inner(self) -> &'w mut T {
                *self.ticks.changed = self.ticks.this_run;
                self.value
            }

            /// Returns a shorter-lived version of self, with borrow checker
            /// guarantees.
            ///
            /// This function does not mark the target as changed.
            #[inline]
            pub fn reborrow(&mut self) -> $name<'_, T> {
                $name {
                    value: self.value,
                    ticks: TicksMut {
                        added: self.ticks.added,
                        changed: self.ticks.changed,
                        last_run: self.ticks.last_run,
                        this_run: self.ticks.this_run,
                    },
                }
            }

            /// Transforms the reference type via a function, preserving the
            /// lifetime.
            ///
            /// Returns the generic [`Mut`] container.
            ///
            /// This function is assumed to only change the type, not modify
            /// data. Modifying data through the mutable reference in the
            /// closure is undefined behavior.
            #[inline]
            pub fn map_type<U: ?Sized>(
                self,
                f: impl FnOnce(&mut T) -> &mut U,
            ) -> Mut<'w, U> {
                Mut {
                    value: f(self.value),
                    ticks: self.ticks,
                }
            }

            /// Transforms the reference type via a function, preserving the
            /// lifetime.
            ///
            /// Returns the generic [`Mut`] container, or an error if the
            /// transformation fails.
            #[inline]
            pub fn try_map_type<U: ?Sized, E>(
                self,
                f: impl FnOnce(&mut T) -> Result<&mut U, E>,
            ) -> Result<Mut<'w, U>, E> {
                let value = f(self.value);
                value.map(|value| Mut { value, ticks: self.ticks })
            }

            /// Dereferences the inner type, e.g., converts
            /// `ResMut<'a, Box<T>>` to `Mut<'a, T>`.
            ///
            /// This function does not set the change flag.
            #[inline]
            pub fn into_deref(
                self,
            ) -> Mut<'w, <T as Deref>::Target>
            where
                T: DerefMut,
            {
                Mut {
                    value: DerefMut::deref_mut(self.value),
                    ticks: self.ticks,
                }
            }
        }
    };
}

impl_resource_mut_methods!(ResMut<'w, T> Resource Send);
impl_resource_mut_methods!(NonSendMut<'w, T> Resource);

// -----------------------------------------------------------------------------
// impl_change_detection resource
// -----------------------------------------------------------------------------

macro_rules! impl_resource_change_detection {
    ($name:ident < $($generics:tt),+ > $($traits:ident)*) => {
        impl<$($generics),* : Resource $(+ $traits)*> DetectChanges for $name<$($generics),*> {
            #[inline]
            fn is_added(&self) -> bool {
                self.ticks
                    .added
                    .is_newer_than(self.ticks.last_run, self.ticks.this_run)
            }

            #[inline]
            fn is_changed(&self) -> bool {
                self.ticks
                    .changed
                    .is_newer_than(self.ticks.last_run, self.ticks.this_run)
            }

            #[inline(always)]
            fn changed_tick(&self) -> Tick {
                *self.ticks.changed
            }

            #[inline(always)]
            fn added_tick(&self) -> Tick {
                *self.ticks.added
            }
        }
    };
}

impl_resource_change_detection!(Res<'w, T> Resource Sync);
impl_resource_change_detection!(ResMut<'w, T> Resource Send);
impl_resource_change_detection!(NonSend<'w, T> Resource);
impl_resource_change_detection!(NonSendMut<'w, T> Resource);

impl<'w, T: Resource + Send> DetectChangesMut for ResMut<'w, T> {
    type Value<'a>
        = &'a mut T
    where
        Self: 'a;

    #[inline(always)]
    fn bypass(&mut self) -> &'_ mut T {
        self.value
    }

    #[inline(always)]
    fn set_added(&mut self) {
        *self.ticks.added = self.ticks.this_run;
    }

    #[inline(always)]
    fn set_changed(&mut self) {
        *self.ticks.changed = self.ticks.this_run;
    }
}

impl<'w, T: Resource> DetectChangesMut for NonSendMut<'w, T> {
    type Value<'a>
        = &'a mut T
    where
        Self: 'a;

    #[inline(always)]
    fn bypass(&mut self) -> &'_ mut T {
        self.value
    }

    #[inline(always)]
    fn set_added(&mut self) {
        *self.ticks.added = self.ticks.this_run;
    }

    #[inline(always)]
    fn set_changed(&mut self) {
        *self.ticks.changed = self.ticks.this_run;
    }
}

// -----------------------------------------------------------------------------
// From conversions
// -----------------------------------------------------------------------------

impl<'w, T: Resource + Send + Sync> From<ResMut<'w, T>> for Res<'w, T> {
    #[inline]
    fn from(other: ResMut<'w, T>) -> Self {
        Res {
            value: other.value,
            ticks: other.ticks.into(),
        }
    }
}

impl<'w, T: Resource> From<NonSendMut<'w, T>> for NonSend<'w, T> {
    #[inline]
    fn from(other: NonSendMut<'w, T>) -> Self {
        NonSend {
            value: other.value,
            ticks: other.ticks.into(),
        }
    }
}

// -----------------------------------------------------------------------------
// Untyped conversions for resources
// -----------------------------------------------------------------------------

impl<'w> UntypedRef<'w> {
    /// Converts to a typed [`Res`] for a `Send + Sync` resource.
    ///
    /// # Safety
    /// `T` must be the erased pointee type for this [`UntypedRef`].
    #[inline(always)]
    pub unsafe fn into_resource<T: Resource + Sync>(self) -> Res<'w, T> {
        self.value.debug_assert_aligned::<T>();
        Res {
            value: unsafe { self.value.deref::<T>() },
            ticks: self.ticks,
        }
    }

    /// Converts to a typed [`NonSend`] for a `!Sync` resource.
    ///
    /// # Safety
    /// `T` must be the erased pointee type for this [`UntypedRef`].
    #[inline(always)]
    pub unsafe fn into_non_send<T: Resource>(self) -> NonSend<'w, T> {
        self.value.debug_assert_aligned::<T>();
        NonSend {
            value: unsafe { self.value.deref::<T>() },
            ticks: self.ticks,
        }
    }
}

impl<'w> UntypedMut<'w> {
    /// Converts to a typed [`ResMut`] for a `Send` resource.
    ///
    /// # Safety
    /// `T` must be the erased pointee type for this [`UntypedMut`].
    #[inline(always)]
    pub unsafe fn into_resource<T: Resource + Send>(self) -> ResMut<'w, T> {
        self.value.debug_assert_aligned::<T>();
        ResMut {
            value: unsafe { self.value.deref::<T>() },
            ticks: self.ticks,
        }
    }

    /// Converts to a typed [`NonSendMut`] for a `!Send` resource.
    ///
    /// # Safety
    /// `T` must be the erased pointee type for this [`UntypedMut`].
    #[inline(always)]
    pub unsafe fn into_non_send<T: Resource>(self) -> NonSendMut<'w, T> {
        self.value.debug_assert_aligned::<T>();
        NonSendMut {
            value: unsafe { self.value.deref::<T>() },
            ticks: self.ticks,
        }
    }
}
