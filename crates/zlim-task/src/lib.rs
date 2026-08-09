#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

// -----------------------------------------------------------------------------
// Compilation config

/// Some macros used for compilation control.
pub mod cfg {
    pub use crate::platform::{multi_thread, single_thread};
}

// -----------------------------------------------------------------------------
// Modules

mod platform;
mod slice;

// -----------------------------------------------------------------------------
// Exports

pub use platform::{AsyncTaskPool, IoTaskPool, MainTaskPool};
pub use platform::{Scope, TaskPool, TaskPoolBuilder};
pub use platform::{block_on, run_local};
pub use slice::ParallelSlice;

// -----------------------------------------------------------------------------
// Re-Exports

pub use async_task::Task;
pub use futures_lite;
pub use futures_lite::future::poll_once;
