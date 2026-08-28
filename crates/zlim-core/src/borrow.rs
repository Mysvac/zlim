//! Borrow containers: typed and untyped references with change detection.
//!
//! The ECS hands out access to component and resource data through the
//! reference wrappers defined in this module. Every wrapper pairs the actual
//! data pointer (or Rust reference) with the change-tick metadata required by
//! [`DetectChanges`], so systems can observe whether a value was added or
//! modified since their last run.
//!
//! ```ignore
//! fn system(query: Query<Ref<Name>>, logger: Res<Logger>) {
//!     for name in query.iter() {
//!         let _: bool = name.is_changed();
//!     }
//!     let _: bool = logger.is_changed();
//!     let _: bool = logger.is_added();
//!     // ......
//! }
//! ```
//!
//! # Typed wrappers
//!
//! - [`Ref`] / [`Mut`] — shared / exclusive references to a single component
//!   value. Queries produce these for `&T` / `&mut T` component access.
//! - [`SliceRef`] / [`SliceMut`] — shared / exclusive references to a
//!   contiguous run of components of the same type, with per-element change
//!   ticks.
//! - [`Res`] / [`ResMut`] / [`NonSend`] / [`NonSendMut`] — resource
//!   references. The four variants encode thread-safety: [`Res`] reads `Sync`
//!   resources, [`ResMut`] writes `Send` resources, while `!Sync` / `!Send`
//!   resources stay on the main thread and use [`NonSend`] / [`NonSendMut`].
//!
//! # Untyped wrappers
//!
//! [`UntypedRef`], [`UntypedMut`], [`UntypedSliceRef`], and
//! [`UntypedSliceMut`] are the type-erased counterparts produced by the
//! storage layer — table columns ([`crate::table`]) and resource storage
//! ([`crate::resource::Resources`]). The concrete type is recovered with
//! [`UntypedRef::with_type`], [`UntypedRef::into_resource`], or
//! [`UntypedRef::into_non_send`].
//!
//! # Change detection
//!
//! All wrappers implement [`DetectChanges`]; the mutable wrappers additionally
//! implement [`DetectChangesMut`]. See [`crate::tick`] for how ticks and
//! wrap-around are handled.
//!
//! [`DetectChanges`]: crate::tick::DetectChanges
//! [`DetectChangesMut`]: crate::tick::DetectChangesMut
//! [`crate::table`]: crate::table
//! [`crate::resource::Resources`]: crate::resource::Resources
//! [`crate::tick`]: crate::tick

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

/// A type-erased shared reference to a component or resource.
///
/// Provides read-only access to a single value without knowing its concrete
/// type at compile time. The type is recovered later with
/// [`with_type`](UntypedRef::with_type) (for components) or
/// [`into_resource`](UntypedRef::into_resource) /
/// [`into_non_send`](UntypedRef::into_non_send) (for resources).
///
/// The wrapper stores a raw [`Ptr`] together with the change-detection ticks
/// ([`TicksRef`]); fields are intentionally public to support custom
/// system-parameter implementations.
///
/// Values are produced by the internal storage layer — table columns
/// ([`crate::table`]) and resource storage ([`crate::resource::Resources`]) — and can also be
/// obtained by downgrading a typed [`Ref`] via `From`.
///
/// # Change detection
///
/// `UntypedRef` implements [`DetectChanges`], so `is_added()` / `is_changed()`
/// can be queried before the pointee type is recovered.
///
/// # Examples
///
/// ```ignore
/// fn process(untyped: UntypedRef<'_>) {
///     if untyped.is_changed() {
///         // Recover the concrete type and continue as a typed `Ref`.
///         let typed: Ref<'_, f32> = unsafe { untyped.with_type::<f32>() };
///         let value: &f32 = typed.into_inner();
///         println!("value: {value}");
///     }
/// }
/// ```
///
/// [`Ptr`]: zlim_ptr::Ptr
/// [`TicksRef`]: crate::tick::TicksRef
/// [`DetectChanges`]: crate::tick::DetectChanges
/// [`crate::table`]: crate::table
/// [`crate::resource::Resources`]: crate::resource::Resources
pub struct UntypedRef<'w> {
    pub value: Ptr<'w>,
    pub ticks: TicksRef<'w>,
}

