//! Copyright (c) 2019 The Crossbeam Project Developers
//!
//! See <https://docs.rs/crate/crossbeam-queue/latest>
//!
//! - Version: 0.3.13
//! - Date: 2026/07/25
#![expect(unsafe_code, reason = "from crossbeam-queue")]

use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::panic::{RefUnwindSafe, UnwindSafe};
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use core::{fmt, ptr};

use super::Backoff;
use crate::ext::CachePadded;

// Bits indicating the state of a slot:
// * If a value has been written into the slot, `WRITE` is set.
// * If a value has been read from the slot, `READ` is set.
// * If the block is being destroyed, `DESTROY` is set.
const WRITE: usize = 1;
const READ: usize = 2;
const DESTROY: usize = 4;

// Each block covers one "lap" of indices.
const LAP: usize = 32;
// The maximum number of values a block can hold.
const BLOCK_CAP: usize = LAP - 1;
// How many lower bits are reserved for metadata.
const SHIFT: usize = 1;
// Indicates that the block is not the last one.
const HAS_NEXT: usize = 1;

/// A slot in a block.
struct Slot<T> {
    /// The value.
    value: UnsafeCell<MaybeUninit<T>>,

    /// The state of the slot.
    state: AtomicUsize,
}

impl<T> Slot<T> {
    /// Waits until a value is written into the slot.
    fn wait_write(&self) {
        let backoff = Backoff::new();
        while self.state.load(Ordering::Acquire) & WRITE == 0 {
            backoff.snooze();
        }
    }
}

/// A block in a linked list.
///
/// Each block in the list can hold up to `BLOCK_CAP` values.
struct Block<T> {
    /// The next block in the linked list.
    next: AtomicPtr<Block<T>>,

    /// Slots for values.
    slots: [Slot<T>; BLOCK_CAP],
}

impl<T> Block<T> {
    /// Creates an empty block.
    fn new() -> Box<Self> {
        // SAFETY: `Box::new_zeroed()` allocates zero-initialized memory. All
        // fields of `Block<T>` have valid zero representations: `AtomicPtr` is
        // null, `Slot<T>` contains `MaybeUninit<T>` (valid when zeroed) and
        // `AtomicUsize` (zero is valid), so `assume_init` is sound.
        unsafe { Box::new_zeroed().assume_init() }
    }

    /// Waits until the next pointer is set.
    fn wait_next(&self) -> *mut Self {
        let backoff = Backoff::new();
        loop {
            let next = self.next.load(Ordering::Acquire);
            if !next.is_null() {
                return next;
            }
            backoff.snooze();
        }
    }

    /// Sets the `DESTROY` bit in slots starting from `start` and destroys the block.
    ///
    /// # Safety
    /// - `this` must point to a valid `Block<T>` that was previously allocated
    ///   via `Box::into_raw` or `Block::new`.
    /// - No other thread may call `destroy` on the same block concurrently.
    /// - The block must be full (all slots written) with no active tail pushing
    ///   into it.
    unsafe fn destroy(this: *mut Self, start: usize) {
        // It is not necessary to set the `DESTROY` bit in the last slot because that slot has
        // begun destruction of the block.
        for i in start..BLOCK_CAP - 1 {
            let slot = unsafe { (*this).slots.get_unchecked(i) };

            // Mark the `DESTROY` bit if a thread is still using the slot.
            if slot.state.load(Ordering::Acquire) & READ == 0
                && slot.state.fetch_or(DESTROY, Ordering::AcqRel) & READ == 0
            {
                // If a thread is still using the slot, it will continue destruction of the block.
                return;
            }
        }
        // No thread is using the block, now it is safe to destroy it.
        drop(unsafe { Box::from_raw(this) });
    }

    /// Destroys the block. Only safe to call with exclusive access, when no other thread is using it.
    ///
    /// # Safety
    /// - `this` must point to a valid `Block<T>` that was previously allocated
    ///   via `Box::into_raw` or `Block::new`.
    /// - The caller must have exclusive access (`&mut self`-level isolation).
    unsafe fn destroy_mut(this: *mut Self) {
        drop(unsafe { Box::from_raw(this) });
    }
}

