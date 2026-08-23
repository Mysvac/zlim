//! A bounded, lock-free work-stealing deque for per-worker task queues.
//!
//! A replication of the classic **Chase-Lev** work-stealing deque, adapted to
//! a fixed-capacity ring of [`Runnable`] slots.
//!
//! # Roles
//!
//! - **Owner** — the worker thread that owns this queue. It calls
//!   [`push`](Self::push) to enqueue tasks and [`pop`](Self::pop) to take
//!   the most recent one (LIFO, the same end it pushes to). These are the
//!   hot paths: `push` is a plain write plus a `Release` store, and `pop`
//!   only performs a CAS when racing stealers for the last element.
//! - **Stealers** — any number of other workers call [`steal`](Self::steal)
//!   to take the *oldest* task (FIFO) with a single CAS.
//!
//! > **Single-owner contract**: `push` and `pop` may only be called by one
//! > thread (the owner). The owner is not `Sync`-checked — callers must
//! > ensure this.
//!
//! > **Never-full contract**: the caller guarantees the deque never holds
//! > more than [`LocalDeque::CAP`] tasks (the steal side drains fast
//! > enough). `push` is therefore `unsafe` and performs **no** capacity
//! > check — not even a `debug_assert`; overflowing the ring writes into a
//! > live slot and is undefined behavior.
//!
//! # Algorithm
//!
//! The ring has [`SLOT_LENGTH`] slots and a usable capacity of
//! [`LocalDeque::CAP`] = `SLOT_LENGTH - 1`, leaving one slack slot so the
//! owner can never overwrite a slot a stealer is concurrently reading. Two
//! monotonically growing indices address the ring:
//!
//! - `tail` — the owner's index: the next [`push`](Self::push) position.
//! - `head` — the stealers' index: the next [`steal`](Self::steal) position.
//!
//! Live elements occupy indices `[head, tail)`. `push` writes the slot and
//! bumps `tail` with a `Release` store; `steal` loads `tail` with `Acquire`,
//! so a pushed value is always visible to a successful stealer. `pop` only
//! reads slots the owner itself wrote.
//!
//! `pop`'s fast path (two or more elements) tentatively decrements `tail`
//! and — after a `SeqCst` fence — re-checks `head`. While `head` is still
//! below the popped slot, that slot is exclusively the owner's and is read
//! directly. If `head` caught up — or overshot it, because a stealer with a
//! stale `tail` read already claimed the element — the owner resolves the
//! classic **handoff** inline with a single CAS on `head`: success means it
//! won the element, failure means a stealer did, so the pop returns either
//! way without retrying.
//!
//! Handoff candidates are read as a peek (a `MaybeUninit` copy) *before*
//! the deciding CAS, except in `pop`'s inline handoff, where the owner
//! reads the slot only after winning the CAS. A lost peek is leaked —
//! never written back, because the slot still owns the value and writing
//! it back would race the winner's read — so exactly one party ever
//! returns a given element.
//!
//! # Thread death
//!
//! Like most lock-free structures, progress is only guaranteed while no
//! thread dies mid-operation.
#![expect(unsafe_code, reason = "raw pointer is unsafe")]

use core::cell::UnsafeCell;
use core::fmt;
use core::mem::MaybeUninit;
use core::panic::{RefUnwindSafe, UnwindSafe};
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering::{Acquire, Relaxed, Release, AcqRel, SeqCst};

use async_task::Runnable;

use zlim_utils::ext::CachePadded;
use zlim_utils::sync::Backoff;
use zlim_utils::vec::ArrayVec;

// -----------------------------------------------------------------------------
// Slot

/// A slot in the ring buffer.
#[repr(align(32))] // Slightly pad to reduce competition.
struct RunnableSlot {
    value: UnsafeCell<MaybeUninit<Runnable>>,
}

// -----------------------------------------------------------------------------
// LocalDeque

/// Ring length: a power of two, one larger than the usable capacity.
const SLOT_LENGTH: usize = 64;
/// Index mask (`SLOT_LENGTH - 1`).
const MASK: usize = SLOT_LENGTH - 1;
/// The usable capacity: the deque is designed to hold at most this many
/// tasks (the pool must not push more; see the never-full contract).
const CAPACITY: usize = 63;

