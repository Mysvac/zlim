//! Fixed-capacity circular array buffer with stack storage.
//!
//! `ArrayDeque` stores elements in a fixed-size ring buffer without heap allocation.
//! It is useful for short-lived queues where capacity is known at compile time.
#![expect(unsafe_code, reason = "original implementation")]

use core::fmt::Debug;
use core::iter::{Chain, FusedIterator};
use core::mem::MaybeUninit;
use core::{ptr, slice};

// ----------------------------------------------------------------------------
// ArrayDeque

/// A ring buffer with fixed capacity, storing data on the stack.
///
/// `ArrayDeque` is a double-ended queue (deque) implemented as a circular buffer
/// with compile-time fixed capacity `N`. All data is stored in an array on the stack,
/// avoiding heap allocations.
///
/// Note that the back operation is faster than front.
///
/// # Examples
///
/// ```
/// use zlim_utils::ext::ArrayDeque;
///
/// let mut deque: ArrayDeque<i32, 4> = ArrayDeque::new();
///
/// // Push elements to the back
/// deque.push_back(1).unwrap();
/// deque.push_back(2).unwrap();
///
/// // Push elements to the front
/// deque.push_front(0).unwrap();
///
/// // Access elements
/// assert_eq!(deque.front(), Some(&0));
/// assert_eq!(deque.back(), Some(&2));
///
/// // Pop elements
/// assert_eq!(deque.pop_front(), Some(0));
/// assert_eq!(deque.pop_back(), Some(2));
///
/// // Check capacity constraints
/// deque.push_back(3).unwrap();
/// deque.push_back(4).unwrap();
/// deque.push_back(5).unwrap();
/// assert!(deque.is_full());
/// assert_eq!(deque.push_back(6), Err(6)); // Full, returns the element
/// ```
pub struct ArrayDeque<T, const N: usize> {
    tail: usize,
    len: usize,
    slots: [MaybeUninit<T>; N],
}

// ----------------------------------------------------------------------------
// drop

impl<T, const N: usize> ArrayDeque<T, N> {
    #[inline]
    fn drop_data(&mut self) {
        if core::mem::needs_drop::<T>() && self.len != 0 {
            if self.len == N {
                // SAFETY: When `self.len == N`, all N slots contain initialized `T` values.
                // The cast from `*mut MaybeUninit<T>` to `*mut T` is valid because they have
                // the same layout.
                unsafe {
                    ptr::drop_in_place::<[T]>(ptr::slice_from_raw_parts_mut(
                        self.slots.as_mut_ptr() as *mut T,
                        N,
                    ));
                }
                return;
            }
            let begin = (self.tail + N - self.len) % N;
            if self.tail > begin {
                // SAFETY: In the contiguous case, `begin = self.tail - self.len`, so
                // `begin + self.len = self.tail <= N`. The range `[begin, begin + len)`
                // covers all initialized elements. The `MaybeUninit<T>` → `T` cast is valid.
                unsafe {
                    ptr::drop_in_place::<[T]>(ptr::slice_from_raw_parts_mut(
                        self.slots.as_mut_ptr().add(begin) as *mut T,
                        self.len,
                    ));
                }
            } else {
                // SAFETY: In the wrapped case, initialized elements are split into two
                // non-overlapping ranges: `[begin, N)` (`N - begin` elements) and
                // `[0, tail)` (`tail` elements). They don't overlap because `tail <= begin`.
                // The `MaybeUninit<T>` → `T` casts are valid (same layout).
                unsafe {
                    ptr::drop_in_place::<[T]>(ptr::slice_from_raw_parts_mut(
                        self.slots.as_mut_ptr() as *mut T,
                        self.tail,
                    ));
                    ptr::drop_in_place::<[T]>(ptr::slice_from_raw_parts_mut(
                        self.slots.as_mut_ptr().add(begin) as *mut T,
                        N - begin,
                    ));
                }
            }
        }
    }
}

impl<T, const N: usize> Drop for ArrayDeque<T, N> {
    fn drop(&mut self) {
        self.drop_data();
    }
}

// ----------------------------------------------------------------------------
// methods

