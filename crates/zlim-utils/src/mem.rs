//! Simple memory pool.
#![expect(unsafe_code, reason = "raw pointer is unsafe")]
#![expect(
    clippy::mut_from_ref,
    reason = "the data is copied, instead of original ref"
)]

use core::alloc::Layout;
use core::cell::Cell;
use core::fmt::Debug;
use core::panic::{RefUnwindSafe, UnwindSafe};
use core::ptr::{self, NonNull};
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering::{AcqRel, Acquire, Release};
use std::alloc as malloc;
use std::sync::{Mutex, PoisonError};

use crate::ext::CachePadded;

// -----------------------------------------------------------------------------
// Block

const ALIGN: usize = const {
    let align_u = align_of::<usize>();
    let align_a = align_of::<AtomicUsize>();
    if align_u < align_a { align_a } else { align_u }
};

const SIZE1: usize = const {
    let size_u = size_of::<usize>();
    let size_a = size_of::<AtomicUsize>().next_power_of_two();
    if size_u < size_a { size_a } else { size_u }
};

const SIZE2: usize = SIZE1 * 2;
const SIZE3: usize = SIZE1 * 3;

/// A memory block that acts as a page in the bump allocator.
///
/// ```text
/// ┌────────────────┬───────────────┬───────────────┬─────────────────┐
/// │ block_len      │ prev_ptr      │ span          │ user_data       │
/// │ (usize)        │ (usize)       │(usize)        │ (need bytes)    │
/// └────────────────┴───────────────┴───────────────┴─────────────────┘
/// │<─── SIZE1 ────>│<─── SIZE1 ───>│<─── SIZE1 ───>│<──── need ─────>│
/// |<─ 0B           |<─ SIZE1       |<- SIZE2       |<- SIZE3         |
/// │<─────────────────────── block_len (aligned) ────────────────────>│
/// ```
///
/// - `block_len` stores the size of the **entire** block.
/// - `prev_ptr` is a pointer that point to the previous block.
/// - `span` is a pointer, and the area starting from it is vacant.
#[derive(Clone, Copy)]
#[repr(transparent)]
struct Block {
    pointer: NonNull<usize>,
}

impl Block {
    /// Create a block from given params.
    ///
    /// Due to the storage of inline pointers, the actual
    /// allocated memory is slightly larger than the given value.
    ///
    /// if `ATOMIC` is `true`, the span info will be `AtomicUsize`.
    ///
    /// # Safety
    /// - `prev` must be a valid pointer to the previous block or null.
    /// - `need` must be less than or equal to `isize::MAX`.
    #[must_use]
    unsafe fn alloc<const ATOMIC: bool>(need: usize, prev: *mut usize) -> Self {
        // Why `SIZE3 + ALIGN - 1`?
        // Because in addition to `need` bytes of user data, we need:
        //   - 1 `usize` to store `block_len`
        //   - 1 `usize` to store `prev_ptr`
        //   - 1 `usize` to store `span`
        //   - At most `ALIGN - 1` bytes of padding
        // Since ALIGN == SIZE (on most platforms), this equals: need + 4 * SIZE - 1
        const PADDING: usize = SIZE3 + ALIGN - 1;
        // No need to use `saturating_add` here.
        let unaligned: usize = need + PADDING;
        // Round up to the nearest multiple of ALIGN.
        let size: usize = unaligned & const { !(ALIGN - 1) };
        let size: usize = size.next_power_of_two();

        // Cannot use `from_size_align_unchecked` because `size` may exceed isize::MAX.
        let layout = Layout::from_size_align(size, ALIGN).unwrap();

        let ptr = NonNull::new(unsafe { malloc::alloc(layout) })
            .unwrap_or_else(|| malloc::handle_alloc_error(layout))
            .cast::<usize>();

        // Write metadata at the beginning of the block.
        unsafe {
            // block_len
            ptr.write(size);
            // prev_ptr
            ptr.byte_add(SIZE1).write(prev as usize);
            // span points to the first free byte
            let span: NonNull<usize> = ptr.byte_add(SIZE2);
            // free points to the first available bit
            let free: NonNull<usize> = ptr.byte_add(SIZE3);
            if ATOMIC {
                let span: NonNull<AtomicUsize> = span.cast();
                span.write(AtomicUsize::new(free.as_ptr() as *mut u8 as usize));
            } else {
                // span
                span.write(free.as_ptr() as *mut u8 as usize);
            }
        }

        Self { pointer: ptr }
    }

