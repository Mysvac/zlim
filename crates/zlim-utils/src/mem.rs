//! Simple memory pool.
#![expect(unsafe_code, reason = "raw pointer is unsafe")]
#![expect(
    clippy::mut_from_ref,
    reason = "the data is copied, instead of original ref"
)]

use core::alloc::Layout;
use core::cell::Cell;
use core::panic::{RefUnwindSafe, UnwindSafe};
use core::ptr::{self, NonNull};
use std::alloc as malloc;
use std::sync::{Mutex, MutexGuard, PoisonError};

// -----------------------------------------------------------------------------
// Block

const ALIGN: usize = align_of::<usize>();
const SIZE1: usize = size_of::<usize>();
const SIZE2: usize = SIZE1 * 2;
const SIZE3: usize = SIZE1 * 3;

/// A memory block that acts as a page in the bump allocator.
///
/// ```text
/// ┌────────────────┬───────────────┬───────────────┬─────────────────┐
/// │ block_len      │ prev_ptr      │ span          │ user_data       │
/// │ (usize)        │ (usize)       │ (usize)       │ (need bytes)    │
/// └────────────────┴───────────────┴───────────────┴─────────────────┘
/// │<─── SIZE1 ────>│<─── SIZE1 ───>│<─── SIZE1 ───>│<──── need ─────>│
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
    /// # Safety
    /// - `prev` must be a valid pointer to the previous block or null.
    /// - `need` must be less than or equal to `isize::MAX`.
    #[must_use]
    unsafe fn alloc(need: usize, prev: *mut usize) -> Self {
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
            // span
            span.write(free.as_ptr() as *mut u8 as usize);
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

        if new_span <= tail {
            unsafe {
                span_ptr.write(new_span);
                // ↓ Faster than `NonNull::new(aligned_span as *mut u8)`
                Some(NonNull::new_unchecked(aligned_span as *mut u8))
            }
        } else {
            None
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

/// A bump-allocator pool that allocates memory in fixed-size pages.
///
/// `PagePool` is a simple, append-only memory pool that allocates data
/// in pages. Each page is a contiguous block of memory that can hold
/// multiple allocations. When a page is full, a new page is allocated
/// and linked to the previous one.
///
/// # Type Parameters
///
/// - `PAGE_SIZE`: The size of each page in bytes.
///
/// # Drop Behavior
///
/// When the pool is dropped, all allocated pages are deallocated.
/// However, the pool does **not** call `drop` on the allocated data.
#[derive(Debug)]
struct PagePool<const PAGE_SIZE: usize> {
    tail: Cell<*mut usize>,
}

// SAFETY: !Sync, but Send.
unsafe impl<const PAGE_SIZE: usize> Send for PagePool<PAGE_SIZE> {}

impl<const PAGE_SIZE: usize> UnwindSafe for PagePool<PAGE_SIZE> {}

impl<const PAGE_SIZE: usize> RefUnwindSafe for PagePool<PAGE_SIZE> {}

impl<const PAGE_SIZE: usize> Drop for PagePool<PAGE_SIZE> {
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

impl<const PAGE_SIZE: usize> PagePool<PAGE_SIZE> {
    /// An empty pool with no pages allocated.
    #[expect(
        clippy::declare_interior_mutable_const,
        reason = "const used as a default/empty sentinel value"
    )]
    const EMPTY: Self = Self {
        tail: Cell::new(ptr::null_mut()),
    };

    /// Allocates memory with the given layout and returns a pointer to it.
    ///
    /// The returned pointer is aligned according to the layout's alignment
    /// requirement. The memory is uninitialized and should be initialized
    /// by the caller.
    fn alloc(&self, layout: Layout) -> NonNull<u8> {
        let Some(block) = Block::from_raw(self.tail.get()) else {
            core::hint::cold_path();
            return self.alloc_layout_slow(layout);
        };

        unsafe {
            block
                .try_insert(layout)
                .unwrap_or_else(|| self.alloc_layout_slow(layout))
        }
    }

    /// Allocates a string slice by copying its contents into the pool.
    ///
    /// Returns a reference to the copied string. The input must be
    /// valid UTF-8.
    #[inline]
    fn alloc_str(&self, s: &str) -> &str {
        let bytes = self.alloc_slice(s.as_bytes());

        unsafe {
            // SAFETY: The input is valid UTF-8, and we're copying it verbatim
            core::str::from_utf8_unchecked(bytes)
        }
    }

    /// Allocates a slice by copying its contents into the pool.
    ///
    /// Returns a mutable reference to the copied slice. The slice elements
    /// must be `Copy`.
    ///
    /// This is safe because `T` implements `Copy` and does not require `Drop`.
    #[inline]
    fn alloc_slice<T: Copy>(&self, slice: &[T]) -> &mut [T] {
        let layout = Layout::for_value(slice);
        let ptr = self.alloc(layout).cast::<T>();

        unsafe {
            // Copy the slice contents
            ptr::copy_nonoverlapping(slice.as_ptr(), ptr.as_ptr(), slice.len());
            core::slice::from_raw_parts_mut(ptr.as_ptr(), slice.len())
        }
    }

    /// Allocates a value of type `T` in the pool and returns a mutable reference.
    ///
    /// The value is moved into the pool's memory. The returned reference is valid
    /// until the pool is cleared or destroyed.
    ///
    /// This is safe because `T` implements `Copy` and does not require `Drop`.
    #[inline]
    fn alloc_value<T: Copy>(&self, val: T) -> &mut T {
        let layout = Layout::new::<T>();
        let ptr = self.alloc(layout).cast::<T>();

        unsafe {
            ptr::write(ptr.as_ptr(), val);
            &mut *ptr.as_ptr()
        }
    }

    /// Allocates a value of type `T` without requiring `Copy`.
    ///
    /// Unlike [`alloc_value`](Self::alloc_value), this method accepts any
    /// `T`. The value is moved into pool-owned memory and **never dropped**
    /// by the pool.
    ///
    /// # Safety
    ///
    /// If `T` implements [`Drop`], the caller **must** manually run the
    /// destructor before the pool is destroyed.
    #[inline]
    pub unsafe fn alloc_unchecked<T>(&self, val: T) -> &mut T {
        let layout = Layout::new::<T>();
        let ptr = self.alloc(layout).cast::<T>();

        unsafe {
            ptr::write(ptr.as_ptr(), val);
            &mut *ptr.as_ptr()
        }
    }

    /// Allocates a new page from the system allocator.
    ///
    /// This is the slow path that is called when:
    /// 1. The pool is empty (no pages allocated yet), or
    /// 2. The current page does not have enough free space.
    ///
    /// The page size is rounded up to the power of two and aligned to `usize`.
    #[inline(never)]
    fn alloc_layout_slow(&self, layout: Layout) -> NonNull<u8> {
        let need = layout.size() + layout.align().max(ALIGN);
        let unaligned = PAGE_SIZE.max(need).next_power_of_two();

        // Ensure that page_size if aligned.
        const MASK: usize = ALIGN - 1;
        let page_size = (MASK + unaligned) & !MASK;

        unsafe {
            let prev = self.tail.get();
            let block = Block::alloc(page_size, prev);
            self.tail.set(block.pointer.as_ptr());

            block.try_insert(layout).expect("enough space")
        }
    }
}

// -----------------------------------------------------------------------------
// Bump

/// A bump allocator with a small page size for temporary caches.
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
///     let cache = Bump::new();
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
pub struct Bump(PagePool<2000>);

impl Default for Bump {
    /// Create a empty pool without additional memory.
    #[inline]
    fn default() -> Self {
        Self(PagePool::EMPTY)
    }
}

impl Bump {
    /// Create a empty pool without additional memory.
    #[inline]
    pub const fn new() -> Self {
        Self(PagePool::EMPTY)
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
    /// let pool = Bump::new();
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
    /// let pool = Bump::new();
    /// let s = pool.alloc_str("Hello, world!");
    /// assert_eq!(s, "Hello, world!");
    /// assert_ne!(s.as_ptr(), "Hello, world!".as_ptr());
    /// ```
    pub fn alloc_str<'a>(&'a self, s: &str) -> &'a str {
        self.0.alloc_str(s)
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
    /// let pool = Bump::new();
    /// let v1 = pool.alloc_value(123);
    /// let v2 = pool.alloc_value([1, 2, 3, 4]);
    ///
    /// assert_eq!(*v1, 123);
    /// assert_eq!(*v2, [1, 2, 3, 4]);
    /// ```
    pub fn alloc_value<T: Copy>(&self, v: T) -> &mut T {
        self.0.alloc_value(v)
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
    /// let pool = Bump::new();
    /// let original = [1, 2, 3, 4, 5];
    /// let slice = pool.alloc_slice(&original);
    ///
    /// assert_eq!(*slice, original);
    /// assert_ne!(slice.as_ptr(), original.as_ptr());
    /// ```
    pub fn alloc_slice<'a, T: Copy>(&'a self, s: &[T]) -> &'a mut [T] {
        self.0.alloc_slice(s)
    }

    /// Allocates a value of type `T` without requiring `Copy`.
    ///
    /// Unlike [`alloc_value`](Self::alloc_value), this method accepts any `T`.
    /// The value is moved into pool-owned memory and never dropped by the
    /// pool.
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
    /// let pool = Bump::new();
    ///
    /// // i32 is fine — no Drop, no problem.
    /// let v = unsafe { pool.alloc_unchecked(42i32) };
    /// assert_eq!(*v, 42);
    ///
    /// // For a type with Drop, the caller must invoke the destructor:
    /// // let s = unsafe { pool.alloc_unchecked(String::from("hi")) };
    /// // unsafe { core::ptr::drop_in_place(s); }
    /// ```
    /// ['drop`]: Drop::drop
    pub unsafe fn alloc_unchecked<T>(&self, v: T) -> &mut T {
        // SAFETY: delegated to the pool's `alloc_unchecked`; the caller is
        // responsible for running `Drop` on the returned value.
        unsafe { self.0.alloc_unchecked(v) }
    }
}

