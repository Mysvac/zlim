//! Platform specific asset readers.
//!
//! Readers live here when the platform keeps its assets somewhere that is not a filesystem
//! path — e.g. Android's APK `assets/` directory, which is only reachable through
//! `AAssetManager`. The wasm reader, `HttpWasmAssetReader`, is the other case: it has no
//! storage of its own and asks the server for the bytes over HTTP instead. The plain
//! filesystem source stays in `crate::io::file`, which is compiled on every target except wasm.

#[cfg(target_os = "android")]
mod android;

#[cfg(target_os = "android")]
pub use android::AndroidAssetReader;

#[cfg(target_family = "wasm")]
mod wasm;

#[cfg(target_family = "wasm")]
pub use wasm::HttpWasmAssetReader;