impl<T, const N: usize> ArrayDeque<T, N> {
    /// Returns `true` if the buffer is full (len == capacity).
    ///
    /// When the deque is full, any attempt to push additional elements
    /// will fail, returning the element in an `Err`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_utils::ext::ArrayDeque;
    /// let mut deque: ArrayDeque<i32, 2> = ArrayDeque::new();
    /// assert!(!deque.is_full());
    ///
    /// deque.push_back(1).unwrap();
    /// deque.push_back(2).unwrap();
    /// assert!(deque.is_full());
    ///
    /// // Attempting to push another element will fail
    /// assert!(deque.push_back(3).is_err());
    /// ```
    #[inline(always)]
    pub const fn is_full(&self) -> bool {
        self.len == N
    }

    /// Returns `true` if the buffer is empty (len == 0).
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_utils::ext::ArrayDeque;
    /// let mut deque: ArrayDeque<i32, 4> = ArrayDeque::new();
    /// assert!(deque.is_empty());
    ///
    /// deque.push_back(1).unwrap();
    /// assert!(!deque.is_empty());
    /// ```
    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the number of elements in the deque.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_utils::ext::ArrayDeque;
    /// let mut deque: ArrayDeque<i32, 4> = ArrayDeque::new();
    /// assert_eq!(deque.len(), 0);
    ///
    /// deque.push_back(1).unwrap();
    /// deque.push_back(2).unwrap();
    /// assert_eq!(deque.len(), 2);
    /// ```
    #[inline(always)]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns the capacity of deque.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_utils::ext::ArrayDeque;
    /// let mut deque = ArrayDeque::<i32, 5>::new();
    /// assert_eq!(deque.capacity(), 5);
    /// ```
    #[inline(always)]
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Returns a pair of slices containing the deque contents in front-to-back order.
    ///
    /// The returned slices represent the logical order of elements:
    ///
    /// - The first slice contains the first contiguous region.
    /// - The second slice contains the wrapped region, if any.
    ///
    /// When the internal storage is not wrapped, all elements are in the first slice
    /// and the second slice is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_utils::ext::ArrayDeque;
    /// let mut deque: ArrayDeque<i32, 5> = ArrayDeque::new();
    /// deque.push_back(0).unwrap();
    /// deque.push_back(1).unwrap();
    /// deque.push_back(2).unwrap();
    ///
    /// let expected = [0, 1, 2];
    /// let (front, back) = deque.as_slices();
    /// assert_eq!(&expected[..front.len()], front);
    /// assert_eq!(&expected[front.len()..], back);
    /// ```
    #[inline]
    pub const fn as_slices(&self) -> (&[T], &[T]) {
        if self.len == 0 {
            return (&[], &[]);
        }

        let begin = (self.tail + N - self.len) % N;
        let ptr = self.slots.as_ptr() as *const T;

        unsafe {
            if begin < self.tail {
                (slice::from_raw_parts(ptr.add(begin), self.len), &[])
            } else {
                (
                    slice::from_raw_parts(ptr.add(begin), N - begin),
                    slice::from_raw_parts(ptr, self.tail),
                )
            }
        }
    }

    /// Returns a pair of mutable slices containing the deque contents in front-to-back order.
    ///
    /// The two mutable slices are non-overlapping and together cover all elements.
    /// If the deque is not wrapped, the second slice is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_utils::ext::ArrayDeque;
    /// let mut deque: ArrayDeque<i32, 4> = ArrayDeque::new();
    /// deque.push_back(1).unwrap();
    /// deque.push_back(2).unwrap();
    /// deque.push_back(3).unwrap();
    ///
    /// let (front, back) = deque.as_mut_slices();
    /// for x in front {
    ///     *x *= 10;
    /// }
    /// for x in back {
    ///     *x *= 10;
    /// }
    ///
    /// assert_eq!(deque.pop_front(), Some(10));
    /// assert_eq!(deque.pop_front(), Some(20));
    /// assert_eq!(deque.pop_front(), Some(30));
    /// ```
    #[inline]
    pub const fn as_mut_slices(&mut self) -> (&mut [T], &mut [T]) {
        if self.len == 0 {
            return (&mut [], &mut []);
        }

        let begin = (self.tail + N - self.len) % N;
        let ptr = self.slots.as_mut_ptr() as *mut T;

        unsafe {
            if begin < self.tail {
                (slice::from_raw_parts_mut(ptr.add(begin), self.len), &mut [])
            } else {
                (
                    slice::from_raw_parts_mut(ptr.add(begin), N - begin),
                    slice::from_raw_parts_mut(ptr, self.tail),
                )
            }
        }
    }

