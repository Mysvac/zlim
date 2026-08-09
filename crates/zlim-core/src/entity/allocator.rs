//! Lock-free entity-id allocator.
//!
//! Ids are handed out from two sources, in priority order:
//!
//! - a [`FreeList`] of recycled ids (from destroyed entities), and
//! - a [`FreshAllocator`] that mints brand-new, never-before-used ids.
//!
//! The shared state lives behind an [`Arc`], so the owning [`EntityAllocator`]
//! and any number of cloned [`RemoteAllocator`]s on other threads can allocate
//! concurrently. The owner additionally batches frees and allocations through a
//! thread-local [`LocalBuffer`] to amortize synchronization with the shared
//! state.

use core::cell::Cell;
use core::fmt::Debug;
use core::iter::FusedIterator;
use core::mem::MaybeUninit;
use core::num::NonZeroU32;
use core::sync::atomic::Ordering;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
use core::{ptr, slice};
use std::sync::Arc;

use zlim_utils::sync::Backoff;
use zlim_utils::vec::ArrayVec;

use super::EntityId;

// -----------------------------------------------------------------------------
// FreshAllocator
// -----------------------------------------------------------------------------

/// Allocator for new, never-before-used entity IDs.
#[repr(transparent)]
struct FreshAllocator {
    next: AtomicU32,
}

impl FreshAllocator {
    /// Panic handler for overflow conditions.
    #[cold]
    #[inline(never)]
    fn on_overflow() -> ! {
        panic!("too many entities")
    }

    /// Creates a new [`FreshAllocator`] starting from ID `1`.
    ///
    /// range: `1 <= index < u32::MAX`.
    #[inline(always)]
    const fn new() -> FreshAllocator {
        FreshAllocator {
            // Start from `1` instead of `0`.
            // A vacant slot is acceptable.
            next: AtomicU32::new(1),
        }
    }

    /// Allocates a single new entity ID.
    fn alloc(&self) -> EntityId {
        use Ordering::Relaxed;

        // if `next` is `u32::MAX`, `try_update` return Err. So we won't get placeholder.
        match self.next.try_update(Relaxed, Relaxed, |v| v.checked_add(1)) {
            Ok(index) => EntityId {
                index,
                generation: NonZeroU32::MIN,
            },
            Err(_) => Self::on_overflow(),
        }
    }

    /// Allocates multiple new entity IDs.
    fn alloc_many(&self, count: u32) -> FreshEntityIter {
        use Ordering::Relaxed;

        match self
            .next
            .try_update(Relaxed, Relaxed, |v| v.checked_add(count))
        {
            Ok(index) => FreshEntityIter(index..(index + count)),
            Err(_) => Self::on_overflow(),
        }
    }
}

// -----------------------------------------------------------------------------
// FreshEntityIter
// -----------------------------------------------------------------------------

/// Iterator over freshly allocated entity IDs.
#[repr(transparent)]
struct FreshEntityIter(core::ops::Range<u32>);