/// A position in a queue.
struct Position<T> {
    /// The index in the queue.
    index: AtomicUsize,

    /// The block in the linked list.
    block: AtomicPtr<Block<T>>,
}

/// An unbounded multi-producer multi-consumer queue.
///
/// This queue is implemented as a linked list of segments, where each segment is a small buffer
/// that can hold a handful of elements. There is no limit to how many elements can be in the queue
/// at a time. However, since segments need to be dynamically allocated as elements get pushed,
/// this queue is somewhat slower than [`ArrayQueue`].
///
/// [`ArrayQueue`]: super::ArrayQueue
///
/// # Examples
///
/// ```
/// use zlim_utils::sync::SegQueue;
///
/// let q = SegQueue::new();
///
/// q.push('a');
/// q.push('b');
///
/// assert_eq!(q.pop(), Some('a'));
/// assert_eq!(q.pop(), Some('b'));
/// assert!(q.pop().is_none());
/// ```
pub struct SegQueue<T> {
    /// The head of the queue.
    head: CachePadded<Position<T>>,

    /// The tail of the queue.
    tail: CachePadded<Position<T>>,

    /// Indicates that dropping a `SegQueue<T>` may drop values of type `T`.
    _marker: PhantomData<T>,
}

unsafe impl<T: Send> Send for SegQueue<T> {}
unsafe impl<T: Send> Sync for SegQueue<T> {}

impl<T> UnwindSafe for SegQueue<T> {}
impl<T> RefUnwindSafe for SegQueue<T> {}

impl<T> SegQueue<T> {
    /// Creates a new unbounded queue.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::SegQueue;
    ///
    /// let q = SegQueue::<i32>::new();
    /// ```
    pub const fn new() -> Self {
        Self {
            head: CachePadded::new(Position {
                block: AtomicPtr::new(ptr::null_mut()),
                index: AtomicUsize::new(0),
            }),
            tail: CachePadded::new(Position {
                block: AtomicPtr::new(ptr::null_mut()),
                index: AtomicUsize::new(0),
            }),
            _marker: PhantomData,
        }
    }

    /// Pushes back an element to the tail.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::SegQueue;
    ///
    /// let q = SegQueue::new();
    ///
    /// q.push(10);
    /// q.push(20);
    /// ```
    pub fn push(&self, value: T) {
        let backoff = Backoff::new();
        let mut tail = self.tail.index.load(Ordering::Acquire);
        let mut block = self.tail.block.load(Ordering::Acquire);
        let mut next_block = None;

        loop {
            // Calculate the offset of the index into the block.
            let offset = (tail >> SHIFT) % LAP;

            // If we reached the end of the block, wait until the next one is installed.
            if offset == BLOCK_CAP {
                backoff.snooze();
                tail = self.tail.index.load(Ordering::Acquire);
                block = self.tail.block.load(Ordering::Acquire);
                continue;
            }

            // If we're going to have to install the next block, allocate it in advance in order to
            // make the wait for other threads as short as possible.
            if offset + 1 == BLOCK_CAP && next_block.is_none() {
                next_block = Some(Block::<T>::new());
            }

            // If this is the first push operation, we need to allocate the first block.
            if block.is_null() {
                let new = Box::into_raw(Block::<T>::new());

                if self
                    .tail
                    .block
                    .compare_exchange(block, new, Ordering::Release, Ordering::Relaxed)
                    .is_ok()
                {
                    self.head.block.store(new, Ordering::Release);
                    block = new;
                } else {
                    next_block = unsafe { Some(Box::from_raw(new)) };
                    tail = self.tail.index.load(Ordering::Acquire);
                    block = self.tail.block.load(Ordering::Acquire);
                    continue;
                }
            }

            let new_tail = tail + (1 << SHIFT);

            // Try advancing the tail forward.
            match self.tail.index.compare_exchange_weak(
                tail,
                new_tail,
                Ordering::SeqCst,
                Ordering::Acquire,
            ) {
                Ok(_) => unsafe {
                    // If we've reached the end of the block, install the next one.
                    if offset + 1 == BLOCK_CAP {
                        let next_block = Box::into_raw(next_block.unwrap());
                        let next_index = new_tail.wrapping_add(1 << SHIFT);

                        self.tail.block.store(next_block, Ordering::Release);
                        self.tail.index.store(next_index, Ordering::Release);
                        (*block).next.store(next_block, Ordering::Release);
                    }

                    // Write the value into the slot.
                    let slot = (*block).slots.get_unchecked(offset);
                    slot.value.get().write(MaybeUninit::new(value));
                    slot.state.fetch_or(WRITE, Ordering::Release);

                    return;
                },
                Err(t) => {
                    tail = t;
                    block = self.tail.block.load(Ordering::Acquire);
                    backoff.spin();
                }
            }
        }
    }

