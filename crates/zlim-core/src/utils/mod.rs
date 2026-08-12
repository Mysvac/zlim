//! Internal utilities shared across the ECS core.
//!
//! # Overview
//!
//! | Module | Purpose |
//! |--------|--------|
//! | `debug_unwrap` | Debug-fail-fast / release-unchecked unwrap for Option/Result |
//! | [`dropper`] | Type-erased drop function wrapper for blob storage |
//! | `forgetter` | RAII guard that forgets an entity on panic during mutation |
//! | `helper` | SIMD-optimised search/sort/clamp helpers for entity and component slices |
//! | `ident` | `define_ident!` macro — generates niche-optimised, strongly-typed IDs |
//! | `ident_pool` | `SlicePool` — static interning pool for component-ID slices |
//!
//! # Visibility
//!
//! Most items in this module are `pub(crate)` — they are implementation
//! details of the ECS engine and not part of the public API.  The exception
//! is [`Dropper`], which is exposed for use by storage backends.
//!
//! [`Dropper`]: dropper::Dropper

mod debug_unwrap;
mod dropper;
mod forgetter;
mod helper;
mod ident;
mod ident_pool;

pub(crate) use debug_unwrap::DebugCheckedUnwrap;
pub(crate) use forgetter::ForgetEntityOnPanic;
pub(crate) use helper::*;
pub(crate) use ident::define_ident;
pub(crate) use ident_pool::SlicePool;

pub use dropper::Dropper;