impl Iterator for FreshEntityIter {
    type Item = EntityId;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|index| EntityId {
            index,
            generation: NonZeroU32::MIN,
        })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl ExactSizeIterator for FreshEntityIter {
    // Manual implementation may reduce indirect calls.
    #[inline]
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl FusedIterator for FreshEntityIter {}

// -----------------------------------------------------------------------------
// Chunk
// -----------------------------------------------------------------------------

/// A lazily-allocated, fixed-capacity block of [`EntityId`] slots.
///
/// `head` is null until the chunk is first written, then points at a leaked
/// `Box<[EntityId]>`; its length is tracked externally by the owning
/// [`FreeBuffer`]. Access is not internally synchronized — callers serialize
/// writes against reads via the [`FreeCount`] disable flag (see the module
/// docs), which is why the `Send`/`Sync` impls below are sound.
#[repr(transparent)]
struct Chunk {
    head: Cell<*mut EntityId>,
}

// SAFETY: a `Chunk`'s `head` is only mutated by the single owning thread while
// the `FreeCount` disable flag excludes concurrent readers; all other accesses
// are reads of an already-published, immutable region. See the module docs.
unsafe impl Sync for Chunk {}
unsafe impl Send for Chunk {}

impl Chunk {
    /// An empty [`Chunk`] with a null pointer.
    #[expect(clippy::declare_interior_mutable_const, reason = "initial value")]
    const UNINIT: Chunk = Self {
        head: Cell::new(ptr::null_mut()),
    };

    /// Allocates memory for the chunk.
    ///
    /// # Safety
    /// - Should only be called when the chunk is uninitialized.
    /// - The caller must ensure concurrency safety.
    #[cold]
    #[inline(never)]
    unsafe fn alloc(&self, capacity: u32) -> *mut EntityId {
        let len = capacity as usize;
        // Using `new_uninit` is faster than `new_zeroed` for uninitialized allocation.
        let mut boxed: Box<[MaybeUninit<EntityId>]> = Box::new_uninit_slice(len);

        // Compile-time assertion: `EntityId::PLACEHOLDER` has all bits set to 1,
        // so a 0xFF memset is a valid way to fill every slot with it.
        const {
            assert!(EntityId::PLACEHOLDER.to_bits() == u64::MAX);
        }

        // SAFETY: `boxed` owns `len` contiguous `MaybeUninit<EntityId>` slots, so
        // writing `len` elements' worth of bytes stays in bounds. `count` is the
        // element count, not a byte count (see `write_bytes` docs).
        unsafe {
            // Initialize every slot with the placeholder value;
            // equivalent to a memset with 0xFF.
            boxed.as_mut_ptr().write_bytes(u8::MAX, len);
        }

        let ptr = Box::leak(boxed).as_mut_ptr() as *mut EntityId;

        self.head.set(ptr);
        ptr
    }

    /// Deallocates memory for the chunk.
    ///
    /// # Safety
    /// - The chunk must have been previously allocated.
    /// - The caller must ensure concurrency safety.
    /// - `capacity` must match the value used during allocation.
    unsafe fn dealloc(&self, capacity: u32) {
        let data = self.head.get();

        if !data.is_null() {
            let len = capacity as usize;
            let slice = ptr::slice_from_raw_parts_mut(data, len);

            unsafe {
                ::core::mem::drop(Box::from_raw(slice));
            }
        }
    }

    /// Retrieves an entity at the specified index within this chunk.
    ///
    /// # Safety
    /// - The caller must ensure concurrency safety.
    /// - The index must be within the chunk's capacity.
    /// - The chunk must be initialized (head is not null).
    #[inline]
    unsafe fn get(&self, index: u32) -> EntityId {
        let head = self.head.get();

        unsafe { *head.add(index as usize) }
    }

    /// Retrieves a slice of entities starting at the specified index.
    ///
    /// # Safety
    /// - The caller must ensure concurrency safety.
    /// - The index and required length must be within bounds.
    /// - The chunk must be initialized (head is not null).
    #[inline]
    unsafe fn get_slice(&self, index: u32, required_len: u32, chunk_capacity: u32) -> &[EntityId] {
        let available_len = (chunk_capacity - index) as usize;
        let len = available_len.min(required_len as usize);

        let head = self.head.get();
        unsafe { slice::from_raw_parts(head.add(index as usize), len) }
    }

    /// Stores a slice of entities starting at the specified index.
    ///
    /// # Safety
    /// - The caller must ensure concurrency safety.
    /// - The index must be within the chunk's capacity.
    /// - The chunk will be allocated if not already initialized.
    ///
    /// # Returns
    /// The number of entities successfully stored.
    #[inline]
    unsafe fn set_slice(&self, index: u32, entities: &[EntityId], chunk_capacity: u32) -> usize {
        let available_len = (chunk_capacity - index) as usize;
        let len = available_len.min(entities.len());

        let mut head = self.head.get();

        if head.is_null() {
            unsafe { head = self.alloc(chunk_capacity) };
        }

        unsafe {
            let target = head.add(index as usize);
            ptr::copy_nonoverlapping(entities.as_ptr(), target, len);
        }

        len
    }
}

// -----------------------------------------------------------------------------
// FreeBuffer
// -----------------------------------------------------------------------------

/// The number of chunks.
const NUM_CHUNKS: u32 = 24;

/// 8_u32: Minimum capacity bit masker.
const NUM_SKIPPED: u32 = u32::BITS - NUM_CHUNKS;

/// A buffer composed of chunks with exponentially increasing capacities.
///
/// Chunk capacities follow the pattern: `[512, 512, 1024, 2048, 4096, ...]`
#[repr(transparent)]
struct FreeBuffer([Chunk; NUM_CHUNKS as usize]);

impl FreeBuffer {
    /// Returns the capacity of the chunk at the specified index.
    ///
    /// Capacities: 512, 512, 1024, 2048, 4096, ...
    #[inline]
    const fn chunck_capacity(chunk_index: u32) -> u32 {
        if chunk_index == 0 {
            const { 1_u32 << (NUM_SKIPPED + 1) }
        } else {
            1_u32 << (NUM_SKIPPED + chunk_index)
        }
    }

    /// Locates the chunk containing the specified global index.
    ///
    /// # Returns
    /// - Reference to the chunk
    /// - Index within that chunk
    /// - Capacity of that chunk
    #[inline]
    fn chunk_with_index(&self, full_index: u32) -> (&Chunk, u32, u32) {
        // Optimization: Determine chunk index based on the position of the highest set bit.
        // For example, the chunk index of `0b1000...000` is `23` (the last chunk).
        let chunk_index = (NUM_CHUNKS - 1).saturating_sub(full_index.leading_zeros());
        // SAFETY: 0 <= chunk_index <= 23, in bound.
        let chunk = unsafe { self.0.get_unchecked(chunk_index as usize) };
        let chunk_capacity = Self::chunck_capacity(chunk_index);
        // ↓ Eq to `full_index & (chunk_capacity - 1)`, but faster.
        let index_in_chunk = full_index & !chunk_capacity;
        (chunk, index_in_chunk, chunk_capacity)
    }

    /// Retrieves an entity at the specified global index.
    ///
    /// # Safety
    /// - The caller must ensure concurrency safety.
    /// - The index must be within the buffer's current logical length.
    unsafe fn get(&self, full_index: u32) -> EntityId {
        let (chunk, index, _) = self.chunk_with_index(full_index);
        // SAFETY: Ensured by caller.
        unsafe { chunk.get(index) }
    }

    /// Stores a slice of entities starting at the specified global index.
    ///
    /// # Safety
    /// - The caller must ensure concurrency safety.
    /// - The operation may span multiple chunks if the slice crosses chunk boundaries.
    unsafe fn set_slice(&self, mut full_index: u32, mut entities: &[EntityId]) {
        while !entities.is_empty() {
            let (chunk, index, capacity) = self.chunk_with_index(full_index);
            // SAFETY: If the chunk is uninit, the `set_slice` fn will initialize it.
            unsafe {
                let len = chunk.set_slice(index, entities, capacity);
                full_index += len as u32;
                // ↓ optimize boundary detection
                core::hint::assert_unchecked(len <= entities.len());
                entities = &entities[len..];
            }
        }
    }
}

impl Drop for FreeBuffer {
    fn drop(&mut self) {
        for index in 0..NUM_CHUNKS {
            let capacity = Self::chunck_capacity(index);
            // SAFETY: the index is in bound.
            let chunk = unsafe { self.0.get_unchecked(index as usize) };
            // SAFETY: Exclusive access and correct capacity.
            unsafe { chunk.dealloc(capacity) };
        }
    }
}

// -----------------------------------------------------------------------------
// FreeBufferIter
// -----------------------------------------------------------------------------

/// Iterator over entities in a [`FreeBuffer`].
struct FreeBufferIter<'a> {
    buffer: &'a FreeBuffer,
    current_iter: slice::Iter<'a, EntityId>,
    remaining_indices: core::ops::Range<u32>,
}

