//! HTTP(S) asset sources.
//!
//! This module is the home of every HTTP based [`AssetReader`]. Today it is empty: the wasm
//! `fetch` reader lives in `io::platform`, and the remote (`http` / `https`) source is
//! deliberately not implemented yet, so that it can be written against a streamed,
//! connection-reusing transport instead of the removed `ureq` one.
//!
//! [`AssetReader`]: crate::io::AssetReader

// TODO
