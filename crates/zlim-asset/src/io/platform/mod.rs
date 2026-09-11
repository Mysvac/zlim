//! Platform specific asset readers.
//!
//! Readers live here when the platform keeps its assets somewhere that is not a filesystem
//! path — e.g. Android's APK `assets/` directory, which is only reachable through
//! `AAssetManager`. The plain filesystem source stays in [`crate::io::file`], which is
//! compiled on every target except wasm.

#[cfg(target_os = "android")]
mod android;

#[cfg(target_os = "android")]
pub use android::AndroidAssetReader;