// -----------------------------------------------------------------------------
// STATIC_POOL

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
        /// > <https://learn.microsoft.com/en-us/windows/win32/api/heapapi/nf-heapapi-heapalloc>
        const CHUNK_SIZE: usize = 512 * 1024 + 8;
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

struct Pool(PagePool<CHUNK_SIZE>);

static POOL: Mutex<Pool> = Mutex::new(Pool(PagePool::EMPTY));

/// A global, shared memory pool for static data.
///
/// `Global` is a **mutex-protected** pool that is shared across all threads.
/// It uses a large page size to efficiently manage long-lived static data.
///
/// This pool is never deallocated, and all allocations live for the entire
/// program duration.
///
/// # When to Use
///
/// This is suitable when:
///
/// - **Memory is constrained**: You cannot afford per-thread pools
/// - **Low-frequency allocation**: Data is allocated rarely (e.g., at startup)
/// - **Shared data**: Multiple threads need access to the same pool
///
/// For temporary, short-lived data, use [`Bump`] instead.
///
/// # Drop Behavior
///
/// When the pool is dropped, all allocated pages are deallocated.
/// However, the pool does **not** call `drop` on the allocated data.
///
/// Use [`alloc_value`] / [`alloc_slice`] / [`alloc_str`] for `Copy` types
/// (they are safe). Use [`alloc_unchecked`] for non-`Copy` types — in that
/// case the caller is responsible for running destructors.
///
/// [`alloc_value`]: Self::alloc_value
/// [`alloc_slice`]: Self::alloc_slice
/// [`alloc_str`]: Self::alloc_str
/// [`alloc_unchecked`]: Self::alloc_unchecked
///
/// # Example
///
/// ```rust
/// use zlim_utils::mem::Global;
/// use std::collections::BTreeSet;
///
/// let mut names: BTreeSet<&'static str> = BTreeSet::new();
///
/// // Allocate a global configuration string
/// let config = Global::alloc_str("App config");
/// assert_eq!(config, "App config");
///
/// // Use memory pools to store data.
/// // Use other structures to store references.
/// names.insert(config);
/// ```
#[derive(Debug)]
#[repr(transparent)]
pub struct Global(());