    /// Removes all elements from the deque.
    ///
    /// This method drops all elements currently in the deque and resets
    /// its internal state to empty. The capacity remains unchanged.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_utils::ext::ArrayDeque;
    /// let mut deque: ArrayDeque<i32, 4> = ArrayDeque::new();
    /// deque.push_back(1).unwrap();
    /// assert_eq!(deque.len(), 1);
    ///
    /// deque.clear();
    /// assert_eq!(deque.len(), 0);
    /// ```
    pub fn clear(&mut self) {
        self.drop_data();
        self.tail = 0;
        self.len = 0;
    }

    /// Creates an empty `ArrayDeque` with uninitialized backing storage.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_utils::ext::ArrayDeque;
    /// let deque = ArrayDeque::<i32, 10>::new();
    /// assert!(deque.is_empty());
    /// # assert!(!deque.is_full());
    /// # assert_eq!(deque.len(), 0);
    /// ```
    ///
    /// Note that the capacity `0` is valid.
    #[must_use]
    #[inline(always)]
    pub const fn new() -> Self {
        assert! {
            const { N != 0 && N <= (usize::MAX >> 2) },
            "the capacity of ArrayDeque cannot be `0` and exceed `usize::MAX / 4`",
        }

        Self {
            slots: unsafe { MaybeUninit::<[MaybeUninit<T>; N]>::uninit().assume_init() },
            tail: 0,
            len: 0,
        }
    }

    /// Returns a reference to the front element, if present.
    ///
    /// This method does not remove the element from the deque.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_utils::ext::ArrayDeque;
    /// let mut deque: ArrayDeque<i32, 4> = ArrayDeque::new();
    /// assert_eq!(deque.front(), None);
    ///
    /// deque.push_back(1).unwrap();
    /// deque.push_back(2).unwrap();
    /// assert_eq!(deque.front(), Some(&1));
    ///
    /// deque.push_front(0).unwrap();
    /// assert_eq!(deque.front(), Some(&0));
    /// ```
    pub const fn front(&self) -> Option<&T> {
        if !self.is_empty() {
            let front = (self.tail + N - self.len) % N;
            unsafe { Some(&*self.slots.as_ptr().add(front).cast::<T>()) }
        } else {
            None
        }
    }

    /// Returns a mutable reference to the front element, if present.
    ///
    /// This method does not remove the element from the deque.
    pub const fn front_mut(&mut self) -> Option<&mut T> {
        if !self.is_empty() {
            let front = (self.tail + N - self.len) % N;
            unsafe { Some(&mut *self.slots.as_mut_ptr().add(front).cast::<T>()) }
        } else {
            None
        }
    }

    /// Returns a reference to the back element, if present.
    ///
    /// This method does not remove the element from the deque.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_utils::ext::ArrayDeque;
    /// let mut deque: ArrayDeque<i32, 4> = ArrayDeque::new();
    /// assert_eq!(deque.back(), None);
    ///
    /// deque.push_back(1).unwrap();
    /// deque.push_back(2).unwrap();
    /// assert_eq!(deque.back(), Some(&2));
    ///
    /// deque.pop_back();
    /// assert_eq!(deque.back(), Some(&1));
    /// ```
    pub const fn back(&self) -> Option<&T> {
        if !self.is_empty() {
            let back = (self.tail + N - 1) % N;
            unsafe {
                let ptr = self.slots.as_ptr();
                Some(&*ptr.add(back).cast::<T>())
            }
        } else {
            None
        }
    }

    /// Returns a mutable reference to the back element, if present.
    ///
    /// This method does not remove the element from the deque.
    pub const fn back_mut(&mut self) -> Option<&mut T> {
        if !self.is_empty() {
            let back = (self.tail + N - 1) % N;
            unsafe {
                let ptr = self.slots.as_mut_ptr();
                Some(&mut *ptr.add(back).cast::<T>())
            }
        } else {
            None
        }
    }

