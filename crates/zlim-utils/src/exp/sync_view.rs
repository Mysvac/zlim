//! A reimplementation of the currently unstable [`core::sync::SyncView`]
#![expect(unsafe_code, reason = "SyncView requires unsafe code.")]

use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::pin::Pin;

/// See [`core::sync::SyncView`]
///
/// # Example
///
/// ```
/// # use core::cell::Cell;
/// # use zlim_utils::exp::SyncView;
///
/// async fn other() {}
///
/// fn assert_sync<T: Sync>(t: T) {}
///
/// struct State<F> {
///     future: SyncView<F>
/// }
///
/// assert_sync(State {
///     future: SyncView::new(async {
///         // including Cell, but SyncView is `sync`
///         let cell = Cell::new(1);
///         let cell_ref = &cell;
///         // other().await;
///         let val = cell_ref.get();
///     })
/// });
/// ```
#[repr(transparent)]
pub struct SyncView<T: ?Sized> {
    inner: T,
}

// SAFETY: `Sync` only allows multithreaded access via immutable reference.
//
// As `SyncView` requires an exclusive reference to access the wrapped value for `!Sync` types,
// marking this type as `Sync` does not actually allow unsynchronized access to the inner value.
unsafe impl<T: ?Sized> Sync for SyncView<T> {}

impl<T: Sized> SyncView<T> {
    /// Wrap a value in an `SyncView`.
    #[must_use]
    #[inline(always)]
    pub const fn new(inner: T) -> Self {
        Self { inner }
    }

    /// Unwrap the value contained in the `SyncView`.
    #[must_use]
    #[inline(always)]
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: ?Sized> SyncView<T> {
    /// Gets pinned exclusive access to the underlying value.
    ///
    /// `SyncView` is considered to _structurally pin_ the underlying
    /// value, which means _unpinned_ `SyncView`s can produce _unpinned_
    /// access to the underlying value, but _pinned_ `SyncView`s only
    /// produce _pinned_ access to the underlying value.
    #[must_use]
    #[inline(always)]
    pub const fn as_pin_mut(self: Pin<&mut Self>) -> Pin<&mut T> {
        // SAFETY: `SyncView` can only produce `&mut T` if itself is unpinned
        // `Pin::map_unchecked_mut` is not const, so we do this conversion manually
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().inner) }
    }

    /// Build a _mutable_ reference to an `SyncView<T>` from
    /// a _mutable_ reference to a `T`. This allows you to skip
    /// building an `SyncView` with [`SyncView::new`].
    #[must_use]
    #[inline(always)]
    pub const fn from_mut(r: &'_ mut T) -> &'_ mut SyncView<T> {
        // SAFETY: repr is ≥ C, so refs have the same layout; and `SyncView` properties are `&mut`-agnostic
        unsafe { &mut *(r as *mut T as *mut SyncView<T>) }
    }

    /// Build a _pinned mutable_ reference to an `SyncView<T>` from
    /// a _pinned mutable_ reference to a `T`. This allows you to skip
    /// building an `SyncView` with [`SyncView::new`].
    #[must_use]
    #[inline(always)]
    pub const fn from_pin_mut(r: Pin<&'_ mut T>) -> Pin<&'_ mut SyncView<T>> {
        // SAFETY: `SyncView` can only produce `&mut T` if itself is unpinned
        // `Pin::map_unchecked_mut` is not const, so we do this conversion manually
        unsafe { Pin::new_unchecked(Self::from_mut(r.get_unchecked_mut())) }
    }
}

impl<T: ?Sized + Sync> SyncView<T> {
    /// Gets pinned shared access to the underlying value.
    ///
    /// `SyncView` is considered to _structurally pin_ the underlying
    /// value, which means _unpinned_ `SyncView`s can produce _unpinned_
    /// access to the underlying value, but _pinned_ `SyncView`s only
    /// produce _pinned_ access to the underlying value.
    #[must_use]
    #[inline(always)]
    pub const fn as_pin_ref(self: Pin<&Self>) -> Pin<&T> {
        // SAFETY: `SyncView` can only produce `&T` if itself is unpinned
        // `Pin::map_unchecked` is not const, so we do this conversion manually
        unsafe { Pin::new_unchecked(&self.get_ref().inner) }
    }
}

impl<T: ?Sized> fmt::Debug for SyncView<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        f.debug_struct("SyncView").finish_non_exhaustive()
    }
}

impl<T: Default> Default for SyncView<T> {
    #[inline]
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> From<T> for SyncView<T> {
    #[inline]
    fn from(t: T) -> Self {
        Self::new(t)
    }
}

impl<T> AsRef<T> for SyncView<T>
where
    T: Sync + ?Sized,
{
    /// Gets shared access to the underlying value.
    #[inline]
    fn as_ref(&self) -> &T {
        &self.inner
    }
}

impl<T> AsMut<T> for SyncView<T>
where
    T: ?Sized,
{
    /// Gets exclusive access to the underlying value.
    #[inline]
    fn as_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T> Clone for SyncView<T>
where
    T: Sync + Clone,
{
    #[inline]
    fn clone(&self) -> Self {
        Self::new(self.inner.clone())
    }
}

impl<T> Copy for SyncView<T> where T: Sync + Copy {}

impl<T, U> PartialEq<SyncView<U>> for SyncView<T>
where
    T: Sync + PartialEq<U> + ?Sized,
    U: Sync + ?Sized,
{
    #[inline]
    fn eq(&self, other: &SyncView<U>) -> bool {
        self.inner == other.inner
    }
}

impl<T> Eq for SyncView<T> where T: Sync + Eq + ?Sized {}

impl<T> Hash for SyncView<T>
where
    T: Sync + Hash + ?Sized,
{
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(&self.inner, state)
    }
}

impl<T, U> PartialOrd<SyncView<U>> for SyncView<T>
where
    T: Sync + PartialOrd<U> + ?Sized,
    U: Sync + ?Sized,
{
    #[inline]
    fn partial_cmp(&self, other: &SyncView<U>) -> Option<Ordering> {
        self.inner.partial_cmp(&other.inner)
    }
}

impl<T> Ord for SyncView<T>
where
    T: Sync + Ord + ?Sized,
{
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.inner.cmp(&other.inner)
    }
}