// --------------------------------------------------------------------
// UntypedMut

/// A type-erased exclusive reference to a component or resource.
///
/// Provides mutable access to a single value without knowing its concrete
/// type at compile time. The type is recovered later with
/// [`with_type`](UntypedMut::with_type) (for components) or
/// [`into_resource`](UntypedMut::into_resource) /
/// [`into_non_send`](UntypedMut::into_non_send) (for resources).
///
/// The wrapper stores a raw [`PtrMut`] together with the change-detection
/// ticks ([`TicksMut`]); fields are intentionally public to support custom
/// system-parameter implementations.
///
/// Values are produced by the internal storage layer — table columns
/// ([`crate::table`]) and resource storage ([`crate::resource::Resources`]) — and can also be
/// obtained by downgrading a typed [`Mut`] via `From`.
///
/// # Change detection
///
/// `UntypedMut` implements [`DetectChanges`] and [`DetectChangesMut`], so
/// callers can both query and manually control the change markers.
///
/// # Examples
///
/// ```ignore
/// fn process(untyped: UntypedMut<'_>) {
///     // Recover the concrete type; `with_type` does not set the changed flag.
///     let typed: Mut<'_, f32> = unsafe { untyped.with_type::<f32>() };
///     let value: &mut f32 = typed.into_inner(); // marks the value as changed
///     *value += 1.0;
/// }
/// ```
///
/// [`PtrMut`]: zlim_ptr::PtrMut
/// [`TicksMut`]: crate::tick::TicksMut
/// [`DetectChanges`]: crate::tick::DetectChanges
/// [`DetectChangesMut`]: crate::tick::DetectChangesMut
/// [`crate::table`]: crate::table
/// [`crate::resource::Resources`]: crate::resource::Resources
pub struct UntypedMut<'w> {
    pub value: PtrMut<'w>,
    pub ticks: TicksMut<'w>,
}

// --------------------------------------------------------------------
// UntypedSliceRef

/// A type-erased shared reference to a slice of components.
///
/// Provides read-only access to multiple components of the same type without
/// knowing that type at compile time. Each element keeps its own change tick,
/// so per-element change tracking survives type erasure.
///
/// The concrete element type is recovered with
/// [`with_type`](UntypedSliceRef::with_type), which yields a [`SliceRef`].
///
/// Values are produced by the table column accessors and consumed by
/// type-erased query paths.
///
/// [`TicksSliceRef`]: crate::tick::TicksSliceRef
pub struct UntypedSliceRef<'w> {
    pub value: Ptr<'w>,
    pub ticks: TicksSliceRef<'w>,
}

// --------------------------------------------------------------------
// UntypedSliceMut

/// A type-erased exclusive reference to a slice of components.
///
/// Provides mutable access to multiple components of the same type without
/// knowing that type at compile time. Each element keeps its own change tick,
/// so per-element change tracking survives type erasure.
///
/// The concrete element type is recovered with
/// [`with_type`](UntypedSliceMut::with_type), which yields a [`SliceMut`].
///
/// Values are produced by the table column accessors and consumed by
/// type-erased query paths.
///
/// [`TicksSliceMut`]: crate::tick::TicksSliceMut
pub struct UntypedSliceMut<'w> {
    pub value: PtrMut<'w>,
    pub ticks: TicksSliceMut<'w>,
}

// --------------------------------------------------------------------
// UntypedRef : Method Implementation