    /// Pushes an element to the queue with exclusive mutable access.
    ///
    /// Avoids atomic operations and synchronization, assuming
    /// no other threads access the queue concurrently.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::SegQueue;
    ///
    /// let mut q = SegQueue::new();
    ///
    /// q.push_mut(10);
    /// q.push_mut(20);
    /// ```
    pub fn push_mut(&mut self, value: T) {
        let tail = *self.tail.index.get_mut();
        let mut block = *self.tail.block.get_mut();

        // Calculate the offset of the index into the block.
        let offset = (tail >> SHIFT) % LAP;

        // If this is the first push operation, we need to allocate the first block.
        if block.is_null() {
            let new = Box::into_raw(Block::<T>::new());
            *self.head.block.get_mut() = new;
            *self.tail.block.get_mut() = new;

            block = new;
        }

        let new_tail = tail + (1 << SHIFT);

        *self.tail.index.get_mut() = new_tail;

        unsafe {
            // If we've reached the end of the block, install the next one.
            if offset + 1 == BLOCK_CAP {
                let next_block = Box::into_raw(Block::<T>::new());
                let next_index = new_tail.wrapping_add(1 << SHIFT);

                *self.tail.block.get_mut() = next_block;
                *self.tail.index.get_mut() = next_index;
                *(*block).next.get_mut() = next_block;
            }

            // Write the value into the slot.
            let slot = (*block).slots.get_unchecked(offset);
            slot.value.get().write(MaybeUninit::new(value));
            *(*block).slots.get_unchecked_mut(offset).state.get_mut() |= WRITE;
        }
    }

    /// Pops the head element from the queue.
    ///
    /// If the queue is empty, `None` is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::SegQueue;
    ///
    /// let q = SegQueue::new();
    ///
    /// q.push(10);
    /// q.push(20);
    /// assert_eq!(q.pop(), Some(10));
    /// assert_eq!(q.pop(), Some(20));
    /// assert!(q.pop().is_none());
    /// ```
    pub fn pop(&self) -> Option<T> {
        let backoff = Backoff::new();
        let mut head = self.head.index.load(Ordering::Acquire);
        let mut block = self.head.block.load(Ordering::Acquire);

        loop {
            // Calculate the offset of the index into the block.
            let offset = (head >> SHIFT) % LAP;

            // If we reached the end of the block, wait until the next one is installed.
            if offset == BLOCK_CAP {
                backoff.snooze();
                head = self.head.index.load(Ordering::Acquire);
                block = self.head.block.load(Ordering::Acquire);
                continue;
            }

            let mut new_head = head + (1 << SHIFT);

            if new_head & HAS_NEXT == 0 {
                core::sync::atomic::fence(Ordering::SeqCst);
                let tail = self.tail.index.load(Ordering::Relaxed);

                // If the tail equals the head, that means the queue is empty.
                if head >> SHIFT == tail >> SHIFT {
                    return None;
                }

                // If head and tail are not in the same block, set `HAS_NEXT` in head.
                if (head >> SHIFT) / LAP != (tail >> SHIFT) / LAP {
                    new_head |= HAS_NEXT;
                }
            }

            // The block can be null here only if the first push operation is in progress. In that
            // case, just wait until it gets initialized.
            if block.is_null() {
                backoff.snooze();
                head = self.head.index.load(Ordering::Acquire);
                block = self.head.block.load(Ordering::Acquire);
                continue;
            }

            // Try moving the head index forward.
            match self.head.index.compare_exchange_weak(
                head,
                new_head,
                Ordering::SeqCst,
                Ordering::Acquire,
            ) {
                Ok(_) => unsafe {
                    // If we've reached the end of the block, move to the next one.
                    if offset + 1 == BLOCK_CAP {
                        let next = (*block).wait_next();
                        let mut next_index = (new_head & !HAS_NEXT).wrapping_add(1 << SHIFT);
                        if !(*next).next.load(Ordering::Relaxed).is_null() {
                            next_index |= HAS_NEXT;
                        }

                        self.head.block.store(next, Ordering::Release);
                        self.head.index.store(next_index, Ordering::Release);
                    }

                    // Read the value.
                    let slot = (*block).slots.get_unchecked(offset);
                    slot.wait_write();
                    let value = slot.value.get().read().assume_init();

                    // Destroy the block if we've reached the end, or if another thread wanted to
                    // destroy but couldn't because we were busy reading from the slot.
                    if offset + 1 == BLOCK_CAP {
                        Block::destroy(block, 0);
                    } else if slot.state.fetch_or(READ, Ordering::AcqRel) & DESTROY != 0 {
                        Block::destroy(block, offset + 1);
                    }

                    return Some(value);
                },
                Err(h) => {
                    head = h;
                    block = self.head.block.load(Ordering::Acquire);
                    backoff.spin();
                }
            }
        }
    }