impl Iterator for FreeBufferIter<'_> {
    type Item = EntityId;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        #[inline(never)]
        fn slow_next(this: &mut FreeBufferIter<'_>, required: u32) -> Option<EntityId> {
            let next_index = this.remaining_indices.start;
            let (chunk, index, capacity) = this.buffer.chunk_with_index(next_index);

            let slice = unsafe { chunk.get_slice(index, required, capacity) };
            this.remaining_indices.start = next_index + slice.len() as u32;
            this.current_iter = slice.iter();

            // `required > 0`, so the slice should not be empty.
            debug_assert!(!slice.is_empty());

            // SAFETY: The new chunk is always not empty.
            unsafe { Some(*this.current_iter.next().unwrap_unchecked()) }
        }

        // First, try to get an entity from the current chunk slice
        if let Some(&entity) = self.current_iter.next() {
            return Some(entity);
        }

        // optimize branch prediction
        core::hint::cold_path();

        // If current slice is exhausted, fetch the next chunk
        let still_need = self.remaining_indices.len() as u32;

        if still_need == 0 {
            None
        } else {
            slow_next(self, still_need)
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.current_iter.len() + self.remaining_indices.len();
        (len, Some(len))
    }
}

impl<'a> ExactSizeIterator for FreeBufferIter<'a> {
    // Manual implementation may reduce indirect calls.
    #[inline]
    fn len(&self) -> usize {
        self.current_iter.len() + self.remaining_indices.len()
    }
}