    /// Returns a reference to the element at logical index `index`.
    ///
    /// Index `0` corresponds to the front element.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_utils::ext::ArrayDeque;
    /// let mut deque: ArrayDeque<i32, 4> = ArrayDeque::new();
    /// deque.push_back(1).unwrap();
    /// deque.push_back(2).unwrap();
    ///
    /// assert_eq!(deque.get(0), Some(&1));
    /// assert_eq!(deque.get(1), Some(&2));
    /// assert_eq!(deque.get(2), None);
    /// ```
    pub const fn get(&self, index: usize) -> Option<&T> {
        if index >= self.len {
            return None;
        }

        let idx = (self.tail + N - self.len + index) % N;
        let ptr = self.slots.as_ptr();

        unsafe { Some(&*ptr.add(idx).cast::<T>()) }
    }

    /// Returns a mutable reference to the element at logical index `index`.
    ///
    /// Index `0` corresponds to the front element.
    pub const fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if index >= self.len {
            return None;
        }

        let idx = (self.tail + N - self.len + index) % N;
        let ptr = self.slots.as_mut_ptr();

        unsafe { Some(&mut *ptr.add(idx).cast::<T>()) }
    }

    /// Returns `true` if the deque contains an element equal to `value`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_utils::ext::ArrayDeque;
    /// let mut deque: ArrayDeque<i32, 3> = [10, 40, 30].into();
    ///
    /// assert!(deque.contains(&30));
    /// assert!(!deque.contains(&50));
    /// ```
    pub fn contains(&self, value: &T) -> bool
    where
        T: PartialEq,
    {
        let (a, b) = self.as_slices();
        a.contains(value) || b.contains(value)
    }

    /// Pushes an element to the front of the deque.
    ///
    /// Returns `Ok(())` if the element was inserted successfully.
    /// If the deque is full, returns `Err(element)` and leaves the deque unchanged.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_utils::ext::ArrayDeque;
    /// let mut deque: ArrayDeque<i32, 2> = ArrayDeque::new();
    ///
    /// assert_eq!(deque.push_front(1), Ok(()));
    /// assert_eq!(deque.push_front(0), Ok(()));
    /// assert_eq!(deque.front(), Some(&0));
    ///
    /// // Deque is now full
    /// assert_eq!(deque.push_front(-1), Err(-1));
    /// assert_eq!(deque.len(), 2); // Length unchanged
    /// ```
    #[inline]
    pub const fn push_front(&mut self, element: T) -> Result<(), T> {
        if self.is_full() {
            return Err(element);
        }
        unsafe {
            self.push_front_unchecked(element);
        }
        Ok(())
    }

    /// Pushes an element to the front of the deque without checking capacity.
    ///
    /// # Safety
    /// The caller must ensure that `!self.is_full()`.
    /// Calling this method when the deque is full results in undefined behavior.
    #[inline]
    pub const unsafe fn push_front_unchecked(&mut self, element: T) {
        let begin = (self.tail + (N << 1) - self.len - 1) % N;
        unsafe {
            let ptr = self.slots.as_mut_ptr();
            let dst = ptr.add(begin) as *mut T;
            ptr::write(dst, element);
        }
        self.len += 1;
    }

    /// Pushes an element to the back of the deque.
    ///
    /// Returns `Ok(())` if the element was inserted successfully.
    /// If the deque is full, returns `Err(element)` and leaves the deque unchanged.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_utils::ext::ArrayDeque;
    /// let mut deque: ArrayDeque<i32, 2> = ArrayDeque::new();
    ///
    /// assert_eq!(deque.push_back(1), Ok(()));
    /// assert_eq!(deque.push_back(2), Ok(()));
    /// assert_eq!(deque.back(), Some(&2));
    ///
    /// // Deque is now full
    /// assert_eq!(deque.push_back(3), Err(3));
    /// assert_eq!(deque.len(), 2); // Length unchanged
    /// ```
    #[inline]
    pub const fn push_back(&mut self, element: T) -> Result<(), T> {
        if self.is_full() {
            return Err(element);
        }
        unsafe {
            self.push_back_unchecked(element);
        }
        Ok(())
    }

    /// Pushes an element to the back of the deque without checking capacity.
    ///
    /// # Safety
    /// The caller must ensure that `!self.is_full()`.
    /// Calling this method when the deque is full results in undefined behavior.
    #[inline]
    pub const unsafe fn push_back_unchecked(&mut self, element: T) {
        unsafe {
            let ptr = self.slots.as_mut_ptr();
            let dst = ptr.add(self.tail) as *mut T;
            ptr::write(dst, element);
        }
        self.tail = (self.tail + 1) % N;
        self.len += 1;
    }

    /// Removes and returns the front element of the deque.
    ///
    /// Returns `Some(T)` if the deque is not empty, otherwise returns `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_utils::ext::ArrayDeque;
    /// let mut deque: ArrayDeque<i32, 4> = ArrayDeque::new();
    /// deque.push_back(1).unwrap();
    /// deque.push_back(2).unwrap();
    ///
    /// assert_eq!(deque.pop_front(), Some(1));
    /// assert_eq!(deque.pop_front(), Some(2));
    /// assert_eq!(deque.pop_front(), None);
    /// ```
    #[inline]
    pub const fn pop_front(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        let begin = (self.tail + N - self.len) % N;
        self.len -= 1;
        let value = unsafe {
            let ptr = self.slots.as_mut_ptr();
            let src: *mut T = ptr.add(begin) as *mut T;
            ptr::read::<T>(src)
        };
        Some(value)
    }

    /// Removes and returns the back element of the deque.
    ///
    /// Returns `Some(T)` if the deque is not empty, otherwise returns `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_utils::ext::ArrayDeque;
    /// let mut deque: ArrayDeque<i32, 4> = ArrayDeque::new();
    /// deque.push_back(1).unwrap();
    /// deque.push_back(2).unwrap();
    ///
    /// assert_eq!(deque.pop_back(), Some(2));
    /// assert_eq!(deque.pop_back(), Some(1));
    /// assert_eq!(deque.pop_back(), None);
    /// ```
    #[inline]
    pub const fn pop_back(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        self.tail = (self.tail + N - 1) % N;
        self.len -= 1;
        let value = unsafe {
            let ptr = self.slots.as_mut_ptr();
            let src = ptr.add(self.tail) as *mut T;
            ptr::read::<T>(src)
        };
        Some(value)
    }

    /// Returns an iterator over references to elements in front-to-back order.
    ///
    /// This is equivalent to iterating over `self.as_slices().0` then
    /// `self.as_slices().1`.
    ///
    /// ```
    /// # use zlim_utils::ext::ArrayDeque;
    /// let mut deque = ArrayDeque::<i32, 4>::new();
    ///
    /// deque.push_back(1);
    /// deque.push_back(2);
    /// deque.push_back(3);
    ///
    /// let vec: Vec<_> = deque.iter().copied().collect();
    /// assert_eq!(vec, [1, 2, 3]);
    /// ```
    pub fn iter(&self) -> Iter<'_, T> {
        let (a, b) = self.as_slices();
        Iter {
            inner: a.iter().chain(b.iter()),
        }
    }

    /// Returns an iterator over mutable references in front-to-back order.
    ///
    /// This is equivalent to iterating over `self.as_mut_slices().0` then
    /// `self.as_mut_slices().1`.
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        let (a, b) = self.as_mut_slices();
        IterMut {
            inner: a.iter_mut().chain(b.iter_mut()),
        }
    }
}