impl<'w> UntypedRef<'w> {
    /// Consumes `self` and returns the underlying [`Ptr`].
    ///
    /// The tick metadata is discarded; only the raw pointer to the
    /// component data is returned.
    ///
    /// [`Ptr`]: zlim_ptr::Ptr
    #[inline(always)]
    pub fn into_inner(self) -> Ptr<'w> {
        self.value
    }

    /// Creates a copy of this untyped reference with the same lifetime and
    /// tick metadata.
    ///
    /// Shared references can be copied freely; the original and the copy
    /// are independent and do not interfere with each other.
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
    /// Consumes `self` and returns the underlying [`PtrMut`].
    ///
    /// The tick metadata is discarded; only the raw pointer to the
    /// component data is returned.
    ///
    /// This function does not set the changed flag.
    ///
    /// [`PtrMut`]: zlim_ptr::PtrMut
    #[inline(always)]
    pub fn into_inner(self) -> PtrMut<'w> {
        self.value
    }

    /// Returns a shorter-lived reborrow of this untyped mutable reference.
    ///
    /// The returned [`UntypedMut`] has a narrower lifetime, satisfying the
    /// borrow checker when the original reference must remain usable after
    /// the reborrow ends.
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
    /// Consumes `self` and returns the underlying [`Ptr`].
    ///
    /// The tick metadata and slice length are discarded; only the raw
    /// pointer to the component data is returned.
    ///
    /// [`Ptr`]: zlim_ptr::Ptr
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
    /// Consumes `self` and returns the underlying [`PtrMut`].
    ///
    /// The tick metadata and slice length are discarded; only the raw
    /// pointer to the component data is returned.
    ///
    /// Unlike the typed [`SliceMut::into_inner`], this function does not mark
    /// any element as changed.
    ///
    /// [`PtrMut`]: zlim_ptr::PtrMut
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

/// A shared reference to a component value with change detection.
///
/// `Ref` wraps an immutable borrow (`&'w T`) together with change-detection
/// tick metadata so that systems can query whether the referenced data was
/// added or modified since the last time they ran.
///
/// `Ref` implements [`Deref`] and [`AsRef`] for ergonomic access.
///
/// Use [`into_inner`] to extract the underlying `&T`, or [`map_type`] /
/// [`try_map_type`] to transform the held type while preserving ticks.
/// The type also implements [`DetectChanges`] for direct change queries
/// (`is_added()`, `is_changed()`, and the tick accessors).
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
/// struct Health(u32);
///
/// fn report(health: Ref<Health>) -> u32 {
///     // `Ref` implements `DetectChanges`, so the change markers can be
///     // queried before the inner value is extracted.
///     assert!(health.is_added());
///     assert!(health.is_changed());
///     health.into_inner().0
/// }
///
/// let mut world = World::alloc();
/// let entity = world.spawn(Health(100), None);
/// let health: Ref<'_, Health> = entity.get_ref::<Health>().unwrap();
/// // A freshly spawned component is both "added" and "changed".
/// assert_eq!(report(health), 100);
/// ```
///
/// [`into_inner`]: Ref::into_inner
/// [`map_type`]: Ref::map_type
/// [`try_map_type`]: Ref::try_map_type
/// [`DetectChanges`]: crate::tick::DetectChanges
pub struct Ref<'w, T: ?Sized> {
    pub(crate) value: &'w T,
    pub(crate) ticks: TicksRef<'w>,
}

// --------------------------------------------------------------------
// Mut