impl<'a> FusedIterator for FreeBufferIter<'a> {}

impl FreeBuffer {
    /// Creates an iterator over a range of indices in the buffer.
    ///
    /// # Safety
    /// - The caller must ensure concurrency safety.
    /// - All indices in the range must have been previously initialized via [`Self::set_slice`].
    unsafe fn iter(&self, mut indices: core::ops::Range<u32>) -> FreeBufferIter<'_> {
        // An empty range maps to no initialized chunk: the first chunk may
        // still be null, and `get_slice` would form a slice from a null
        // pointer (UB even for a zero length). Bail out with an empty iterator.
        if indices.is_empty() {
            return FreeBufferIter {
                buffer: self,
                current_iter: [].iter(),
                remaining_indices: indices,
            };
        }

        // Eagerly load the first chunk so the common `next` path stays hot.
        let next_index = indices.start;
        let required = indices.len() as u32;
        let (chunk, index, capacity) = self.chunk_with_index(next_index);

        // SAFETY: `indices` is non-empty and bounded by the prior logical
        // length (caller contract), so `next_index` lies in an initialized
        // chunk and the resulting slice is non-empty.
        let slice = unsafe { chunk.get_slice(index, required, capacity) };
        indices.start = next_index + slice.len() as u32;
        debug_assert!(!slice.is_empty());

        FreeBufferIter {
            buffer: self,
            current_iter: slice.iter(),
            remaining_indices: indices,
        }
    }
}

// -----------------------------------------------------------------------------
// FreeCount
// -----------------------------------------------------------------------------

/// Packed state representation for [`FreeCount`].
///
/// Three fields share a single `u64`:
///
/// | bits   | field      | notes                                              |
/// |--------|------------|----------------------------------------------------|
/// | 0..=32 | length     | stored with a `+2^32` bias ([`LENGTH_0`]), 33 bits |
/// | 33     | disable    | when set, remote allocation is blocked             |
/// | 34..=63| generation | wrapping ABA counter for the remote CAS loop       |
///
/// The bias lets a pop underflow harmlessly: [`length`](Self::length) clamps
/// any value below the bias back to `0`.
///
/// [`LENGTH_0`]: Self::LENGTH_0
#[derive(Clone, Copy)]
#[repr(transparent)]
struct FreeCountState(u64);

impl FreeCountState {
    /// Bit position for the disable flag.
    /// When set, remote allocations are blocked.
    const DISABLING_BIT: u64 = 1 << 33;

    /// Bitmask for the length field (33 bits).
    const LENGTH_MASK: u64 = (1 << 32) | (u32::MAX as u64);

    /// Encoded value representing length = 0.
    const LENGTH_0: u64 = 1 << 32;

    /// Least significant bit of the 30-bit generation counter.
    const GENERATION_LEAST_BIT: u64 = 1 << 34;

    /// Extracts the logical length from the packed state.
    ///
    /// Removes the [`LENGTH_0`](Self::LENGTH_0) bias; a length field that has
    /// underflowed below the bias (e.g. after popping an empty list) clamps to
    /// `0` rather than reporting a bogus large value.
    #[inline]
    const fn length(self) -> u32 {
        let unsigned_length = self.0 & Self::LENGTH_MASK;
        unsigned_length.saturating_sub(Self::LENGTH_0) as u32
    }

    /// Checks if the disable flag is set.
    #[inline]
    const fn is_disabled(self) -> bool {
        (self.0 & Self::DISABLING_BIT) > 0
    }