impl Global {
    /// Locks the global pool and returns a guard.
    #[inline(always)]
    fn lock() -> MutexGuard<'static, Pool> {
        POOL.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Allocates memory with the given layout.
    ///
    /// See [`Bump::alloc`] for details.
    #[inline(never)]
    pub fn alloc(layout: Layout) -> NonNull<u8> {
        Self::lock().0.alloc(layout)
    }

    /// Allocates a string slice by copying its contents.
    ///
    /// The returned string has a `'static` lifetime.
    ///
    /// See [`Bump::alloc_str`] for details.
    #[inline(never)]
    pub fn alloc_str(s: &str) -> &'static str {
        let binding = &Self::lock().0;
        let r: &str = binding.alloc_str(s);
        // SAFETY: The data is allocated in `POOL` (a `static`), giving it `'static` lifetime.
        // The MutexGuard (`binding`) is dropped after this line, but the pool data persists.
        unsafe { core::mem::transmute(r) }
    }

    /// Allocates a value of type `T` in the pool.
    ///
    /// The returned reference has a `'static` lifetime.
    ///
    /// See [`Bump::alloc_value`] for details.
    #[inline(never)]
    pub fn alloc_value<T: Copy>(v: T) -> &'static mut T {
        let binding = &Self::lock().0;
        let r: &mut T = binding.alloc_value(v);
        // SAFETY: The data is allocated in `POOL` (a `static`), giving it `'static` lifetime.
        // The MutexGuard (`binding`) is dropped after this line, but the pool data persists.
        unsafe { core::mem::transmute(r) }
    }

    /// Allocates a slice by copying its contents.
    ///
    /// The returned slice has a `'static` lifetime.
    ///
    /// See [`Bump::alloc_slice`] for details.
    #[inline(never)]
    pub fn alloc_slice<T: Copy>(s: &[T]) -> &'static mut [T] {
        let binding = &Self::lock().0;
        let r: &mut [T] = binding.alloc_slice(s);
        // SAFETY: The data is allocated in `POOL` (a `static`), giving it `'static` lifetime.
        // The MutexGuard (`binding`) is dropped after this line, but the pool data persists.
        unsafe { core::mem::transmute(r) }
    }

    /// Allocates a value of type `T` without requiring `Copy`.
    ///
    /// The returned reference has a `'static` lifetime.
    ///
    /// # Safety
    ///
    /// If `T` implements [`Drop`], the caller **must** manually run the
    /// destructor. The pool itself will never call [`drop`] on the allocated
    /// value.
    ///
    /// [`drop`]: Drop::drop
    #[inline(never)]
    pub unsafe fn alloc_unchecked<T>(v: T) -> &'static mut T {
        let binding = &Self::lock().0;
        // SAFETY: delegated to the pool's `alloc_unchecked`; the caller is
        // responsible for running `Drop` on the returned value.
        let r: &mut T = unsafe { binding.alloc_unchecked(v) };
        // SAFETY: The data is allocated in `POOL` (a `static`), giving it `'static` lifetime.
        // The MutexGuard (`binding`) is dropped after this line, but the pool data persists.
        unsafe { core::mem::transmute(r) }
    }
}