/// A bounded, lock-free work-stealing deque of [`Runnable`] tasks.
///
/// The owner thread pushes to and pops from the back (LIFO); any thread can
/// steal the oldest task from the front (FIFO). See the
/// [module documentation](self) for the algorithm and the single-owner
/// contract.
pub(super) struct LocalDeque {
    /// Stealers' index: the next `steal` position (front / oldest tasks).
    head: CachePadded<AtomicUsize>,
    /// Owner's index: the next `push` position (back / newest tasks).
    tail: CachePadded<AtomicUsize>,
    /// The ring buffer of [`SLOT_LENGTH`] slots.
    buffer: Box<[RunnableSlot]>,
}

unsafe impl Sync for LocalDeque {}
unsafe impl Send for LocalDeque {}
impl UnwindSafe for LocalDeque {}
impl RefUnwindSafe for LocalDeque {}

impl LocalDeque {
    /// The usable capacity (`SLOT_LENGTH - 1`; one slack slot keeps the
    /// ring safe).
    pub(super) const CAP: usize = CAPACITY;

    /// Creates a new empty deque.
    pub(super) fn new() -> Self {
        let mut buf = Vec::<RunnableSlot>::with_capacity(SLOT_LENGTH);
        // SAFETY: all slots are uninitialized `MaybeUninit` — they are only
        // read after being written by `push`.
        unsafe { buf.set_len(SLOT_LENGTH) }

        Self {
            head: CachePadded::new(AtomicUsize::new(0)),
            tail: CachePadded::new(AtomicUsize::new(0)),
            buffer: buf.into_boxed_slice(),
        }
    }

    /// Returns the slot at the given ring index.
    #[inline(always)]
    fn slot(&self, index: usize) -> &RunnableSlot {
        let index = index & MASK;
        // SAFETY: `index` is bounded by `MASK`, which is `SLOT_LENGTH - 1`.
        unsafe { self.buffer.get_unchecked(index) }
    }


    /// Pushes a task onto the back of the deque.
    ///
    /// **Owner only** — may only be called by the thread that owns this
    /// deque.
    /// 
    /// Push can only be called on local threads, and the caller ensures sufficient capacity.
    /// At this point, tail will not be moved by pop, so it is safe.
    /// 
    /// # Safety
    ///
    /// - This must only be called by the owner thread (single-owner
    ///   contract).
    /// - The caller must guarantee the deque is **not full**: it never
    ///   holds more than `CAP` tasks. There is **no** capacity check —
    ///   overflowing the ring writes into a live slot and is undefined behavior.
    pub(super) unsafe fn push(&self, runnable: Runnable) {
        let tail = self.tail.load(Acquire);

        #[cfg(debug_assertions)]
        {
            let head = self.head.load(Acquire);
            assert!(tail.wrapping_sub(head) < Self::CAP);
        }

        // SAFETY: the caller guarantees the deque is not full, so the slot
        // at `tail & MASK` holds no live element; and the owner contract
        // guarantees no other thread writes it.
        let slot = self.slot(tail);
        
        unsafe {
            slot.value.get().write(MaybeUninit::new(runnable));
        }

        // Release: make the write visible to stealers before bumping `tail`.
        self.tail.store(tail + 1, Release);
    }

    /// Pushes all remaining runnables in `runnables` onto the back of the
    /// deque, in reverse `pop` order.
    ///
    /// **Owner only** — may only be called by the thread that owns this
    /// deque. `runnables` is drained (emptied) by this call.
    ///
    /// # Safety
    ///
    /// - This must only be called by the owner thread (single-owner
    ///   contract).
    /// - The caller must guarantee the deque is **not full**: after the
    ///   push it holds at most `CAP` tasks. There is **no** capacity
    ///   check — not even a `debug_assert`; overflowing the ring writes
    ///   into a live slot and is undefined behavior.
    pub(super) unsafe fn push_many(&self, runnables: &mut ArrayVec<Runnable, CAPACITY>) {
        let mut tail: usize = self.tail.load(Relaxed);

        #[cfg(debug_assertions)]
        {
            let len = runnables.len();
            let head = self.head.load(Acquire);
            assert!(tail.wrapping_add(len).wrapping_sub(head) <= Self::CAP);
        }

        while let Some(runnable) = runnables.pop() {
            // SAFETY: the caller guarantees the deque is not full, so the
            // slot at `tail & MASK` holds no live element; and the owner
            // contract guarantees no other thread writes it.
            let slot = self.slot(tail);
            unsafe {
                slot.value.get().write(MaybeUninit::new(runnable));
            }
            tail += 1;
        }

        // Release: make the writes visible to stealers before bumping `tail`.
        self.tail.store(tail, Release);
    }