    /// Pops the head element from the queue using an exclusive reference.
    ///
    /// Avoids atomic operations and synchronization, assuming
    /// no other threads access the queue concurrently.
    ///
    /// If the queue is empty, `None` is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::SegQueue;
    ///
    /// let mut q = SegQueue::new();
    ///
    /// q.push(10);
    /// q.push(20);
    /// assert_eq!(q.pop_mut(), Some(10));
    /// assert_eq!(q.pop_mut(), Some(20));
    /// assert!(q.pop_mut().is_none());
    /// ```
    pub fn pop_mut(&mut self) -> Option<T> {
        let head = *self.head.index.get_mut();
        let block = *self.head.block.get_mut();

        // Calculate the offset of the index into the block.
        let offset = (head >> SHIFT) % LAP;

        let mut new_head = head + (1 << SHIFT);

        if new_head & HAS_NEXT == 0 {
            let tail = *self.tail.index.get_mut();

            // If the tail equals the head, that means the queue is empty.
            if head >> SHIFT == tail >> SHIFT {
                return None;
            }

            // If head and tail are not in the same block, set `HAS_NEXT` in head.
            if (head >> SHIFT) / LAP != (tail >> SHIFT) / LAP {
                new_head |= HAS_NEXT;
            }
        }

        *self.head.index.get_mut() = new_head;

        unsafe {
            // If we've reached the end of the block, move to the next one.
            if offset + 1 == BLOCK_CAP {
                let next = *(*block).next.get_mut();
                let mut next_index = (new_head & !HAS_NEXT).wrapping_add(1 << SHIFT);
                if !(*next).next.get_mut().is_null() {
                    next_index |= HAS_NEXT;
                }

                *self.head.block.get_mut() = next;
                *self.head.index.get_mut() = next_index;
            }

            // Read the value.
            let slot = (*block).slots.get_unchecked(offset);
            let value = slot.value.get().read().assume_init();

            // Destroy the block if we've reached the end
            if offset + 1 == BLOCK_CAP {
                Block::destroy_mut(block);
            } else {
                let state = *(*block).slots.get_unchecked_mut(offset).state.get_mut();
                *(*block).slots.get_unchecked_mut(offset).state.get_mut() = state | READ;
                if state & DESTROY != 0 {
                    Block::destroy(block, offset + 1);
                }
            }

            Some(value)
        }
    }