    /// Creates a new state with only the length changed.
    #[inline]
    const fn with_length(self, length: u32) -> Self {
        // Encode length with the "zero offset" bit set
        let length = length as u64 | Self::LENGTH_0;
        Self(self.0 & !Self::LENGTH_MASK | length)
    }

    /// Encodes a "pop" of `num` elements as a single value to subtract.
    ///
    /// Subtracting this from the state lowers the length by `num` and, via the
    /// borrow out of the generation's least bit, advances the generation
    /// counter. The counter's direction is irrelevant — the remote CAS loop
    /// only needs it to change so it can detect concurrent pops (ABA).
    #[inline]
    const fn encode_generation(num: u32) -> u64 {
        let subtract_length = num as u64;
        subtract_length | Self::GENERATION_LEAST_BIT
    }

    /// Applies a pop operation to the state.
    #[inline]
    const fn pop(self, num: u32) -> Self {
        Self(self.0.wrapping_sub(Self::encode_generation(num)))
    }
}

/// Atomic interface for [`FreeCountState`].
#[repr(transparent)]
struct FreeCount(AtomicU64);

impl FreeCount {
    /// Loads the current state with [`Acquire`](Ordering::Acquire) ordering.
    #[inline]
    fn acquire_state(&self) -> FreeCountState {
        FreeCountState(self.0.load(Ordering::Acquire))
    }

    /// Atomically subtracts `num` from the length, returning the previous state.
    ///
    /// # Note
    /// The caller must ensure that:
    /// - Changing the state is permitted (not disabled)
    /// - Sufficient elements exist to pop
    #[inline]
    fn pop_for_state(&self, num: u32) -> FreeCountState {
        let to_sub = FreeCountState::encode_generation(num);
        let raw = self.0.fetch_sub(to_sub, Ordering::Acquire);
        FreeCountState(raw)
    }

    /// Sets the disable flag, returning the previous state.
    ///
    /// Note that the returned state is old count (usually not disabled).
    #[inline]
    fn disable_for_state(&self) -> FreeCountState {
        // Generation change is irrelevant here since we're modifying the value anyway
        FreeCountState(
            self.0
                .fetch_or(FreeCountState::DISABLING_BIT, Ordering::Acquire),
        )
    }

    /// Stores a new state value.
    ///
    /// # Safety
    /// This is "risky" because it doesn't verify that the state hasn't changed
    /// since it was read. Incorrect use may cause entities to be skipped or
    /// allocated multiple times.
    #[inline]
    fn set_state_risky(&self, state: FreeCountState) {
        self.0.store(state.0, Ordering::Release);
    }

    /// Attempts to update the state atomically using compare-and-swap.
    #[inline]
    fn try_set_state(
        &self,
        expected_current_state: FreeCountState,
        target_state: FreeCountState,
    ) -> Result<(), FreeCountState> {
        match self.0.compare_exchange(
            expected_current_state.0,
            target_state.0,
            Ordering::Release,
            Ordering::Acquire,
        ) {
            Ok(_) => Ok(()),
            Err(val) => Err(FreeCountState(val)),
        }
    }
}

// -----------------------------------------------------------------------------
// FreeList
// -----------------------------------------------------------------------------

/// Thread-safe collection of recycled entities.
///
/// Similar to a `Vec<EntityId>`, but optimized for concurrent access and
/// remote allocation scenarios.
struct FreeList {
    /// Storage buffer for entities.
    buffer: FreeBuffer,
    /// Atomic state tracking length, disable flag, and generation.
    len: FreeCount,
}

impl FreeList {
    /// Creates an empty [`FreeList`].
    #[inline]
    const fn new() -> Self {
        Self {
            buffer: FreeBuffer([Chunk::UNINIT; NUM_CHUNKS as usize]),
            len: FreeCount(AtomicU64::new(FreeCountState::LENGTH_0)),
        }
    }

    /// Adds entities to the free list for reuse.
    ///
    /// # Safety
    /// - The caller must ensure exclusive access or proper synchronization.
    unsafe fn free(&self, entities: &[EntityId]) {
        // Block remote allocations during this operation
        let state = self.len.disable_for_state();
        // Append entities to the buffer
        let full_index = state.length();
        unsafe {
            self.buffer.set_slice(full_index, entities);
        }

        // Update length and re-enable allocations
        let length = full_index + entities.len() as u32;

        // This state is the *old* state, returned by `fetch_or`,
        // which is not disabled. So the `set_state_risky` function
        // implicit reset the `disable` flag.
        let new_state = state.with_length(length);
        self.len.set_state_risky(new_state);
    }