    /// Pops the most recently pushed task from the back of the deque.
    ///
    /// **Owner only** — may only be called by the thread that owns this
    /// deque (LIFO end). Returns `None` if the deque is empty.
    /// 
    /// Pop can only be called on local threads. If it is the last element,
    /// it is mandatory to move the Head. If it is not the last element, move the tail.
    pub(super) fn pop(&self) -> Option<Runnable> {
        let backoff = Backoff::new();

        let tail = self.tail.load(Relaxed);
        let new_tail: usize = tail.wrapping_sub(1);

        loop {
            let head = self.head.load(Acquire);

            if tail.wrapping_sub(head.wrapping_add(1)) >= Self::CAP {
                return None;
            }
            // `tail - (head + 1)` is `num - 1` for `num = tail - head`:
            // `None` when empty (`num == 0`) or over capacity
            // (`num > Self::CAP`). The equivalent length check needs an
            // extra subtraction.

            if new_tail != head {
                self.tail.store(new_tail, Release);
                ::core::sync::atomic::fence(SeqCst);

                // A stealer may have caught up to our tentative tail — or
                // overshot it, if it claimed this last element with a stale
                // `tail` read. `new_head` is then `new_tail` (the element
                // became the *last* one) or `tail` (a stealer already took
                // it); either way it can no longer be read directly — it
                // must be claimed through a CAS on `head`.

                let new_head = self.head.load(Acquire);

                // 1. `new_head == tail` — already stolen: the CAS below
                //    fails and we return `None`.
                // 2. `new_head == new_tail` — the element became the last
                //    one: claim it with the CAS handoff below.
                // 3. `new_head < new_tail` — more elements remain: the
                //    slot is exclusively ours, read it directly.

                if new_tail.wrapping_sub(new_head.wrapping_add(1)) < Self::CAP {
                    let slot = self.slot(new_tail);
                    let value: Runnable = unsafe { slot.value.get().read().assume_init() };
                    return Some(value);
                }
                ::core::hint::cold_path();

                // Handoff: the CAS moves `head` from `new_tail` to `tail` —
                // the same transition stealers attempt, so exactly one
                // party wins. On success we own the last element (restore
                // `tail` and return it); on failure a stealer consumed it
                // (restore `tail` and return `None`).
                if self.head.compare_exchange(new_tail, tail, AcqRel, Relaxed).is_ok() {
                    self.tail.store(tail, Release);
                    let slot = self.slot(new_tail);
                    let value: Runnable = unsafe { slot.value.get().read().assume_init() };
                    return Some(value);
                } else {
                    // Already stolen.
                    self.tail.store(tail, Release);
                    return None;
                }
            }

            ::core::hint::cold_path();

            let slot = self.slot(head);
            let value: MaybeUninit<Runnable> = unsafe { slot.value.get().read() };

            if self.head.compare_exchange(head, tail, SeqCst, Relaxed).is_ok() {
                return Some(unsafe { value.assume_init() });
            } else {
                backoff.snooze();
            }
        }
    }

    /// Steals the oldest task from the front of the deque.
    ///
    /// **Any thread** may call this (the steal end, FIFO). Returns `None` if
    /// the deque is empty.
    pub(super) fn steal(&self) -> Option<Runnable> {
        let backoff = Backoff::new();

        loop {
            // Load the front, then — with a SeqCst fence — the back. The
            // fence orders the two loads so a race with the owner's pop on
            // the last element is resolved consistently (Chase-Lev handoff).
            let head = self.head.load(Acquire);
            let tail = self.tail.load(SeqCst);

            let new_head = head.wrapping_add(1);

            if tail.wrapping_sub(new_head) >= Self::CAP {
                return None;
            }
            // `tail - (head + 1)` is `num - 1` for `num = tail - head`:
            // `None` when empty (`num == 0`) or over capacity
            // (`num > Self::CAP`). The equivalent length check needs an
            // extra subtraction.

            let slot = self.slot(head);
            let value: MaybeUninit<Runnable> = unsafe { slot.value.get().read() };

            if self.head.compare_exchange_weak(head, new_head, SeqCst, Relaxed).is_ok() {
                return Some(unsafe { value.assume_init() });
            } else {
                backoff.snooze();
            }
        }
    }