// ----------------------------------------------------------------------------
// Trait

impl<T, const N: usize> Default for ArrayDeque<T, N> {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Debug, const N: usize> Debug for ArrayDeque<T, N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let (first, second) = self.as_slices();
        f.debug_list()
            .entries(first.iter().chain(second.iter()))
            .finish()
    }
}

impl<T: Clone, const N: usize> Clone for ArrayDeque<T, N> {
    fn clone(&self) -> Self {
        let mut out = Self::new();
        self.iter().for_each(|item| unsafe {
            out.push_back_unchecked(item.clone());
        });
        out
    }

    fn clone_from(&mut self, source: &Self) {
        self.clear();

        source.iter().for_each(|item| unsafe {
            self.push_back_unchecked(item.clone());
        });
    }
}

impl<T, const N: usize> From<[T; N]> for ArrayDeque<T, N> {
    #[inline]
    fn from(value: [T; N]) -> Self {
        let mut deque = Self::new();
        let dst = &raw mut deque.slots as *mut [T; N];
        // SAFETY:
        // - `[T; N]` and `[MaybeUninit<T>; N]` have identical layout (same size, alignment, ABI),
        //   so the pointer cast `*mut [MaybeUninit<T>; N]` → `*mut [T; N]` is valid.
        // - `ptr::write(dst, value)` overwrites the uninitialized `MaybeUninit` slots with N
        //   initialized `T` values, without dropping old content (which was uninitialized).
        unsafe {
            ptr::write(dst, value);
        }
        deque.len = N;
        deque.tail = 0;

        deque
    }
}

