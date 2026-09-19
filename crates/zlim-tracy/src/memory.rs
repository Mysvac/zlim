use core::alloc::{GlobalAlloc, Layout};
use std::alloc::System;

use tracy_client_sys as sys;

// -----------------------------------------------------------------------------
// ProfiledAllocator

/// A global allocator that reports every allocation and deallocation to the profiler.
///
/// Reporting the allocations of the process makes the memory view of the profiler show where the
/// memory of the program goes.
struct ProfiledAllocator<T>(T, u16);

impl<T> ProfiledAllocator<T> {
    /// Wraps `inner_allocator`, reporting allocations with a callstack of at most
    /// `callstack_depth` frames.
    ///
    /// A `callstack_depth` of zero reports the allocations without any callstack, which is much
    /// cheaper but does not say where the allocations come from.
    const fn new(inner_allocator: T, callstack_depth: u16) -> Self {
        Self(
            inner_allocator,
            crate::internal::adjust_stack_depth(callstack_depth),
        )
    }

    /// Reports an allocation to the profiler.
    fn emit_alloc(&self, ptr: *mut u8, size: usize) {
        unsafe {
            // SAFETY: the profiler only reads the pointer and the size.
            if self.1 == 0 {
                let () = sys::___tracy_emit_memory_alloc(ptr.cast(), size);
            } else {
                let () = sys::___tracy_emit_memory_alloc_callstack(ptr.cast(), size, self.1.into());
            }
        }
    }

    /// Reports a deallocation to the profiler.
    fn emit_free(&self, ptr: *mut u8) {
        unsafe {
            // SAFETY: the profiler only reads the pointer.
            if self.1 == 0 {
                let () = sys::___tracy_emit_memory_free(ptr.cast());
            } else {
                let () = sys::___tracy_emit_memory_free_callstack(ptr.cast(), self.1.into());
            }
        }
    }
}

#[expect(
    unsafe_code,
    reason = "`GlobalAlloc` is an unsafe trait with unsafe methods"
)]
unsafe impl<T: GlobalAlloc> GlobalAlloc for ProfiledAllocator<T> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller of this method upholds the contract of `GlobalAlloc::alloc`.
        let ptr = unsafe { self.0.alloc(layout) };
        self.emit_alloc(ptr, layout.size());
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller of this method upholds the contract of `GlobalAlloc::alloc_zeroed`.
        let ptr = unsafe { self.0.alloc_zeroed(layout) };
        self.emit_alloc(ptr, layout.size());
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.emit_free(ptr);

        // SAFETY: the caller of this method upholds the contract of `GlobalAlloc::dealloc`.
        unsafe { self.0.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        self.emit_free(ptr);

        // SAFETY: the caller of this method upholds the contract of `GlobalAlloc::realloc`.
        let ptr = unsafe { self.0.realloc(ptr, layout, new_size) };
        self.emit_alloc(ptr, new_size);
        ptr
    }
}

// -----------------------------------------------------------------------------
// Global allocator

/// The global allocator of the program, which reports the allocations to the profiler.
///
/// The `tracy_memory` feature installs this allocator, so enabling the feature is all a program has
/// to do to get memory profiling. A program has a single global allocator, hence the feature cannot
/// be combined with an allocator of its own.
///
/// The callstack depth is a compromise: deep enough to tell the parts of the program that allocate
/// apart, and shallow enough for the cost of collecting it to stay bearable.
#[global_allocator]
static GLOBAL: ProfiledAllocator<System> = ProfiledAllocator::new(System, 100);