    /// Dealloc this block and return the pointer of previous block.
    ///
    /// Return `null_ptr` if self is head block.
    ///
    /// # Safety
    /// - `self` must be a valid block that has not been deallocated yet.
    #[must_use = "Need to dealloc the previous block"]
    unsafe fn dealloc(self) -> *mut usize {
        let size = unsafe { self.pointer.read() };
        // Optional: `from_size_align_unchecked`
        let layout = Layout::from_size_align(size, ALIGN).unwrap();
        let prev = unsafe { self.pointer.byte_add(SIZE1).read() };

        unsafe {
            malloc::dealloc(self.pointer.as_ptr() as *mut u8, layout);
        }

        prev as *mut usize
    }

    /// Attempts to allocate `layout` bytes from this block's free space.
    ///
    /// # Safety
    /// - `self` must be a valid block that has not been deallocated yet.
    /// - `self` must be created by `Block::alloc::<false>`.
    unsafe fn try_insert(self, layout: Layout) -> Option<NonNull<u8>> {
        let head: usize = self.pointer.as_ptr() as usize;
        let size: usize = unsafe { self.pointer.read() };
        let tail: usize = head + size;
        let span_ptr: *mut usize = unsafe { self.pointer.as_ptr().byte_add(SIZE2) };
        let span: usize = unsafe { span_ptr.read() };

        // Align the current span to the layout's alignment requirement.
        let align_mask = layout.align() - 1;
        let aligned_span = (span + align_mask) & !align_mask;
        let new_span = aligned_span.saturating_add(layout.size());

        if new_span > tail {
            return None;
        }

        unsafe {
            span_ptr.write(new_span);
            Some(NonNull::new_unchecked(aligned_span as *mut u8))
        }
    }

    /// Attempts to allocate `layout` bytes from this block's free space.
    ///
    /// # Safety
    /// - `self` must be a valid block that has not been deallocated yet.
    /// - `self` must be created by `Block::alloc::<true>`.
    #[inline]
    unsafe fn try_insert_atomic(self, layout: Layout) -> Option<NonNull<u8>> {
        let head: usize = self.pointer.as_ptr() as usize;
        let size: usize = unsafe { self.pointer.read() };
        let tail: usize = head + size;

        let span_ptr: *mut usize = unsafe { self.pointer.as_ptr().byte_add(SIZE2) };
        let span_ptr: &AtomicUsize = unsafe { &*(span_ptr as *mut AtomicUsize) };

        let align_mask = layout.align() - 1;

        let mut span: usize = span_ptr.load(Acquire);

        loop {
            // Align the current span to the layout's alignment requirement.
            let aligned_span = (span + align_mask) & !align_mask;
            let new_span = aligned_span.saturating_add(layout.size());

            if new_span > tail {
                return None;
            }

            match span_ptr.compare_exchange(span, new_span, AcqRel, Acquire) {
                Ok(_) => unsafe {
                    return Some(NonNull::new_unchecked(aligned_span as *mut u8));
                },
                Err(modified) => {
                    span = modified;
                }
            }
        }
    }

    /// Creates a `Block` from a raw pointer.
    #[inline]
    fn from_raw(ptr: *mut usize) -> Option<Self> {
        NonNull::new(ptr).map(|p| Self { pointer: p })
    }
}

// -----------------------------------------------------------------------------
// PagePool