    /// Non precise length in concurrent mode.
    /// 
    /// Used to calculate the appropriate amount of theft.
    #[inline]
    pub(super) fn len(&self) -> usize {
        let head = self.head.load(Acquire);
        let tail = self.tail.load(Acquire);
        tail.wrapping_sub(head) & CAPACITY
    }
}

impl Default for LocalDeque {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LocalDeque {
    fn drop(&mut self) {
        // With `&mut self` we have exclusive access; drop every live task in
        // `[head, tail)`.
        let mut index = *self.head.get_mut();
        let tail = *self.tail.get_mut();
        while index != tail {
            // SAFETY: `index` is a live index in `[head, tail)`, so its slot
            // holds an initialized value. With `&mut self`, no other thread
            // accesses the deque.
            let slot = self.slot(index);
            unsafe {
                (*slot.value.get()).assume_init_drop();
            }
            index += 1;
        }
    }
}

impl fmt::Debug for LocalDeque {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("LocalDeque { .. }")
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use core::sync::atomic::AtomicUsize;
    use core::sync::atomic::Ordering::Relaxed;
    use std::collections::VecDeque;
    use std::thread::scope;

    use super::LocalDeque;

    /// Creates a runnable that increments `counters[id]` when run.
    fn make_runnable(counters: &'static [AtomicUsize], id: usize) -> async_task::Runnable {
        let (runnable, task) = async_task::spawn(
            async move {
                counters[id].fetch_add(1, Relaxed);
            },
            |_| {},
        );
        task.detach();
        runnable
    }

    fn run_and_drop(r: async_task::Runnable) {
        r.run();
    }

    #[test]
    fn smoke() {
        let d = LocalDeque::new();
        assert!(d.len() == 0);

        static C: [AtomicUsize; 2] = [const { AtomicUsize::new(0) }; 2];
        unsafe {
            d.push(make_runnable(&C, 0));
            d.push(make_runnable(&C, 1));
        }

        // Owner pop: LIFO.
        let r = d.pop().unwrap();
        run_and_drop(r);
        assert_eq!(C[1].load(Relaxed), 1);

        // Steal: FIFO.
        let r = d.steal().unwrap();
        run_and_drop(r);
        assert_eq!(C[0].load(Relaxed), 1);
        assert_eq!(C[1].load(Relaxed), 1);

        assert!(d.len() == 0);
        assert!(d.pop().is_none());
        assert!(d.steal().is_none());
    }

    #[test]
    fn owner_lifo() {
        let d = LocalDeque::new();
        static C: [AtomicUsize; 4] = [const { AtomicUsize::new(0) }; 4];
        for i in 0..4 {
            unsafe {
                d.push(make_runnable(&C, i));
            }
        }
        for i in (0..4).rev() {
            run_and_drop(d.pop().unwrap());
            assert_eq!(C[i].load(Relaxed), 1);
        }
    }

    #[test]
    fn steal_fifo() {
        let d = LocalDeque::new();
        static C: [AtomicUsize; 4] = [const { AtomicUsize::new(0) }; 4];
        for i in 0..4 {
            unsafe {
                d.push(make_runnable(&C, i));
            }
        }
        for c in &C {
            run_and_drop(d.steal().unwrap());
            assert_eq!(c.load(Relaxed), 1);
        }
    }

    #[test]
    fn capacity_holds_cap() {
        // The caller guarantees push never fills the deque; verify that
        // pushing exactly `CAP` tasks works and they all run once.
        let d = LocalDeque::new();
        static C: [AtomicUsize; LocalDeque::CAP] = [const { AtomicUsize::new(0) }; LocalDeque::CAP];

        for i in 0..LocalDeque::CAP {
            unsafe {
                d.push(make_runnable(&C, i));
            }
        }
        assert_eq!(d.len(), LocalDeque::CAP);

        while let Some(r) = d.steal() {
            run_and_drop(r);
        }
        for c in C.iter() {
            assert_eq!(c.load(Relaxed), 1);
        }
    }