    /// Returns `true` if the queue is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::SegQueue;
    ///
    /// let q = SegQueue::new();
    ///
    /// assert!(q.is_empty());
    /// q.push(1);
    /// assert!(!q.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        let head = self.head.index.load(Ordering::SeqCst);
        let tail = self.tail.index.load(Ordering::SeqCst);
        head >> SHIFT == tail >> SHIFT
    }

    /// Returns the number of elements in the queue.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::sync::SegQueue;
    ///
    /// let q = SegQueue::new();
    /// assert_eq!(q.len(), 0);
    ///
    /// q.push(10);
    /// assert_eq!(q.len(), 1);
    ///
    /// q.push(20);
    /// assert_eq!(q.len(), 2);
    /// ```
    pub fn len(&self) -> usize {
        loop {
            // Load the tail index, then load the head index.
            let mut tail = self.tail.index.load(Ordering::SeqCst);
            let mut head = self.head.index.load(Ordering::SeqCst);

            // If the tail index didn't change, we've got consistent indices to work with.
            if self.tail.index.load(Ordering::SeqCst) == tail {
                // Erase the lower bits.
                tail &= !((1 << SHIFT) - 1);
                head &= !((1 << SHIFT) - 1);

                // Fix up indices if they fall onto block ends.
                if (tail >> SHIFT) & (LAP - 1) == LAP - 1 {
                    tail = tail.wrapping_add(1 << SHIFT);
                }
                if (head >> SHIFT) & (LAP - 1) == LAP - 1 {
                    head = head.wrapping_add(1 << SHIFT);
                }

                // Rotate indices so that head falls into the first block.
                let lap = (head >> SHIFT) / LAP;
                tail = tail.wrapping_sub((lap * LAP) << SHIFT);
                head = head.wrapping_sub((lap * LAP) << SHIFT);

                // Remove the lower bits.
                tail >>= SHIFT;
                head >>= SHIFT;

                // Return the difference minus the number of blocks between tail and head.
                return tail - head - tail / LAP;
            }
        }
    }
}

impl<T> Drop for SegQueue<T> {
    fn drop(&mut self) {
        let mut head = *self.head.index.get_mut();
        let mut tail = *self.tail.index.get_mut();
        let mut block = *self.head.block.get_mut();

        // Erase the lower bits.
        head &= !((1 << SHIFT) - 1);
        tail &= !((1 << SHIFT) - 1);

        unsafe {
            // Drop all values between `head` and `tail` and deallocate the heap-allocated blocks.
            while head != tail {
                let offset = (head >> SHIFT) % LAP;

                if offset < BLOCK_CAP {
                    // Drop the value in the slot.
                    let slot = (*block).slots.get_unchecked(offset);
                    (*slot.value.get()).assume_init_drop();
                } else {
                    // Deallocate the block and move to the next one.
                    let next = *(*block).next.get_mut();
                    drop(Box::from_raw(block));
                    block = next;
                }

                head = head.wrapping_add(1 << SHIFT);
            }

            // Deallocate the last remaining block.
            if !block.is_null() {
                drop(Box::from_raw(block));
            }
        }
    }
}

impl<T> fmt::Debug for SegQueue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("SegQueue { .. }")
    }
}

impl<T> Default for SegQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::thread::scope;

    use super::SegQueue;

    #[test]
    fn smoke() {
        let q = SegQueue::default();
        q.push(7);
        assert_eq!(q.pop(), Some(7));

        q.push(8);
        assert_eq!(q.pop(), Some(8));
        assert!(q.pop().is_none());
    }

    #[test]
    fn len_empty_full() {
        let q = SegQueue::default();

        assert_eq!(q.len(), 0);
        assert!(q.is_empty());

        q.push(());

        assert_eq!(q.len(), 1);
        assert!(!q.is_empty());

        q.pop().unwrap();

        assert_eq!(q.len(), 0);
        assert!(q.is_empty());
    }

    #[test]
    fn len() {
        let q = SegQueue::default();

        assert_eq!(q.len(), 0);

        for i in 0..50 {
            q.push(i);
            assert_eq!(q.len(), i + 1);
        }

        for i in 0..50 {
            q.pop().unwrap();
            assert_eq!(q.len(), 50 - i - 1);
        }

        assert_eq!(q.len(), 0);
    }

    #[test]
    fn spsc() {
        #[cfg(miri)]
        const COUNT: usize = 50;
        #[cfg(not(miri))]
        const COUNT: usize = 100_000;

        let q = SegQueue::default();

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
                    q.push(i);
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

        let q = SegQueue::<usize>::default();
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
                        q.push(i);
                    }
                });
            }
        });

        for c in v {
            assert_eq!(c.load(Ordering::SeqCst), THREADS);
        }
    }
}