/// A bump-allocator pool that allocates memory in growing pages.
///
/// `PagePool` is a simple, append-only memory pool that allocates data
/// in pages. Each page is a contiguous block of memory that can hold
/// multiple allocations. When a page is full, a new page is allocated
/// and linked to the previous one.
///
/// Pages are not fixed-size. The pool tracks an internal size counter
/// (see [`base`](Self::base)), and each new page is sized to the
/// current counter. After allocating a page, the counter is multiplied
/// by 1.5 (`size += size / 2`), so pages grow geometrically.
///
/// # Drop Behavior
///
/// When the pool is dropped, all allocated pages are deallocated.
/// However, the pool does **not** call `drop` on the allocated data.
#[derive(Debug)]
struct PagePool {
    size: Cell<usize>,
    tail: Cell<*mut usize>,
}

impl Drop for PagePool {
    fn drop(&mut self) {
        let mut ptr = self.tail.get();
        while let Some(block) = Block::from_raw(ptr) {
            // SAFETY: Each block in the chain is valid (from_raw succeeded).
            // `Block::dealloc` reads the prev_ptr from the block before
            // deallocating its memory, then returns the prev_ptr so the
            // caller can continue the chain. The loop terminates when
            // prev_ptr is null (the head block). No block is freed twice
            // and no use-after-free occurs because prev_ptr is read before
            // the deallocation.
            unsafe {
                ptr = block.dealloc();
            }
        }
    }
}

impl PagePool {
    /// An empty pool with no pages allocated.
    ///
    /// The first page is sized to `size`, floored to 128 bytes.
    /// Each subsequent page grows by 1.5× over the previous one.
    const fn base(size: usize) -> Self {
        Self {
            size: Cell::new(if size < 128 { 128 } else { size }),
            tail: Cell::new(ptr::null_mut()),
        }
    }

    /// Allocates memory with the given layout and returns a pointer to it.
    ///
    /// The returned pointer is aligned according to the layout's alignment
    /// requirement. The memory is uninitialized and should be initialized
    /// by the caller.
    fn alloc(&self, layout: Layout) -> NonNull<u8> {
        let Some(block) = Block::from_raw(self.tail.get()) else {
            core::hint::cold_path();
            return self.alloc_slow(layout);
        };

        unsafe {
            block
                .try_insert(layout)
                .unwrap_or_else(|| self.alloc_slow(layout))
        }
    }

    /// Allocates a new page from the system allocator.
    ///
    /// This is the slow path that is called when:
    /// 1. The pool is empty (no pages allocated yet), or
    /// 2. The current page does not have enough free space.
    ///
    /// The page is sized to `max(current_size, need)`, rounded up to the
    /// next power of two and aligned to `usize`. After allocation, the
    /// internal size counter grows by 1.5× for the next page.
    #[inline(never)]
    fn alloc_slow(&self, layout: Layout) -> NonNull<u8> {
        let need = layout.size() + layout.align().max(ALIGN);
        let unaligned = self.size.get().max(need);

        // Ensure that page_size if aligned.
        const MASK: usize = ALIGN - 1;
        let page_size = (MASK + unaligned) & !MASK;

        unsafe {
            let prev = self.tail.get();
            let block = Block::alloc::<false>(page_size, prev);
            self.tail.set(block.pointer.as_ptr());
            self.size.update(|x| x + (x >> 1));

            block.try_insert(layout).expect("enough space")
        }
    }
}

// -----------------------------------------------------------------------------
// Bump