    /// Allocates a single entity from the free list.
    #[inline]
    fn alloc(&self) -> Option<EntityId> {
        let len = self.len.pop_for_state(1).length();
        let index = len.checked_sub(1)?;

        Some(unsafe { self.buffer.get(index) })
    }

    /// Allocates multiple entities from the free list.
    #[inline]
    fn alloc_many(&self, count: u32) -> FreeBufferIter<'_> {
        let len = self.len.pop_for_state(count).length();
        let index = len.saturating_sub(count);

        unsafe { self.buffer.iter(index..len) }
    }

    /// Allocates an entity safely from a remote context.
    /// Uses compare-and-swap loops to handle concurrent modifications.
    fn remote_alloc(&self) -> Option<EntityId> {
        let backoff = Backoff::new();
        let mut state = self.len.acquire_state();

        loop {
            // Wait if free operations are in progress
            if state.is_disabled() {
                backoff.snooze();
                state = self.len.acquire_state();
                continue;
            }

            let len = state.length();
            let index = len.checked_sub(1)?;

            // Read the entity before attempting to claim it
            let entity = unsafe { self.buffer.get(index) };
            let new_state = state.pop(1);

            // Attempt to atomically claim this entity
            match self.len.try_set_state(state, new_state) {
                Ok(_) => return Some(entity),
                Err(actual) => state = actual, // Retry with updated state
            }
        }
    }
}

// -----------------------------------------------------------------------------
// AllocEntitiesIter
// -----------------------------------------------------------------------------

/// Iterator that yields entities from both recycled and fresh sources.
pub struct AllocEntitiesIter<'a> {
    fresh: FreshEntityIter,
    reused: FreeBufferIter<'a>,
}

impl Iterator for AllocEntitiesIter<'_> {
    type Item = EntityId;

    fn next(&mut self) -> Option<Self::Item> {
        // Prioritize recycled entities before allocating new ones
        self.reused.next().or_else(|| self.fresh.next())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.reused.len() + self.fresh.len();
        (len, Some(len))
    }
}

impl ExactSizeIterator for AllocEntitiesIter<'_> {}
impl FusedIterator for AllocEntitiesIter<'_> {}

impl Drop for AllocEntitiesIter<'_> {
    fn drop(&mut self) {
        let leaking = self.reused.len() + self.fresh.len();

        if leaking > 0 {
            core::hint::cold_path();
            log::warn!("{leaking} entities being leaked via unfinished `AllocEntitiesIter`");
        }
    }
}

// -----------------------------------------------------------------------------
// SharedAllocator
// -----------------------------------------------------------------------------

/// Shared state between [`EntityAllocator`] and [`RemoteAllocator`].
/// Provides thread-safe entity allocation with support for both
/// in-world and remote allocation scenarios.
struct SharedAllocator {
    /// Recycled entities available for reuse
    free: FreeList,
    /// Allocator for new entity IDs
    fresh: FreshAllocator,
    /// Flag indicating whether the allocator has been closed
    is_closed: AtomicBool,
}

// -----------------------------------------------------------------------------
// RemoteAllocator
// -----------------------------------------------------------------------------

/// Entity allocator that can operate without direct `World` access.
///
/// Useful for asynchronous operations, background tasks, or any scenario
/// where entity allocation is needed but holding a `World` reference is
/// impractical or impossible.
///
/// # Safety Considerations
/// - Entities allocated remotely may become invalid if the source `World`
///   is destroyed before they are used.
/// - Always verify allocation validity using [`RemoteAllocator::is_closed`]
///   or [`EntityAllocator::is_connected_to`] before using remotely allocated entities.
#[derive(Clone)]
pub struct RemoteAllocator {
    shared: Arc<SharedAllocator>,
}