// ----------------------------------------------------------------------------
// Iter & IterMut & IntoIter

/// Shared iterator for `ArrayDeque`.
#[derive(Debug)]
pub struct Iter<'a, T> {
    inner: Chain<slice::Iter<'a, T>, slice::Iter<'a, T>>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<T> ExactSizeIterator for Iter<'_, T> {}
impl<T> FusedIterator for Iter<'_, T> {}

/// Mutable iterator for `ArrayDeque`.
#[derive(Debug)]
pub struct IterMut<'a, T> {
    inner: Chain<slice::IterMut<'a, T>, slice::IterMut<'a, T>>,
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<T> ExactSizeIterator for IterMut<'_, T> {}
impl<T> FusedIterator for IterMut<'_, T> {}

/// Owning iterator for `ArrayDeque`.
#[derive(Debug, Clone)]
pub struct IntoIter<T, const N: usize> {
    deque: ArrayDeque<T, N>,
}

impl<T, const N: usize> Iterator for IntoIter<T, N> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.deque.pop_front()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.deque.len(), Some(self.deque.len()))
    }
}

impl<T, const N: usize> ExactSizeIterator for IntoIter<T, N> {}
impl<T, const N: usize> FusedIterator for IntoIter<T, N> {}

impl<T, const N: usize> IntoIterator for ArrayDeque<T, N> {
    type Item = T;
    type IntoIter = IntoIter<T, N>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter { deque: self }
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a ArrayDeque<T, N> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a mut ArrayDeque<T, N> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

// ----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::ArrayDeque;
    use core::sync::atomic::{AtomicUsize, Ordering};

    macro_rules! define_tracker {
        ($n:ident, $t:ident) => {
            static $n: AtomicUsize = AtomicUsize::new(0);

            #[derive(Debug)]
            struct $t;

            impl Drop for $t {
                fn drop(&mut self) {
                    $n.fetch_add(1, Ordering::SeqCst);
                }
            }
        };
    }

    #[test]
    fn is_sync_send() {
        use core::panic::{RefUnwindSafe, UnwindSafe};

        fn is_send<T: Send>() {}
        fn is_sync<T: Send>() {}
        fn is_unwindsafe<T: UnwindSafe>() {}
        fn is_refunwindsafe<T: RefUnwindSafe>() {}

        is_send::<ArrayDeque<i32, 0>>();
        is_sync::<ArrayDeque<i32, 0>>();
        is_unwindsafe::<ArrayDeque<i32, 0>>();
        is_refunwindsafe::<ArrayDeque<i32, 0>>();
    }