/// A bump allocator for temporary caches whose pages start small
/// and grow by 1.5× as needed.
///
/// # When to Use
///
/// - Building strings or buffers within a function scope
/// - Temporary data structures that are discarded after use
/// - Per-request caching in web servers
/// - Any scenario where the pool is created and dropped frequently
///
/// # Drop Behavior
///
/// When the pool is dropped, all allocated pages are deallocated.
/// However, the pool does **not** call `drop` on the allocated data.
///
/// Use [`alloc_value`] / [`alloc_slice`] / [`alloc_str`] for `Copy` types
/// (they are safe). Use [`alloc_unchecked`] for non-`Copy` types — in that
/// case the caller is responsible for running destructors before the pool
/// is destroyed.
///
/// [`alloc_value`]: Self::alloc_value
/// [`alloc_slice`]: Self::alloc_slice
/// [`alloc_str`]: Self::alloc_str
/// [`alloc_unchecked`]: Self::alloc_unchecked
///
/// # Example
///
/// ```rust
/// use zlim_utils::mem::Bump;
///
/// fn process_data() {
///     let cache = Bump::new(1000);
///
///     // Allocate temporary data
///     let temp_string = cache.alloc_str("Processing...");
///     let numbers = cache.alloc_slice(&[1, 2, 3, 4, 5]);
///
///     // ... do work ...
///
///     // When the function returns, cache is dropped and memory is freed
/// }
/// ```
#[derive(Debug)]
#[repr(transparent)]
pub struct Bump(PagePool);

unsafe impl Send for Bump {}

impl UnwindSafe for Bump {}
impl RefUnwindSafe for Bump {}

impl Default for Bump {
    /// Creates an empty pool without any pages allocated; the first
    /// page will be 1000 bytes and grow by 1.5× from there.
    #[inline]
    fn default() -> Self {
        Self(PagePool::base(960))
    }
}

impl Bump {
    /// Creates an empty pool without any pages allocated.
    ///
    /// `base_size` is the size of the first page, floored to 256 bytes.
    /// Later pages grow by 1.5× from there.
    #[inline]
    pub const fn new(base_size: usize) -> Self {
        Self(PagePool::base(base_size))
    }

    /// Allocates memory with the given layout and returns a pointer to it.
    ///
    /// The returned pointer is aligned according to the layout's alignment
    /// requirement. The memory is uninitialized and should be initialized
    /// by the caller.
    ///
    /// # Panics
    ///
    /// This method may panic if the system allocator fails to allocate memory.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::mem::Bump;
    /// use core::alloc::Layout;
    ///
    /// let pool = Bump::new(1000);
    /// let layout = Layout::new::<i32>();
    /// let ptr = pool.alloc(layout);
    ///
    /// unsafe {
    ///     ptr.cast::<i32>().as_ptr().write(42);
    /// }
    /// ```
    pub fn alloc(&self, layout: Layout) -> NonNull<u8> {
        self.0.alloc(layout)
    }

