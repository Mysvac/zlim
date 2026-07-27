//! Provides experimental api.
//!
//! - [`SyncView`]: A reimplementation of unstable `core::sync::SyncView`
//! - [`SyncUnsafeCell`]: A reimplementation of unstable `core::cell::SyncUnsafeCell`

mod sync_unsafe_cell;
mod sync_view;

pub use sync_unsafe_cell::SyncUnsafeCell;
pub use sync_view::SyncView;
