//! Copyright (c) 2019 The Crossbeam Project Developers
//!
//! See <https://docs.rs/crate/crossbeam-queue/latest>
//!
//! - Version: 0.3.13
//! - Date: 2026/07/25
#![expect(unsafe_code, reason = "from crossbeam-queue")]

use core::cell::UnsafeCell;
use core::fmt;
use core::mem::{self, MaybeUninit};
use core::panic::{RefUnwindSafe, UnwindSafe};
use core::sync::atomic::{AtomicUsize, Ordering};

use super::Backoff;
use crate::ext::CachePadded;

/// A slot in a queue.
struct Slot<T> {
    /// The current stamp.
    ///
    /// If the stamp equals the tail, this node will be next written to. If it equals head + 1,
    /// this node will be next read from.
    stamp: AtomicUsize,

    /// The value in this slot.
    value: UnsafeCell<MaybeUninit<T>>,
}

/// A bounded multi-producer multi-consumer queue.
///
/// This queue allocates a fixed-capacity buffer on construction, which
/// is used to store pushed elements. The queue cannot hold more elements
/// than the buffer allows. Attempting to push an element into a full queue
/// will fail. Having a buffer allocated upfront makes this queue a bit
/// faster than [`SegQueue`](super::SegQueue).
///
/// # Examples
///
/// ```
/// use zlim_utils::sync::ArrayQueue;
///
/// let q = ArrayQueue::new(2);
///
/// assert_eq!(q.push('a'), Ok(()));
/// assert_eq!(q.push('b'), Ok(()));
/// assert_eq!(q.push('c'), Err('c'));
/// assert_eq!(q.pop(), Some('a'));
/// ```
pub struct ArrayQueue<T> {
    /// The head of the queue.
    ///
    /// This value is a "stamp" consisting of an index into the buffer and a lap, but packed into a
    /// single `usize`. The lower bits represent the index, while the upper bits represent the lap.
    ///
    /// Elements are popped from the head of the queue.
    head: CachePadded<AtomicUsize>,

    /// The tail of the queue.
    ///
    /// This value is a "stamp" consisting of an index into the buffer and a lap, but packed into a
    /// single `usize`. The lower bits represent the index, while the upper bits represent the lap.
    ///
    /// Elements are pushed into the tail of the queue.
    tail: CachePadded<AtomicUsize>,

    /// The buffer holding slots.
    buffer: Box<[Slot<T>]>,

    /// A stamp with the value of `{ lap: 1, index: 0 }`.
    one_lap: usize,
}

unsafe impl<T: Send> Sync for ArrayQueue<T> {}
unsafe impl<T: Send> Send for ArrayQueue<T> {}

impl<T> UnwindSafe for ArrayQueue<T> {}
impl<T> RefUnwindSafe for ArrayQueue<T> {}

impl<T> ArrayQueue<T> {
    /// Creates a new bounded queue with the given capacity.
    ///
    /// # Panics
    ///
    /// Panics if the capacity is zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::ArrayQueue;
    ///
    /// let q = ArrayQueue::<i32>::new(100);
    /// ```
    pub fn new(cap: usize) -> Self {
        assert!(cap > 0, "capacity must be non-zero");

        // Head is initialized to `{ lap: 0, index: 0 }`.
        // Tail is initialized to `{ lap: 0, index: 0 }`.
        let head = 0;
        let tail = 0;

        // Allocate a buffer of `cap` slots initialized
        // with stamps.
        let buffer: Box<[Slot<T>]> = (0..cap)
            .map(|i| {
                // Set the stamp to `{ lap: 0, index: i }`.
                Slot {
                    stamp: AtomicUsize::new(i),
                    value: UnsafeCell::new(MaybeUninit::uninit()),
                }
            })
            .collect();

        // One lap is the smallest power of two greater than `cap`.
        let one_lap = (cap + 1).next_power_of_two();

        Self {
            buffer,
            one_lap,
            head: CachePadded::new(AtomicUsize::new(head)),
            tail: CachePadded::new(AtomicUsize::new(tail)),
        }
    }