    /// Allocates a string slice by copying its contents into the pool.
    ///
    /// Returns a reference to the copied string. The input must be valid UTF-8.
    ///
    /// # Panics
    ///
    /// This method may panic if the system allocator fails to allocate memory.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::mem::Bump;
    ///
    /// let pool = Bump::new(1000);
    /// let s = pool.alloc_str("Hello, world!");
    /// assert_eq!(s, "Hello, world!");
    /// assert_ne!(s.as_ptr(), "Hello, world!".as_ptr());
    /// ```
    pub fn alloc_str<'a>(&'a self, s: &str) -> &'a str {
        let bytes = self.alloc_slice(s.as_bytes());

        unsafe {
            // SAFETY: The input is valid UTF-8, and we're copying it verbatim
            core::str::from_utf8_unchecked(bytes)
        }
    }

    /// Allocates a value of type `T` in the pool and returns a mutable reference.
    ///
    /// The value is moved into the pool's memory. The returned reference is valid
    /// until the pool is cleared or destroyed.
    ///
    /// This is safe because `T` implements `Copy` and does not require `Drop`.
    ///
    /// # Panics
    ///
    /// This method may panic if the system allocator fails to allocate memory.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::mem::Bump;
    ///
    /// let pool = Bump::new(1000);
    /// let v1 = pool.alloc_value(123);
    /// let v2 = pool.alloc_value([1, 2, 3, 4]);
    ///
    /// assert_eq!(*v1, 123);
    /// assert_eq!(*v2, [1, 2, 3, 4]);
    /// ```
    pub fn alloc_value<T: Copy>(&self, v: T) -> &mut T {
        let layout = Layout::new::<T>();
        let ptr = self.alloc(layout).cast::<T>();

        unsafe {
            ptr::write(ptr.as_ptr(), v);
            &mut *ptr.as_ptr()
        }
    }

    /// Allocates a slice by copying its contents into the pool.
    ///
    /// Returns a mutable reference to the copied slice. The slice elements
    /// must be `Copy`.
    ///
    /// # Panics
    ///
    /// This method may panic if the system allocator fails to allocate memory.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::mem::Bump;
    ///
    /// let pool = Bump::new(1000);
    /// let original = [1, 2, 3, 4, 5];
    /// let slice = pool.alloc_slice(&original);
    ///
    /// assert_eq!(*slice, original);
    /// assert_ne!(slice.as_ptr(), original.as_ptr());
    /// ```
    pub fn alloc_slice<'a, T: Copy>(&'a self, s: &[T]) -> &'a mut [T] {
        let layout = Layout::for_value(s);
        let ptr = self.alloc(layout).cast::<T>();

        unsafe {
            // Copy the slice contents
            ptr::copy_nonoverlapping(s.as_ptr(), ptr.as_ptr(), s.len());
            core::slice::from_raw_parts_mut(ptr.as_ptr(), s.len())
        }
    }

    /// Allocates a value of type `T` without requiring `Copy`.
    ///
    /// Unlike [`alloc_value`], this method accepts any `T`. The value
    /// is moved into pool-owned memory and never dropped by the pool.
    ///
    /// # Safety
    ///
    /// If `T` implements [`Drop`], the caller **must** manually run the
    /// destructor before the pool is destroyed. The pool itself will never
    /// call [`drop`] on the allocated value.
    ///
    /// # Panics
    ///
    /// This method may panic if the system allocator fails to allocate
    /// memory.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::mem::Bump;
    ///
    /// let pool = Bump::new(1000);
    ///
    /// // i32 is fine — no Drop, no problem.
    /// let v = unsafe { pool.alloc_unchecked(42i32) };
    /// assert_eq!(*v, 42);
    ///
    /// // For a type with Drop, the caller must invoke the destructor:
    /// // let s = unsafe { pool.alloc_unchecked(String::from("hi")) };
    /// // unsafe { core::ptr::drop_in_place(s); }
    /// ```
    ///
    /// ['drop`]: Drop::drop
    /// [`alloc_value`]: Self::alloc_value
    pub unsafe fn alloc_unchecked<T>(&self, v: T) -> &mut T {
        let layout = Layout::new::<T>();
        let ptr = self.alloc(layout).cast::<T>();

        unsafe {
            ptr::write(ptr.as_ptr(), v);
            &mut *ptr.as_ptr()
        }
    }
}

// -----------------------------------------------------------------------------
// AtomicPool