impl RemoteAllocator {
    /// Checks whether the allocator has been closed.
    ///
    /// The allocator is closed when the parent [`EntityAllocator`] is dropped,
    /// which typically indicates that the `World` has been destroyed.
    ///
    /// # Returns
    /// `true` if the allocator is closed and allocations should not be used.
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.shared.is_closed.load(Ordering::Acquire)
    }

    /// Determines if this [`RemoteAllocator`] is connected to the same
    /// `World` as the provided [`EntityAllocator`].
    #[inline]
    pub fn is_connected_to(&self, source: &EntityAllocator) -> bool {
        Arc::ptr_eq(&self.shared, &source.shared)
    }

    /// Allocates a single entity.
    ///
    /// Attempts to reuse a recycled entity first, falling back to
    /// allocating a new entity if none are available.
    pub fn alloc(&self) -> EntityId {
        self.shared
            .free
            .remote_alloc()
            .unwrap_or_else(|| self.shared.fresh.alloc())
    }
}

impl Debug for RemoteAllocator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("RemoteAllocator")
            .field(&Arc::as_ptr(&self.shared))
            .finish()
    }
}

// -----------------------------------------------------------------------------
// EntityAllocator
// -----------------------------------------------------------------------------

const LOCAL_SIZE: usize = 127;

/// Primary entity allocator bound to a `World` instance.
///
/// Manages both allocation of new entities and recycling of destroyed entities.
/// This is an internal type; entity allocation is automatically handled by
/// `World` when creating entities.
///
/// # Important Notes
/// - Entities are specific to their creating `World` and cannot be used
///   across different `World` instances.
/// - The allocator does not modify an entity's [`EntityVersion`] during
///   allocation or recycling. Callers must advance the version themselves when
///   reusing an id, to prevent a recycled id from aliasing an older handle.
pub struct EntityAllocator {
    /// See [`SharedAllocator`] for details.
    shared: Arc<SharedAllocator>,
    free_buf: Box<ArrayVec<EntityId, LOCAL_SIZE>>,
    alloc_buf: Box<ArrayVec<EntityId, LOCAL_SIZE>>,
}

impl Debug for EntityAllocator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("EntityAllocator")
            .field(&Arc::as_ptr(&self.shared))
            .finish()
    }
}

impl Drop for EntityAllocator {
    fn drop(&mut self) {
        // Signal to remote allocators that this allocator is no longer valid
        self.shared.is_closed.store(true, Ordering::Release);
    }
}

impl Default for EntityAllocator {
    /// Creates a new [`EntityAllocator`].
    fn default() -> Self {
        Self {
            shared: Arc::new(SharedAllocator {
                free: FreeList::new(),
                fresh: FreshAllocator::new(),
                is_closed: AtomicBool::new(false),
            }),
            free_buf: Box::new(ArrayVec::new()),
            alloc_buf: Box::new(ArrayVec::new()),
        }
    }
}

impl EntityAllocator {
    /// Creates a [`RemoteAllocator`] from this allocator.
    ///
    /// The remote allocator can be used to allocate entities without
    /// requiring direct access to the `World`.
    #[inline]
    pub fn remote(&self) -> RemoteAllocator {
        RemoteAllocator {
            shared: self.shared.clone(),
        }
    }

    /// Checks if a [`RemoteAllocator`] is connected to this allocator.
    #[inline]
    pub fn is_connected_to(&self, remote: &RemoteAllocator) -> bool {
        Arc::ptr_eq(&self.shared, &remote.shared)
    }

    /// Recycles a single entity for future reuse.
    ///
    /// Note: Entities may be stored in a local buffer and not immediately
    /// made available for allocation until the buffer is flushed.
    #[inline(always)]
    pub fn free(&mut self, entity: EntityId) {
        #[inline(never)]
        fn slow_path(this: &mut EntityAllocator) {
            // SAFETY: We have exclusive access (&mut self)
            unsafe {
                let entities = this.free_buf.as_slice();
                this.shared.free.free(entities);
                this.free_buf.set_len(0);
            }
        }

        // Flush local buffer if full
        if self.free_buf.is_full() {
            core::hint::cold_path();
            slow_path(self);
        }

        // Add entity to local buffer
        unsafe {
            self.free_buf.push_unchecked(entity);
        }
    }

    /// Recycles multiple entities for future reuse.
    ///
    /// More efficient than individual [`free`](Self::free) calls for batches.
    pub fn free_many(&mut self, entities: &[EntityId]) {
        // SAFETY: We have exclusive access (&mut self)
        unsafe {
            self.shared.free.free(entities);
        }
    }