/// An exclusive reference to a component value with change detection.
///
/// `Mut` wraps a mutable borrow (`&'w mut T`) together with change-detection
/// tick metadata.
///
/// `Mut` implements [`Deref`] / [`DerefMut`] and [`AsRef`] / [`AsMut`] for
/// ergonomic access; every mutable deref or `as_mut` (like [`into_inner`])
/// marks the value as changed.
///
/// Calling [`into_inner`] consumes `self` and returns the `&mut T`, automatically
/// marking the data as changed so that downstream consumers can observe the
/// modification.
///
/// Use [`reborrow`] to obtain a shorter-lived view without consuming `self`,
/// or [`map_type`] / [`try_map_type`] to project into a sub-field while
/// preserving ticks.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
/// struct Health(u32);
///
/// let mut world = World::alloc();
/// let mut entity = world.spawn(Health(100), None);
///
/// // `get_mut` yields a change-aware `Mut`; `into_inner` returns the
/// // `&mut T` and marks the value as changed.
/// let health: Mut<'_, Health> = entity.get_mut::<Health>().unwrap();
/// let raw: &mut Health = health.into_inner();
/// raw.0 = raw.0.saturating_sub(10);
/// assert_eq!(*raw, Health(90));
/// ```
///
/// [`into_inner`]: Mut::into_inner
/// [`reborrow`]: Mut::reborrow
/// [`map_type`]: Mut::map_type
/// [`try_map_type`]: Mut::try_map_type
pub struct Mut<'w, T: ?Sized> {
    pub(crate) value: &'w mut T,
    pub(crate) ticks: TicksMut<'w>,
}

// --------------------------------------------------------------------
// SliceRef

/// A shared reference to a slice of components.
///
/// Provides read-only access to multiple components of the same type in a
/// contiguous memory region. Each element carries its own per-element
/// change-detection tick, so callers can distinguish which individual
/// components were added or modified since the last system run.
///
/// `SliceRef` implements [`IntoIterator`], yielding [`Ref<'w, T>`] items
/// through a [`SliceRefIter`]. It also dereferences to `&[T]` via [`Deref`].
///
/// This is a low-level wrapper used by storage/query internals — most
/// application code will interact with slices through higher-level query
/// abstractions.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
/// struct Velocity(f32);
///
/// let mut world = World::alloc();
/// world.spawn(Velocity(1.0), None);
/// world.spawn(Velocity(2.0), None);
///
/// // `Query<Ref<T>>` yields change-aware items; `iter_slice` batches a
/// // whole table column into one `SliceRef`.
/// let query = world.query::<Ref<Velocity>, ()>();
/// let slice: SliceRef<'_, Velocity> = query.iter_slice().next().unwrap();
///
/// // Read-only access to the whole run via `Deref`:
/// assert_eq!(slice.len(), 2);
/// assert_eq!(&slice[0], &Velocity(1.0));
/// assert_eq!(&slice[1], &Velocity(2.0));
///
/// // Per-element change detection via iteration:
/// let changed: usize = slice.into_iter().filter(|item| item.is_changed()).count();
/// assert_eq!(changed, 2);
/// ```
pub struct SliceRef<'w, T> {
    pub(crate) value: ThinSlice<'w, T>,
    pub(crate) ticks: TicksSliceRef<'w>,
}

// --------------------------------------------------------------------
// SliceMut

/// An exclusive reference to a slice of components.
///
/// Provides mutable access to multiple components of the same type in a
/// contiguous memory region. Each element carries its own per-element
/// change-detection tick so callers can distinguish which individual
/// components were added or modified since the last system run.
///
/// Mutating through [`DerefMut`], [`AsMut`], or [`into_inner`] marks
/// every element in the slice as changed. For per-element change tracking,
/// iterate with [`into_iter`] to obtain individual [`Mut<'w, T>`] items
/// via a [`SliceMutIter`].
///
/// This is a low-level wrapper used by storage/query internals — most
/// application code will interact with slices through higher-level query
/// abstractions.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
/// struct Velocity(f32);
///
/// let mut world = World::alloc();
/// world.spawn(Velocity(10.0), None);
/// world.spawn(Velocity(20.0), None);
///
/// {
///     let mut query = world.query_mut::<&mut Velocity, ()>();
///     for mut slice in query.iter_slice_mut() {
///         // `DerefMut` exposes `&mut [T]` and marks every element as changed.
///         for velocity in slice.iter_mut() {
///             velocity.0 -= 9.81;
///         }
///     }
/// }
///
/// // Verify the bulk update through a read-only query.
/// let total: f32 = world.query::<&Velocity, ()>().iter().map(|v| v.0).sum();
/// assert_eq!(total, (10.0 - 9.81) + (20.0 - 9.81));
/// ```
///
/// [`into_inner`]: SliceMut::into_inner
/// [`into_iter`]: SliceMut::into_iter
pub struct SliceMut<'w, T> {
    pub(crate) value: ThinSliceMut<'w, T>,
    pub(crate) ticks: TicksSliceMut<'w>,
}