cfg_select! {
    target_family = "wasm" => {
        /// # wasm
        /// - Use `dlmalloc` by default, page_size = 64KiB.
        /// - Request two pages at a time, leaving some redundant space.
        ///
        /// > <https://github.com/alexcrichton/dlmalloc-rs/blob/main/src/wasm.rs>
        /// > <https://github.com/alexcrichton/dlmalloc-rs/blob/main/src/dlmalloc.rs>
        const CHUNK_SIZE: usize = 128 * 1024 - 128;
    }
    target_os = "android" => {
        /// # android
        /// - Use `scudo` by default, page_size = 64KiB.
        /// - Request two pages at a time, leaving some redundant space.
        ///
        /// > <https://technologeeks.com/blog/Scudo/>
        const CHUNK_SIZE: usize = 128 * 1024 - 128;
    }
    target_os = "windows" => {
        /// # windows
        /// - `once_alloc > 0x7FFF8 (512KiB)`
        ///
        /// "If the heap specified by the `hHeap` parameter is a 'non-growable' heap,
        /// `dwBytes` must be less than 0x7FFF8."
        ///
        /// The internal page size will be a power of 2, so the CHUNK_SIZE is slightly less than 512 K.
        ///
        /// > <https://learn.microsoft.com/en-us/windows/win32/api/heapapi/nf-heapapi-heapalloc>
        const CHUNK_SIZE: usize = 512 * 1024 - 128;
    }
    target_os = "linux" => {
        /// # Linux
        /// - `once_alloc > 128KiB (`
        /// - To reduce allocation, increase to `256KiB`, leaving some redundant space.
        ///
        ///  "When allocating  blocks of memory larger than MMAP_THRESHOLD bytes,
        /// the glibc  malloc() implementation allocates the memory as a private
        ///  anonymous mapping using mmap(2).  MMAP_THRESHOLD is 128 kB by default."
        ///
        /// > <https://man7.org/linux/man-pages/man3/malloc.3.html>
        const CHUNK_SIZE: usize = 256 * 1024 - 128;
    }
    _ => {
        /// # other
        /// - `PAGE_SIZE > 128KiB`
        const CHUNK_SIZE: usize = 256 * 1024 - 128;
    }
}

struct AtomicPool {
    tail: AtomicUsize,  //
    size: Mutex<usize>, // size + lock
}

static POOL: CachePadded<AtomicPool> = CachePadded::new(AtomicPool {
    tail: AtomicUsize::new(0),
    size: Mutex::new(CHUNK_SIZE),
});

// No need to impl `Drop`.
//
// Static items do not call drop at the end of the program.
//
// https://doc.rust-lang.org/reference/items/static-items.html

// -----------------------------------------------------------------------------
// GlobalPool

/// A process-wide, thread-safe memory pool for `'static` data.
///
/// `Global` is a zero-sized type used as a namespace; it cannot be
/// instantiated. All state lives in a private `static` pool, so every
/// operation is an associated function (`Global::alloc`, ...).
///
/// # Thread Safety
///
/// The fast path is lock-free: it performs a CAS on an atomic span
/// pointer stored in the current tail page. Only the slow path — when
/// a new page must be allocated — takes a mutex, and that mutex only
/// guards the page-size counter. `tail` itself is an `AtomicUsize`.
///
/// # Lifetime & Deallocation
///
/// All memory allocated from `Global` lives until the process exits.
/// Pages are never freed, and destructors for non-`Copy` values are
/// never run. This is intentional: `Global` is meant for long-lived,
/// process-wide data.
///
/// # When to Use
///
/// - **Shared data**: multiple threads allocate into the same pool
/// - **Process-lifetime data**: values that must outlive any thread
/// - **Memory-constrained startup**: avoid per-thread pools
///
/// For temporary, short-lived data, use [`Bump`] instead.
///
/// # Allocation API
///
/// - [`alloc`] returns a pointer to **uninitialized** memory with the
///   given layout. The caller is responsible for writing a value into
///   it before reading.
///
/// - [`alloc_str`] / [`alloc_slice`] copy the values into the pool
///   and return a `'static` reference.
///
/// - [`alloc_value`] move (ptr::write) a value into the pool and return
///   a `'static` reference. It requires `Copy` payloads, so no destructor
///   ever needs to run.
///
/// - [`alloc_static`] accepts any `T`, including types with `Drop`.
///   The value is moved into the pool; its destructor is never run (deliberate
///   leak).
///
/// # Example
///
/// ```rust
/// use zlim_utils::mem::Global;
/// use std::collections::BTreeSet;
///
/// let mut names: BTreeSet<&'static str> = BTreeSet::new();
///
/// let config = Global::alloc_str("App config");
/// assert_eq!(config, "App config");
///
/// names.insert(config);
/// ```
///
/// [`alloc`]: Self::alloc
/// [`alloc_value`]: Self::alloc_value
/// [`alloc_slice`]: Self::alloc_slice
/// [`alloc_str`]: Self::alloc_str
/// [`alloc_static`]: Self::alloc_static
#[repr(transparent)]
pub struct Global(());