    /// Allocates a single entity, preferring recycled entities.
    ///
    /// Note: does not modify the entity's `Generation`. Callers must
    /// advance the generation when reusing an id, to prevent aliasing.
    #[must_use]
    pub fn alloc(&self) -> EntityId {
        self.shared
            .free
            .alloc()
            .unwrap_or_else(|| self.shared.fresh.alloc())
    }

    /// Efficiently allocates multiple entities.
    ///
    /// Returns an iterator that must be fully consumed; otherwise,
    /// any remaining entities will be leaked (not available for reuse).
    #[must_use]
    pub fn alloc_many(&self, count: u32) -> AllocEntitiesIter<'_> {
        // SAFETY: Caller ensures exclusive access or proper synchronization
        let reused = self.shared.free.alloc_many(count);
        let still_need = count - reused.len() as u32;
        let fresh = self.shared.fresh.alloc_many(still_need);
        AllocEntitiesIter { fresh, reused }
    }

    /// Allocates a single entity with mutable access, checking local buffer first.
    ///
    /// More efficient than [`alloc`](Self::alloc) when mutable access is available.
    #[must_use]
    #[inline(always)]
    pub fn alloc_mut(&mut self) -> EntityId {
        #[inline(never)]
        fn slow_path(this: &mut EntityAllocator) -> EntityId {
            let count = LOCAL_SIZE as u32 + 1;
            let mut reused = this.shared.free.alloc_many(count);
            let still_need = count - reused.len() as u32;
            let mut fresh = this.shared.fresh.alloc_many(still_need);
            let ret = reused.next().or_else(|| fresh.next());

            let buf = &mut this.alloc_buf;

            unsafe {
                reused.for_each(|v| buf.push_unchecked(v));
                fresh.for_each(|v| buf.push_unchecked(v));
            }

            debug_assert!(ret.is_some());
            unsafe { ret.unwrap_unchecked() }
        }

        if let Some(entity) = self.alloc_buf.pop() {
            entity
        } else {
            ::core::hint::cold_path();
            slow_path(self)
        }
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{EntityAllocator, FreeBuffer, RemoteAllocator};

    #[test]
    fn send_and_sync() {
        fn is_send_sync<T: Send + Sync>() {}

        is_send_sync::<EntityAllocator>();
        is_send_sync::<RemoteAllocator>();
    }

    #[test]
    fn chunck_capacity() {
        assert!(FreeBuffer::chunck_capacity(0) == 512);
        assert!(FreeBuffer::chunck_capacity(1) == 512);
        assert!(FreeBuffer::chunck_capacity(2) == 1024);
        assert!(FreeBuffer::chunck_capacity(3) == 2048);
    }

    #[test]
    fn uniqueness() {
        let mut entities = Vec::with_capacity(2000);
        let mut allocator = EntityAllocator::default();

        entities.extend(allocator.alloc_many(1000));

        let pre_len = entities.len();
        entities.sort();
        entities.dedup();
        assert_eq!(pre_len, entities.len(), "fail 1");

        entities.drain(500..).for_each(|e| allocator.free(e));
        allocator.free_many(&entities);
        entities.clear();

        entities.extend(allocator.alloc_many(500));
        (0..500).for_each(|_| entities.push(allocator.alloc()));
        (0..500).for_each(|_| entities.push(allocator.alloc_mut()));
        entities.extend(allocator.alloc_many(500));

        let pre_len = entities.len();
        entities.sort();
        entities.dedup();
        assert_eq!(pre_len, entities.len(), "fail 2");
    }

    #[test]
    fn recyclable() {
        let mut entities = Vec::with_capacity(550);
        let mut allocator = EntityAllocator::default();

        for _ in 0..50 {
            (0..150).for_each(|_| entities.push(allocator.alloc()));
            (0..150).for_each(|_| entities.push(allocator.alloc_mut()));
            entities.extend(allocator.alloc_many(200));

            // We only allocated 500 units, but there is a buffer inside the allocator.
            // So the maximum entity index will exceed 500, but it shouldn't be much bigger.
            assert!(entities.iter().all(|t| t.index() < 1500));

            entities.drain(300..).for_each(|e| allocator.free(e));
            allocator.free_many(&entities);
            entities.clear();
        }
    }
}