// --------------------------------------------------------------------
// From Untyped

impl<'w> UntypedRef<'w> {
    /// Specifies the reference type and converts `self` to a [`Ref`].
    ///
    /// # Safety
    /// `T` must be the erased pointee type for this [`UntypedRef`], and the
    /// underlying pointer must be aligned for `T`.
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
    /// `T` must be the erased pointee type for this [`UntypedMut`], and the
    /// underlying pointer must be aligned for `T`.
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
    /// `T` must be the erased pointee type for this [`UntypedSliceRef`], and
    /// the underlying pointer must be aligned for `T`.
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
    /// `T` must be the erased pointee type for this [`UntypedSliceMut`], and
    /// the underlying pointer must be aligned for `T`.
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
// Deref

impl<'w, T: ?Sized> Deref for Ref<'w, T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<'w, T: ?Sized> Deref for Mut<'w, T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<'w, T: ?Sized> DerefMut for Mut<'w, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.set_changed();
        self.value
    }
}

impl<'w, T> AsRef<T> for Ref<'w, T> {
    #[inline(always)]
    fn as_ref(&self) -> &T {
        self.value
    }
}

impl<'w, T> AsRef<T> for Mut<'w, T> {
    #[inline(always)]
    fn as_ref(&self) -> &T {
        self.value
    }
}

impl<'w, T> AsMut<T> for Mut<'w, T> {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut T {
        self.set_changed();
        self.value
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
    /// Consumes `self` and returns the inner reference `&T` with the same
    /// lifetime.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
    /// struct Health(u32);
    ///
    /// let mut world = World::alloc();
    /// let entity = world.spawn(Health(100), None);
    /// let r: Ref<'_, Health> = entity.get_ref::<Health>().unwrap();
    /// let value: &Health = r.into_inner();
    /// assert_eq!(value, &Health(100));
    /// ```
    #[inline(always)]
    pub fn into_inner(self) -> &'w T {
        self.value
    }

    /// Creates a copy of this reference with the same lifetime and tick
    /// metadata.
    ///
    /// Unlike [`Mut::reborrow`], shared references can be copied freely
    /// without shortening the lifetime — the borrow checker treats both
    /// the original and the copy as independent shared borrows.
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
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
    /// struct Outer {
    ///     inner: f32,
    /// }
    ///
    /// let mut world = World::alloc();
    /// let entity = world.spawn(Outer { inner: 1.5 }, None);
    /// let r: Ref<'_, Outer> = entity.get_ref::<Outer>().unwrap();
    /// let inner: Ref<'_, f32> = r.map_type(|outer| &outer.inner);
    /// assert_eq!(*inner.into_inner(), 1.5);
    /// ```
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
    /// Consumes `self` and returns the inner reference `&mut T` with the
    /// same lifetime, marking the target as changed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
    /// struct Score(f32);
    ///
    /// let mut world = World::alloc();
    /// let mut entity = world.spawn(Score(3.0), None);
    /// let m: Mut<'_, Score> = entity.get_mut::<Score>().unwrap();
    /// let value: &mut Score = m.into_inner(); // marks the value as changed
    /// value.0 *= 2.0;
    /// assert_eq!(*value, Score(6.0));
    /// ```
    #[inline]
    pub fn into_inner(self) -> &'w mut T {
        *self.ticks.changed = self.ticks.this_run;
        self.value
    }

    /// Returns a shorter-lived version of `self`, with borrow checker
    /// guarantees.
    ///
    /// This function does not mark the target as changed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
    /// struct Counter(u32);
    ///
    /// let mut world = World::alloc();
    /// let mut entity = world.spawn(Counter(0), None);
    ///
    /// let mut m: Mut<'_, Counter> = entity.get_mut::<Counter>().unwrap();
    /// // `reborrow` yields a shorter-lived view without consuming `m`.
    /// let view: Mut<'_, Counter> = m.reborrow();
    /// *view.into_inner() = Counter(1); // marks the value as changed
    /// // The original borrow is still usable after the reborrow ends.
    /// let again: &mut Counter = m.into_inner();
    /// assert_eq!(*again, Counter(1));
    /// ```
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
    /// Modifying data through the mutable reference in the closure is a logic
    /// error (data may be modified without triggering change events).
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
    /// Modifying data through the mutable reference in the closure is a logic
    /// error (data may be modified without triggering change events).
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
    /// Consumes `self` and returns the underlying slice `&[T]` with the
    /// same lifetime.
    #[inline(always)]
    pub fn into_inner(self) -> &'w [T] {
        unsafe { self.value.deref(self.ticks.length) }
    }

    /// Creates a copy of this slice reference with the same lifetime and
    /// tick metadata.
    ///
    /// Since this is a shared reference, the original and copy do not
    /// interfere — both can be used independently without lifetime
    /// shortening.
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