impl Debug for Global {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Global(..)")
    }
}

impl Global {
    /// Allocates a new page from the system allocator.
    ///
    /// This is the slow path that is called when:
    /// 1. The pool is empty (no pages allocated yet), or
    /// 2. The current page does not have enough free space.
    ///
    /// The page is sized to `max(current_size, need)`, rounded up to the
    /// next power of two and aligned to `usize`. After allocation, the
    /// internal size counter grows by 1.5× for the next page.
    #[cold]
    #[inline(never)]
    fn alloc_layout_slow(layout: Layout) -> NonNull<u8> {
        let pool: &AtomicPool = &POOL;

        let mut guard = pool.size.lock().unwrap_or_else(PoisonError::into_inner);

        let prev = pool.tail.load(Acquire) as *mut usize;
        let may_block = Block::from_raw(prev);

        if let Some(prev_block) = may_block
            && let Some(p) = unsafe { prev_block.try_insert_atomic(layout) }
        {
            return p;
        }

        let need = layout.size() + layout.align().max(ALIGN);

        let unaligned = (*guard).max(need);
        *guard = (*guard) + ((*guard) >> 1);

        // Ensure that page_size if aligned.
        const MASK: usize = ALIGN - 1;
        let page_size = (MASK + unaligned) & !MASK;

        unsafe {
            let block = Block::alloc::<true>(page_size, prev);
            let ptr = block.try_insert_atomic(layout).expect("enough space");
            // `insert` before `store`, avoid competition and ensure success.
            let new_ptr: *mut usize = block.pointer.as_ptr();
            pool.tail.store(new_ptr as usize, Release);
            ptr
        }
    }

    #[inline(always)]
    fn alloc_inner(layout: Layout) -> NonNull<u8> {
        let pool: &AtomicPool = &POOL;
        let tail = pool.tail.load(Acquire) as *mut usize;
        let Some(block) = Block::from_raw(tail) else {
            // `alloc_layout_slow` already marked `#[coold]`
            return Global::alloc_layout_slow(layout);
        };

        unsafe {
            block.try_insert_atomic(layout).unwrap_or_else(|| {
                // `alloc_layout_slow` already marked `#[coold]`
                Global::alloc_layout_slow(layout)
            })
        }
    }
}

impl Global {
    /// Allocates memory with the given layout from the global pool.
    ///
    /// The returned pointer is aligned according to `layout.align()` and
    /// points to uninitialized memory. The caller is responsible for
    /// initializing it before reading.
    ///
    /// # Lifetime
    ///
    /// The returned memory lives for the entire duration of the program.
    /// `Global` pages are never deallocated until process exit.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::mem::Global;
    /// use core::alloc::Layout;
    ///
    /// let layout = Layout::new::<u32>();
    /// let ptr = Global::alloc(layout).cast::<u32>();
    ///
    /// unsafe {
    ///     ptr.as_ptr().write(42);
    ///     assert_eq!(*ptr.as_ptr(), 42);
    /// }
    /// ```
    #[inline(never)]
    pub fn alloc(layout: Layout) -> NonNull<u8> {
        Global::alloc_inner(layout)
    }

    /// Allocates a string slice by copying its contents into the global pool.
    ///
    /// The input must be valid UTF-8. The bytes are copied verbatim, and
    /// the returned reference points into pool-owned memory.
    ///
    /// # Lifetime
    ///
    /// The returned `&'static str` is valid for the entire duration of the
    /// program.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::mem::Global;
    ///
    /// let s: &'static str = Global::alloc_str("hello");
    /// assert_eq!(s, "hello");
    /// ```
    #[inline(never)]
    pub fn alloc_str(s: &str) -> &'static str {
        let layout = Layout::for_value(s.as_bytes());
        let ptr = Global::alloc_inner(layout).cast::<u8>();

