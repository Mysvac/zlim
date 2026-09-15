//! The transaction log: what tells an interrupted import run apart from a finished one.
//!
//! The built-in file-backed logger lives in `file`, which is only compiled off wasm.

mod base;
pub use base::*;

#[cfg(not(target_family = "wasm"))]
pub mod file;
