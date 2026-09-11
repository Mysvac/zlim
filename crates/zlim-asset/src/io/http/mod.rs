//! HTTP(S) asset sources.
//!
//! This module is the home of every HTTP based [`AssetReader`](crate::io::AssetReader).
//! Today it only holds the wasm `fetch` reader; the remote (`http` / `https`) source is
//! deliberately not implemented yet (see `ARCHITECTURE.md` §9), so that it can be written
//! against a streamed, connection-reusing transport instead of the removed `ureq` one.

#[cfg(target_family = "wasm")]
mod wasm;

#[cfg(target_family = "wasm")]
pub use wasm::HttpWasmAssetReader;