        unsafe {
            let len = s.len();
            // Copy the slice contents
            ptr::copy_nonoverlapping(s.as_ptr(), ptr.as_ptr(), len);
            let bytes: &[u8] = core::slice::from_raw_parts_mut(ptr.as_ptr(), len);
            // SAFETY: The input is valid UTF-8, and we're copying it verbatim
            core::str::from_utf8_unchecked(bytes)
        }
    }

    /// Allocates a slice by copying its contents into the global pool.
    ///
    /// The elements must be `Copy`; they are bitwise-copied into pool-owned
    /// memory. The returned slice has the same length as the input.
    ///
    /// # Lifetime
    ///
    /// The returned `&'static mut [T]` is valid for the entire duration of
    /// the program.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::mem::Global;
    ///
    /// let original = [1_i32, 2, 3, 4];
    /// let slice: &'static mut [i32] = Global::alloc_slice(&original);
    ///
    /// assert_eq!(*slice, original);
    /// assert_ne!(slice.as_ptr(), original.as_ptr());
    /// ```
    #[inline]
    pub fn alloc_slice<T: Copy>(s: &[T]) -> &'static mut [T] {
        let layout = Layout::for_value(s);
        let ptr = Global::alloc(layout).cast::<T>();

        unsafe {
            // Copy the slice contents
            ptr::copy_nonoverlapping(s.as_ptr(), ptr.as_ptr(), s.len());
            core::slice::from_raw_parts_mut(ptr.as_ptr(), s.len())
        }
    }

    /// Allocates a value of type `T` in the global pool.
    ///
    /// `T` must be `Copy`, so the value is moved into pool-owned memory
    /// without needing to run a destructor.
    ///
    /// # Lifetime
    ///
    /// The returned `&'static mut T` is valid for the entire duration of
    /// the program.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::mem::Global;
    ///
    /// let v1 = Global::alloc_value(123);
    /// let v2 = Global::alloc_value([1, 2, 3, 4]);
    ///
    /// assert_eq!(*v1, 123);
    /// assert_eq!(*v2, [1, 2, 3, 4]);
    /// ```
    #[inline]
    pub fn alloc_value<T: Copy>(v: T) -> &'static mut T {
        let layout = Layout::new::<T>();
        let ptr = Global::alloc(layout).cast::<T>();

        unsafe {
            ptr::write(ptr.as_ptr(), v);
            &mut *ptr.as_ptr()
        }
    }

    /// Allocates a value of type `T` in the global pool.
    ///
    /// Unlike [`Global::alloc_value`], this method accepts any, `T`
    /// including types that implement [`Drop`]. The value is moved
    /// into pool-owned memory; the pool never runs its destructor.
    ///
    /// # Lifetime
    ///
    /// The returned `&'static mut T` is valid for the entire duration of
    /// the program.
    ///
    /// # Leak Warning
    ///
    /// If `T` implements [`Drop`], its destructor is **never** called,
    /// because `Global` pages are not deallocated until process exit.
    ///
    /// This is a deliberate memory leak; prefer [`Global::alloc_value`]
    /// when `T: Copy`.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_utils::mem::Global;
    ///
    /// let v: &'static mut u32 = Global::alloc_static(123_u32);
    ///
    /// assert_eq!(*v, 123_u32);
    /// ```
    #[inline]
    pub fn alloc_static<T: 'static>(v: T) -> &'static mut T {
        let layout = Layout::new::<T>();
        let ptr = Global::alloc(layout).cast::<T>();

        unsafe {
            ptr::write(ptr.as_ptr(), v);
            &mut *ptr.as_ptr()
        }
    }
}