    fn push_or_else<F>(&self, mut value: T, f: F) -> Result<(), T>
    where
        F: Fn(T, usize, usize, &Slot<T>) -> Result<T, T>,
    {
        let backoff = Backoff::new();
        let mut tail = self.tail.load(Ordering::Relaxed);

        loop {
            // Deconstruct the tail.
            let index = tail & (self.one_lap - 1);
            let lap = tail & !(self.one_lap - 1);

            let new_tail = if index + 1 < self.capacity() {
                // Same lap, incremented index.
                // Set to `{ lap: lap, index: index + 1 }`.
                tail + 1
            } else {
                // One lap forward, index wraps around to zero.
                // Set to `{ lap: lap.wrapping_add(1), index: 0 }`.
                lap.wrapping_add(self.one_lap)
            };

            // Inspect the corresponding slot.
            debug_assert!(index < self.buffer.len());
            // SAFETY: `index` = tail % cap, always < `self.buffer.len()`
            // by construction.  The buffer has exactly `cap` elements.
            let slot = unsafe { self.buffer.get_unchecked(index) };
            let stamp = slot.stamp.load(Ordering::Acquire);

            // If the tail and the stamp match, we may attempt to push.
            if tail == stamp {
                // Try moving the tail.
                match self.tail.compare_exchange_weak(
                    tail,
                    new_tail,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        // Write the value into the slot and update the stamp.
                        // SAFETY: The CAS succeeded, giving us exclusive write
                        // ownership of this slot. The slot's `MaybeUninit<T>` is
                        // uninitialized (confirmed by `tail == stamp`), so
                        // `write` is the correct operation.
                        unsafe {
                            slot.value.get().write(MaybeUninit::new(value));
                        }
                        slot.stamp.store(tail + 1, Ordering::Release);
                        return Ok(());
                    }
                    Err(t) => {
                        tail = t;
                        backoff.spin();
                    }
                }
            } else if stamp.wrapping_add(self.one_lap) == tail + 1 {
                core::sync::atomic::fence(Ordering::SeqCst);
                value = f(value, tail, new_tail, slot)?;
                backoff.spin();
                tail = self.tail.load(Ordering::Relaxed);
            } else {
                // Snooze because we need to wait for the stamp to get updated.
                backoff.snooze();
                tail = self.tail.load(Ordering::Relaxed);
            }
        }
    }

    /// Attempts to push an element into the queue.
    ///
    /// If the queue is full, the element is returned back as an error.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::ArrayQueue;
    ///
    /// let q = ArrayQueue::new(1);
    ///
    /// assert_eq!(q.push(10), Ok(()));
    /// assert_eq!(q.push(20), Err(20));
    /// ```
    pub fn push(&self, value: T) -> Result<(), T> {
        self.push_or_else(value, |v, tail, _, _| {
            let head = self.head.load(Ordering::Relaxed);

            // If the head lags one lap behind the tail as well...
            if head.wrapping_add(self.one_lap) == tail {
                // ...then the queue is full.
                Err(v)
            } else {
                Ok(v)
            }
        })
    }

    /// Attempts to push an element using an exclusive reference of the queue.
    ///
    /// Atomic operations and checks are omitted
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::ArrayQueue;
    ///
    /// let mut q = ArrayQueue::new(1);
    ///
    /// assert_eq!(q.push_mut(10), Ok(()));
    /// assert_eq!(q.push_mut(20), Err(20));
    /// ```
    pub fn push_mut(&mut self, value: T) -> Result<(), T> {
        let tail = *self.tail.get_mut();
        let head = *self.head.get_mut();

        if head.wrapping_add(self.one_lap) == tail {
            return Err(value);
        }

        let index = tail & (self.one_lap - 1);
        let lap = tail & !(self.one_lap - 1);
        let new_tail = if index + 1 < self.capacity() {
            tail + 1
        } else {
            lap.wrapping_add(self.one_lap)
        };

        *self.tail.get_mut() = new_tail;

        // SAFETY: With `&mut self` we have exclusive access. `index` is
        // verified by the capacity check above (via `head.wrapping_add(one_lap)
        // != tail`), so it is within bounds of the buffer.
        let slot = unsafe { self.buffer.get_unchecked_mut(index) };
        // SAFETY: The queue is not full (confirmed above), so the slot is not
        // in use. With `&mut self`, no concurrent reader writes to this slot.
        unsafe {
            slot.value.get().write(MaybeUninit::new(value));
        }
        *slot.stamp.get_mut() = tail + 1;

        Ok(())
    }

    /// Attempts to pop an element from the queue.
    ///
    /// If the queue is empty, `None` is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::ArrayQueue;
    ///
    /// let q = ArrayQueue::new(1);
    /// assert_eq!(q.push(10), Ok(()));
    ///
    /// assert_eq!(q.pop(), Some(10));
    /// assert!(q.pop().is_none());
    /// ```
    pub fn pop(&self) -> Option<T> {
        let backoff = Backoff::new();
        let mut head = self.head.load(Ordering::Relaxed);

        loop {
            // Deconstruct the head.
            let index = head & (self.one_lap - 1);
            let lap = head & !(self.one_lap - 1);

            // Inspect the corresponding slot.
            debug_assert!(index < self.buffer.len());
            // SAFETY: `index` is bounded by `self.one_lap - 1` (derived from
            // the bitwise-and with `self.one_lap - 1` above), and the buffer
            // has at least `self.one_lap - 1 >= capacity()` slots.
            let slot = unsafe { self.buffer.get_unchecked(index) };
            let stamp = slot.stamp.load(Ordering::Acquire);

            // If the stamp is ahead of the head by 1, we may attempt to pop.
            if head + 1 == stamp {
                let new = if index + 1 < self.capacity() {
                    // Same lap, incremented index.
                    // Set to `{ lap: lap, index: index + 1 }`.
                    head + 1
                } else {
                    // One lap forward, index wraps around to zero.
                    // Set to `{ lap: lap.wrapping_add(1), index: 0 }`.
                    lap.wrapping_add(self.one_lap)
                };

                // Try moving the head.
                match self.head.compare_exchange_weak(
                    head,
                    new,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        // Read the value from the slot and update the stamp.
                        // SAFETY: The CAS succeeded, giving us exclusive read
                        // ownership of this slot. `head + 1 == stamp` guarantees
                        // the slot was written by a prior `push` (confirmed by
                        // the Acquire load of stamp that synchronizes with the
                        // Release store in `push`). `read().assume_init()` reads
                        // the initialized `T`.
                        let msg = unsafe { slot.value.get().read().assume_init() };
                        slot.stamp
                            .store(head.wrapping_add(self.one_lap), Ordering::Release);
                        return Some(msg);
                    }
                    Err(h) => {
                        head = h;
                        backoff.spin();
                    }
                }
            } else if stamp == head {
                core::sync::atomic::fence(Ordering::SeqCst);
                let tail = self.tail.load(Ordering::Relaxed);

                // If the tail equals the head, that means the channel is empty.
                if tail == head {
                    return None;
                }

                backoff.spin();
                head = self.head.load(Ordering::Relaxed);
            } else {
                // Snooze because we need to wait for the stamp to get updated.
                backoff.snooze();
                head = self.head.load(Ordering::Relaxed);
            }
        }
    }

    /// Attempts to pop an element using an exclusive reference of the queue.
    ///
    /// Due to having an exclusive reference, atomic operations and checks are omitted
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::ArrayQueue;
    ///
    /// let mut q = ArrayQueue::new(1);
    /// assert_eq!(q.push(10), Ok(()));
    ///
    /// assert_eq!(q.pop_mut(), Some(10));
    /// assert!(q.pop_mut().is_none());
    /// ```
    pub fn pop_mut(&mut self) -> Option<T> {
        let head = *self.head.get_mut();
        let tail = *self.tail.get_mut();

        // If the tail equals the head, that means the channel is empty.
        if tail == head {
            return None;
        }
        let index = head & (self.one_lap - 1);
        let lap = head & !(self.one_lap - 1);

        // Inspect the corresponding slot.
        debug_assert!(index < self.buffer.len());

        let new = if index + 1 < self.capacity() {
            // Same lap, incremented index.
            // Set to `{ lap: lap, index: index + 1 }`.
            head + 1
        } else {
            // One lap forward, index wraps around to zero.
            // Set to `{ lap: lap.wrapping_add(1), index: 0 }`.
            lap.wrapping_add(self.one_lap)
        };

        // SAFETY: With `&mut self` we have exclusive access. `index` is
        // bounded by `self.one_lap - 1`, which is >= capacity.
        let slot = unsafe { self.buffer.get_unchecked_mut(index) };

        // SAFETY: The queue is not empty (confirmed by `tail != head` above),
        // so the slot contains an initialized value. With `&mut self`, no
        // concurrent writer modifies this slot.
        let msg = unsafe { slot.value.get().read().assume_init() };
        *slot.stamp.get_mut() = head.wrapping_add(self.one_lap);
        *self.head.get_mut() = new;
        Some(msg)
    }

    /// Returns the capacity of the queue.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::ArrayQueue;
    ///
    /// let q = ArrayQueue::<i32>::new(100);
    ///
    /// assert_eq!(q.capacity(), 100);
    /// ```
    #[inline]
    pub fn capacity(&self) -> usize {
        self.buffer.len()
    }

    /// Returns `true` if the queue is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::ArrayQueue;
    ///
    /// let q = ArrayQueue::new(100);
    ///
    /// assert!(q.is_empty());
    /// q.push(1).unwrap();
    /// assert!(!q.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        let head = self.head.load(Ordering::SeqCst);
        let tail = self.tail.load(Ordering::SeqCst);

        // Is the tail lagging one lap behind head?
        // Is the tail equal to the head?
        //
        // Note: If the head changes just before we load the tail, that means there was a moment
        // when the channel was not empty, so it is safe to just return `false`.
        tail == head
    }

    /// Returns `true` if the queue is full.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::ArrayQueue;
    ///
    /// let q = ArrayQueue::new(1);
    ///
    /// assert!(!q.is_full());
    /// q.push(1).unwrap();
    /// assert!(q.is_full());
    /// ```
    pub fn is_full(&self) -> bool {
        let tail = self.tail.load(Ordering::SeqCst);
        let head = self.head.load(Ordering::SeqCst);

        // Is the head lagging one lap behind tail?
        //
        // Note: If the tail changes just before we load the head, that means there was a moment
        // when the queue was not full, so it is safe to just return `false`.
        head.wrapping_add(self.one_lap) == tail
    }

    /// Returns the number of elements in the queue.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::ArrayQueue;
    ///
    /// let q = ArrayQueue::new(100);
    /// assert_eq!(q.len(), 0);
    ///
    /// q.push(10).unwrap();
    /// assert_eq!(q.len(), 1);
    ///
    /// q.push(20).unwrap();
    /// assert_eq!(q.len(), 2);
    /// ```
    pub fn len(&self) -> usize {
        loop {
            // Load the tail, then load the head.
            let tail = self.tail.load(Ordering::SeqCst);
            let head = self.head.load(Ordering::SeqCst);

            // If the tail didn't change, we've got consistent values to work with.
            if self.tail.load(Ordering::SeqCst) == tail {
                let hix = head & (self.one_lap - 1);
                let tix = tail & (self.one_lap - 1);

                return if hix < tix {
                    tix - hix
                } else if hix > tix {
                    self.capacity() - hix + tix
                } else if tail == head {
                    0
                } else {
                    self.capacity()
                };
            }
        }
    }
}