    #[test]
    #[expect(clippy::print_stderr, reason = "diagnostics for failed stress runs")]
    fn model_stress() {
        // Single-thread model test against std::collections::VecDeque.
        #[cfg(miri)]
        const COUNT: usize = 100;
        #[cfg(not(miri))]
        const COUNT: usize = 10_000;

        static C: [AtomicUsize; COUNT] = [const { AtomicUsize::new(0) }; COUNT];
        let counters: &'static [AtomicUsize] = &C;

        let d = LocalDeque::new();
        let mut model = VecDeque::new();

        for i in 0..COUNT {
            match i % 3 {
                0 => {
                    // The push/pop/steal cycle keeps the deque well under
                    // `CAP`, satisfying the never-full contract.
                    unsafe {
                        d.push(make_runnable(counters, i));
                    }
                    model.push_back(i);
                }
                1 => match (model.pop_back(), d.pop()) {
                    (Some(id), Some(r)) => {
                        run_and_drop(r);
                        assert_eq!(C[id].load(Relaxed), 1);
                    }
                    (None, None) => {}
                    _ => panic!("pop out of sync with model"),
                },
                _ => match (model.pop_front(), d.steal()) {
                    (Some(id), Some(r)) => {
                        run_and_drop(r);
                        assert_eq!(C[id].load(Relaxed), 1);
                    }
                    (None, None) => {}
                    _ => panic!("steal out of sync with model"),
                },
            }
        }

        // Drain the remainder of both and compare.
        loop {
            match (model.pop_front(), d.steal()) {
                (Some(id), Some(r)) => {
                    run_and_drop(r);
                    assert_eq!(C[id].load(Relaxed), 1);
                }
                (None, None) => break,
                _ => panic!("drain out of sync with model"),
            }
        }
        assert!(d.len() == 0);

        // Every *pushed* task (ids with `i % 3 == 0`) ran exactly once.
        let mut bad = 0;
        for (i, c) in C.iter().enumerate().step_by(3) {
            let v = c.load(Relaxed);
            if v != 1 {
                eprintln!("[dbg] task {i} ran {v} times");
                bad += 1;
            }
        }
        assert_eq!(bad, 0, "{bad} tasks lost or duplicated");
    }

    #[test]
    fn owner_steal_exactly_once() {
        // One owner pushes and occasionally pops; two stealers steal. Every
        // pushed task must run exactly once.
        #[cfg(miri)]
        const COUNT: usize = 100;
        #[cfg(not(miri))]
        const COUNT: usize = 50_000;

        static C: [AtomicUsize; COUNT] = [const { AtomicUsize::new(0) }; COUNT];
        let counters: &'static [AtomicUsize] = &C;

        let d = LocalDeque::new();
        let finished = AtomicUsize::new(0);

        let d = &d;
        let finished = &finished;
        scope(|scope| {
            // Owner: push everything, occasionally pop the back ourselves.
            // The len guard keeps the never-full contract.
            scope.spawn(move || {
                let backoff = zlim_utils::sync::Backoff::new();
                for v in 0..COUNT {
                    while d.len() >= LocalDeque::CAP {
                        backoff.snooze();
                    }
                    unsafe {
                        d.push(make_runnable(counters, v));
                    }
                    if v % 100 == 0 {
                        while let Some(r) = d.pop() {
                            run_and_drop(r);
                        }
                    }
                }
                finished.fetch_add(1, Relaxed);
            });

            // Stealers: drain the front until the owner finished and the
            // deque is empty.
            for _ in 0..2 {
                scope.spawn(move || {
                    let backoff = zlim_utils::sync::Backoff::new();
                    loop {
                        if let Some(r) = d.steal() {
                            run_and_drop(r);
                        } else if finished.load(Relaxed) == 1 && d.len() == 0 {
                            break;
                        } else {
                            backoff.snooze();
                        }
                    }
                });
            }
        });

        for (i, c) in C.iter().enumerate() {
            assert_eq!(
                c.load(Relaxed),
                1,
                "task {i} ran {} times",
                c.load(Relaxed)
            );
        }
        assert!(d.len() == 0);
    }
}