/// An iterator over shared references to components in a [`SliceRef`].
///
/// Each item yielded by this iterator is a [`Ref<'w, T>`] providing read-only
/// access to a single element together with its own per-element
/// change-detection tick metadata. Because each element carries independent
/// tick information, consumers can detect which specific elements were added
/// or modified since the last system run.
///
/// This iterator implements [`ExactSizeIterator`] and [`FusedIterator`], so
/// its length is known in advance and it will continue to yield `None` after
/// exhaustion.
///
/// Obtain this iterator via [`SliceRef::into_iter`] or by using a
/// [`SliceRef`] directly in a `for` loop.
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

    /// Consumes `self` and returns the underlying mutable slice `&mut [T]`
    /// with the same lifetime.
    ///
    /// All elements in the slice are marked as changed before the inner
    /// reference is returned, so any downstream change-detection queries
    /// will observe the modification.
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

/// An iterator over mutable references to components in a [`SliceMut`].
///
/// Each item yielded by this iterator is a [`Mut<'w, T>`] providing exclusive
/// access to a single element together with its own per-element
/// change-detection tick metadata. Because each element carries independent
/// tick information, consumers can detect which specific elements were added
/// or modified since the last system run.
///
/// Calling [`Mut::into_inner`] on a yielded [`Mut`] automatically marks that
/// element as changed, so downstream change-detection queries will observe the
/// modification.
///
/// This iterator implements [`ExactSizeIterator`] and [`FusedIterator`], so
/// its length is known in advance and it will continue to yield `None` after
/// exhaustion.
///
/// Obtain this iterator via [`SliceMut::into_iter`] or by using a
/// [`SliceMut`] directly in a `for` loop.
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
// Res
// -----------------------------------------------------------------------------