    #[test]
    fn drop_contiguous() {
        define_tracker!(DROPS, Tracker);
        DROPS.store(0, Ordering::SeqCst);

        {
            let mut deque: ArrayDeque<Tracker, 8> = ArrayDeque::new();
            deque.push_back(Tracker).unwrap();
            deque.push_back(Tracker).unwrap();
            deque.push_back(Tracker).unwrap();

            assert_eq!(DROPS.load(Ordering::SeqCst), 0);
        }

        assert_eq!(DROPS.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn drop_full() {
        define_tracker!(DROPS, Tracker);
        DROPS.store(0, Ordering::SeqCst);

        {
            let mut deque: ArrayDeque<Tracker, 4> = ArrayDeque::new();
            deque.push_back(Tracker).unwrap();
            deque.push_back(Tracker).unwrap();
            deque.push_back(Tracker).unwrap();
            deque.push_back(Tracker).unwrap();
            assert!(deque.is_full());

            assert_eq!(DROPS.load(Ordering::SeqCst), 0);
        }

        assert_eq!(DROPS.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn drop_wrapped() {
        define_tracker!(DROPS, Tracker);
        DROPS.store(0, Ordering::SeqCst);

        {
            let mut deque: ArrayDeque<Tracker, 5> = ArrayDeque::new();
            deque.push_back(Tracker).unwrap();
            deque.push_back(Tracker).unwrap();
            deque.push_back(Tracker).unwrap();
            deque.push_back(Tracker).unwrap();

            let v1 = deque.pop_front().unwrap();
            let v2 = deque.pop_front().unwrap();
            drop(v1);
            drop(v2);
            assert_eq!(DROPS.load(Ordering::SeqCst), 2);

            // Make internal range wrap around while len < N.
            deque.push_back(Tracker).unwrap();

            assert_eq!(deque.len(), 3);
        }

        assert_eq!(DROPS.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn drop_clear_pop() {
        define_tracker!(DROPS, Tracker);
        DROPS.store(0, Ordering::SeqCst);

        let mut deque: ArrayDeque<Tracker, 6> = ArrayDeque::new();
        deque.push_back(Tracker).unwrap();
        deque.push_back(Tracker).unwrap();
        deque.push_back(Tracker).unwrap();
        deque.push_back(Tracker).unwrap();

        let front = deque.pop_front().unwrap();
        assert_eq!(DROPS.load(Ordering::SeqCst), 0);
        drop(front);
        assert_eq!(DROPS.load(Ordering::SeqCst), 1);

        deque.clear();
        assert!(deque.is_empty());
        assert_eq!(DROPS.load(Ordering::SeqCst), 4);

        drop(deque);
        assert_eq!(DROPS.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn mut_peek_and_contains() {
        let mut deque: ArrayDeque<i32, 4> = ArrayDeque::new();
        deque.push_back(1).unwrap();
        deque.push_back(2).unwrap();

        assert!(deque.contains(&1));
        assert!(!deque.contains(&3));

        *deque.front_mut().unwrap() = 10;
        *deque.back_mut().unwrap() = 20;

        assert_eq!(deque.front(), Some(&10));
        assert_eq!(deque.back(), Some(&20));
    }

    #[test]
    fn get_and_get_mut() {
        let mut deque: ArrayDeque<i32, 5> = ArrayDeque::new();
        deque.push_back(1).unwrap();
        deque.push_back(2).unwrap();
        deque.push_back(3).unwrap();

        assert_eq!(deque.get(0), Some(&1));
        assert_eq!(deque.get(2), Some(&3));
        assert_eq!(deque.get(3), None);

        *deque.get_mut(1).unwrap() = 20;
        assert_eq!(deque.get(1), Some(&20));

        assert_eq!(deque.pop_front(), Some(1));
        deque.push_back(4).unwrap();
        deque.push_back(5).unwrap();
        assert_eq!(deque.get(0), Some(&20));
        assert_eq!(deque.get(3), Some(&5));
    }

    #[test]
    fn iter_is_fused() {
        let deque = ArrayDeque::<i32, 2>::from([1, 2]);
        let mut iter = deque.iter();
        assert_eq!(iter.next(), Some(&1));
        assert_eq!(iter.next(), Some(&2));
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn iter_mut_is_fused() {
        let mut deque = ArrayDeque::<i32, 2>::from([1, 2]);
        let mut iter = deque.iter_mut();
        assert_eq!(iter.next().map(|x| *x), Some(1));
        assert_eq!(iter.next().map(|x| *x), Some(2));
        assert_eq!(iter.next().map(|x| *x), None);
        assert_eq!(iter.next().map(|x| *x), None);
    }

    #[test]
    fn iter_wrap_order() {
        let mut deque: ArrayDeque<i32, 4> = ArrayDeque::new();
        deque.push_back(1).unwrap();
        deque.push_back(2).unwrap();
        deque.push_back(3).unwrap();

        assert_eq!(deque.pop_front(), Some(1));
        deque.push_back(4).unwrap();

        let got: Vec<_> = deque.iter().copied().collect();
        assert_eq!(got, [2, 3, 4]);
    }

    #[test]
    fn from_array_clone_and_into_iter() {
        let deque = ArrayDeque::<i32, 4>::from([1, 2, 3, 4]);
        assert_eq!(deque.len(), 4);

        let cloned = deque.clone();
        let got: Vec<_> = cloned.into_iter().collect();
        assert_eq!(got, [1, 2, 3, 4]);
    }
}
