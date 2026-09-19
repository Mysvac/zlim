#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(feature = "tracy", expect(unsafe_code, reason = "C FFI"))]

#[cfg(all(not(feature = "tracy"), feature = "tracy_memory"))]
compile_error!("`tracy_memory` cargo feature is enabled, but missing `tracy` feature");

#[cfg(all(not(feature = "tracy"), feature = "tracy_system"))]
compile_error!("`tracy_system` cargo feature is enabled, but missing `tracy` feature");

#[cfg(all(not(feature = "tracy"), feature = "tracy_sampling"))]
compile_error!("`tracy_sampling` cargo feature is enabled, but missing `tracy` feature");

#[cfg(all(not(feature = "tracy"), feature = "tracy_broadcast"))]
compile_error!("`tracy_broadcast` cargo feature is enabled, but missing `tracy` feature");

#[cfg(all(not(feature = "tracy"), feature = "tracy_only_localhost"))]
compile_error!("`tracy_only_localhost` cargo feature is enabled, but missing `tracy` feature");

#[cfg(all(not(feature = "tracy"), feature = "tracy_context_switch"))]
compile_error!("`tracy_context_switch` cargo feature is enabled, but missing `tracy` feature");

#[cfg(all(not(feature = "tracy"), feature = "tracy_callstack_inlines"))]
compile_error!("`tracy_callstack_inlines` cargo feature is enabled, but missing `tracy` feature");

#[cfg(all(not(feature = "tracy"), feature = "tracy_demangle"))]
compile_error!("`tracy_demangle` cargo feature is enabled, but missing `tracy` feature");

mod client;
mod frame;
mod gpu;
mod plot;
mod span;

#[cfg(all(feature = "tracy", feature = "tracy_memory"))]
mod memory;

#[cfg(all(feature = "tracy", feature = "tracy_demangle"))]
mod demangle;

pub use client::Client;
pub use frame::*;
pub use gpu::*;
pub use plot::*;
pub use span::*;

// -----------------------------------------------------------------------------
// Internal

/// Implementation details the macros of this crate expand to.
///
/// Nothing in this module is covered by the stability guarantees of the crate.
#[doc(hidden)]
pub mod internal {
    pub use core::cell::LazyCell;

    /// Clamp the stack depth to the maximum supported by Tracy.
    #[inline]
    #[must_use]
    pub const fn adjust_stack_depth(depth: u16) -> u16 {
        #[cfg(not(windows))]
        return depth;

        // Clamp `depth` to 62
        #[cfg(windows)]
        return 62 ^ ((depth ^ 62) & 0u16.wrapping_sub((depth < 62) as u16));
    }

    /// Constructs a [`PlotName`](crate::PlotName) from a null-terminated literal.
    ///
    /// Only the [`plot_name!`](crate::plot_name) macro calls this function, and it always appends
    /// the null terminator, so the profiler is never handed a string that runs past its end.
    #[must_use]
    #[track_caller]
    pub const fn create_plot(name: &'static str) -> crate::PlotName {
        debug_assert!(
            matches!(name.as_bytes().last(), Some(&0)),
            "a plot name must be null-terminated",
        );

        #[cfg(feature = "tracy")]
        return crate::PlotName::from_lit(name);

        #[cfg(not(feature = "tracy"))]
        {
            let _ = name;
            crate::PlotName::__no_tracy()
        }
    }

    /// Constructs a [`FrameName`](crate::FrameName) from a null-terminated literal.
    ///
    /// Only the [`frame_name!`](crate::frame_name) macro calls this function, and it always appends
    /// the null terminator, so the profiler is never handed a string that runs past its end.
    #[must_use]
    #[track_caller]
    pub const fn create_frame_name(name: &'static str) -> crate::FrameName {
        debug_assert!(
            matches!(name.as_bytes().last(), Some(&0)),
            "a frame name must be null-terminated",
        );

        #[cfg(feature = "tracy")]
        return crate::FrameName::from_lit(name);

        #[cfg(not(feature = "tracy"))]
        {
            let _ = name;
            crate::FrameName::__no_tracy()
        }
    }
}