/// A shared reference to a `Sync` resource with change detection.
///
/// This is the read-only resource parameter for systems. `Res` always carries
/// change-detection ticks — there is no thin variant without tracking.
///
/// Use [`into_inner`](Self::into_inner) to extract the underlying `&T`, or
/// [`map_type`](Self::map_type) / [`try_map_type`](Self::try_map_type) to
/// project into a sub-field while preserving the ticks.
///
/// # Examples
///
/// ```rust
/// use zlim_core::borrow::Res;
/// use zlim_core::prelude::*;
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Resource)]
/// struct Logger;
///
/// let mut world = World::alloc();
/// world.insert_resource(Logger);
///
/// let logger: Res<'_, Logger> = world.get_resource_ref::<Logger>().unwrap();
/// // A freshly inserted resource is marked as changed.
/// assert!(logger.is_changed());
/// let _: &Logger = logger.into_inner();
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
/// Writing through [`DerefMut`] or [`AsMut`] — or consuming the wrapper with
/// [`into_inner`](Self::into_inner) — automatically marks the resource as
/// changed.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Resource)]
/// struct Logger;
///
/// let mut world = World::alloc();
/// world.insert_resource(Logger);
///
/// let mut logger: ResMut<'_, Logger> = world.get_resource_mut::<Logger>().unwrap();
/// // Writing through `DerefMut` marks the resource as changed.
/// let _inner: &mut Logger = &mut logger;
/// assert!(logger.is_changed());
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
/// `NonSend` is the main-thread-only counterpart to [`Res`]. It is intended
/// for resource types that do not implement `Sync` (and therefore cannot be
/// shared across threads). Like [`Res`], it provides immutable access with
/// full change-detection support via [`DetectChanges`].
///
/// # Thread safety
///
/// `NonSend` parameters can only be fetched from systems that run on the
/// world's main thread. The ECS scheduler enforces this automatically.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// // `Cell` is `Send` but not `Sync`, so this type cannot be shared across
/// // threads — exactly the case `NonSend` is designed for.
/// #[derive(TypePath, Resource)]
/// struct ThreadLocalState {
///     counter: core::cell::Cell<u32>,
/// }
///
/// let mut world = World::alloc();
/// world.with_non_send_mut(|w| {
///     w.insert_non_send(ThreadLocalState { counter: core::cell::Cell::new(0) });
/// });
///
/// world.with_non_send(|w| {
///     let state: NonSend<'_, ThreadLocalState> =
///         w.get_non_send_ref::<ThreadLocalState>().unwrap();
///     // A freshly inserted resource is marked as changed.
///     assert!(state.is_changed());
/// });
/// ```
///
/// [`DetectChanges`]: crate::tick::DetectChanges
pub struct NonSend<'w, T: Resource> {
    pub(crate) value: &'w T,
    pub(crate) ticks: TicksRef<'w>,
}

// -----------------------------------------------------------------------------
// NonSendMut
// -----------------------------------------------------------------------------

/// An exclusive reference to a `!Send` resource with change detection.
///
/// `NonSendMut` is the main-thread-only counterpart to [`ResMut`]. It is
/// intended for resource types that do not implement `Send` (and therefore
/// cannot be transferred to another thread). Like [`ResMut`], it provides
/// mutable access with full change-detection support — writing through
/// [`DerefMut`] or [`AsMut`] automatically marks the resource as changed.
///
/// # Thread safety
///
/// `NonSendMut` parameters can only be fetched from systems that run on the
/// world's main thread. The ECS scheduler enforces this automatically and
/// will prevent any concurrent access to the same resource while the system
/// runs.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Resource)]
/// struct RngState(u64);
///
/// let mut world = World::alloc();
/// world.with_non_send_mut(|w| {
///     w.insert_non_send(RngState(1));
/// });
///
/// world.with_non_send_mut(|w| {
///     let mut rng: NonSendMut<'_, RngState> = w.get_non_send_mut::<RngState>().unwrap();
///     // Writing through `DerefMut` marks the resource as changed.
///     rng.0 = rng.0.wrapping_mul(636413622).wrapping_add(1);
///     assert!(rng.is_changed());
/// });
/// ```
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
            /// closure is a logic error.
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
    /// `T` must be the erased pointee type for this [`UntypedRef`], and the
    /// underlying pointer must be aligned for `T`.
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
    /// `T` must be the erased pointee type for this [`UntypedRef`], and the
    /// underlying pointer must be aligned for `T`.
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
    /// `T` must be the erased pointee type for this [`UntypedMut`], and the
    /// underlying pointer must be aligned for `T`.
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
    /// `T` must be the erased pointee type for this [`UntypedMut`], and the
    /// underlying pointer must be aligned for `T`.
    #[inline(always)]
    pub unsafe fn into_non_send<T: Resource>(self) -> NonSendMut<'w, T> {
        self.value.debug_assert_aligned::<T>();
        NonSendMut {
            value: unsafe { self.value.deref::<T>() },
            ticks: self.ticks,
        }
    }
}

// -----------------------------------------------------------------------------
