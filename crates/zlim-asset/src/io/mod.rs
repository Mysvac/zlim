//! Provides underlying IO interfaces.
//!
//! The layer is split in two halves:
//!
//! - **Byte streams** — [`Reader`], [`Writer`] and their concrete implementations
//!   ([`VecReader`], [`SliceReader`], …). They are thin extensions of `futures_lite`'s
//!   `AsyncRead` / `AsyncWrite` that add the "give me everything at once" fast paths.
//! - **Asset sources** — [`AssetReader`] / [`AssetWriter`] translate *asset paths* into byte
//!   streams (or directory listings) for one storage backend, and are grouped per
//!   [`AssetSourceId`] by [`AssetSources`].
//!
//! Both halves ship a type-erased mirror ([`ErasedAssetReader`] / [`ErasedAssetWriter`] for
//! sources) because the primary traits use RPITIT and are therefore not object safe.
//!
//! [`AssetSources`]: crate::source::AssetSources
//! [`AssetSourceId`]: crate::ident::AssetSourceId

pub mod reader;
pub mod writer;

pub mod embedded;
pub mod future;
pub mod memory;

pub mod watcher;

pub use embedded::EMBEDDED;
pub use reader::*;
pub use writer::*;

// HTTP(S) readers (currently the wasm `fetch` one) live in `io::http`.
pub mod http;

// Platform specific readers (currently the Android `AAssets` one) live in `io::platform`.
pub mod platform;

#[cfg(not(target_family = "wasm"))]
pub mod file;

// TODO(asset_processor): `gated.rs` (`ProcessorGatedReader`) lands with the processor (M4).