impl<T> Drop for ArrayQueue<T> {
    fn drop(&mut self) {
        if mem::needs_drop::<T>() {
            // Get the index of the head.
            let head = *self.head.get_mut();
            let tail = *self.tail.get_mut();

            let hix = head & (self.one_lap - 1);
            let tix = tail & (self.one_lap - 1);

            let len = if hix < tix {
                tix - hix
            } else if hix > tix {
                self.capacity() - hix + tix
            } else if tail == head {
                0
            } else {
                self.capacity()
            };

            // Loop over all slots that hold a message and drop them.
            for i in 0..len {
                // Compute the index of the next slot holding a message.
                let index = if hix + i < self.capacity() {
                    hix + i
                } else {
                    hix + i - self.capacity()
                };

                // SAFETY: With `&mut self` we have exclusive access during drop.
                // `index` is bounded by the lap arithmetic (hix + i, wrapped by
                // capacity), and confirmed by `debug_assert`. The slot at `index`
                // holds an initialized value (we computed `len` from head/tail
                // with exclusive access, so the count is exact).
                unsafe {
                    debug_assert!(index < self.buffer.len());
                    let slot = self.buffer.get_unchecked_mut(index);
                    (*slot.value.get()).assume_init_drop();
                }
            }
        }
    }
}

impl<T> fmt::Debug for ArrayQueue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("ArrayQueue { .. }")
    }
}

// ----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::thread::scope;

    use super::ArrayQueue;

    #[test]
    fn smoke() {
        let q = ArrayQueue::new(1);

        q.push(7).unwrap();
        assert_eq!(q.pop(), Some(7));

        q.push(8).unwrap();
        assert_eq!(q.pop(), Some(8));
        assert!(q.pop().is_none());
    }

    #[test]
    fn capacity() {
        for i in 1..10 {
            let q = ArrayQueue::<i32>::new(i);
            assert_eq!(q.capacity(), i);
        }
    }

    #[test]
    fn len_empty_full() {
        let q = ArrayQueue::new(2);

        assert_eq!(q.len(), 0);
        assert!(q.is_empty());
        assert!(!q.is_full());

        q.push(()).unwrap();

        assert_eq!(q.len(), 1);
        assert!(!q.is_empty());
        assert!(!q.is_full());

        q.push(()).unwrap();

        assert_eq!(q.len(), 2);
        assert!(!q.is_empty());
        assert!(q.is_full());

        q.pop().unwrap();

        assert_eq!(q.len(), 1);
        assert!(!q.is_empty());
        assert!(!q.is_full());
    }

    #[test]
    fn spsc() {
        #[cfg(miri)]
        const COUNT: usize = 50;
        #[cfg(not(miri))]
        const COUNT: usize = 100_000;

        let q = ArrayQueue::new(3);

        scope(|scope| {
            scope.spawn(|| {
                for i in 0..COUNT {
                    loop {
                        if let Some(x) = q.pop() {
                            assert_eq!(x, i);
                            break;
                        }
                    }
                }
                assert!(q.pop().is_none());
            });

            scope.spawn(|| {
                for i in 0..COUNT {
                    while q.push(i).is_err() {}
                }
            });
        });
    }

    #[test]
    fn mpmc() {
        #[cfg(miri)]
        const COUNT: usize = 50;
        #[cfg(not(miri))]
        const COUNT: usize = 25_000;
        const THREADS: usize = 4;

        let q = ArrayQueue::<usize>::new(3);
        let v = (0..COUNT).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>();

        scope(|scope| {
            for _ in 0..THREADS {
                scope.spawn(|| {
                    for _ in 0..COUNT {
                        let n = loop {
                            if let Some(x) = q.pop() {
                                break x;
                            }
                        };
                        v[n].fetch_add(1, Ordering::SeqCst);
                    }
                });
            }
            for _ in 0..THREADS {
                scope.spawn(|| {
                    for i in 0..COUNT {
                        while q.push(i).is_err() {}
                    }
                });
            }
        });

        for c in v {
            assert_eq!(c.load(Ordering::SeqCst), THREADS);
        }
    }
}
