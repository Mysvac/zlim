//! Asset loading, caching and lifecycle management for the zlim engine.
#![cfg_attr(docsrs, feature(doc_cfg))]

// -----------------------------------------------------------------------------
// Compile Config

pub mod cfg {
    zlim_cfg::define_alias! {
        #[cfg(all(
            feature = "notify",
            any(target_os = "windows", target_os = "linux", target_os = "macos"),
        ))] => notify
    }
}

// -----------------------------------------------------------------------------
// Extern Self

// Usually, we need to use `crate` in the crate itself and use `zlim_*` in
// doc testing. `zlim_derive_utils::crate_path` choose `zlim_*`, so we must
// have an `extern self` to ensure it can be used as an alias for `crate`.
extern crate self as zlim_asset;

// -----------------------------------------------------------------------------
// Macros

pub use zlim_asset_derive as derive;

// -----------------------------------------------------------------------------
// 3rd

pub use uuid;

// -----------------------------------------------------------------------------
// Modules

pub mod asset;
pub mod assets;
pub mod change;
pub mod event;
pub mod handle;
pub mod ident;
pub mod io;
pub mod loader;
pub mod meta;
pub mod path;
pub mod processor;
pub mod render;
pub mod saver;
pub mod server;
pub mod source;
pub mod transformer;
pub mod utils;

// -----------------------------------------------------------------------------
// Exports

// ↓ Waiting for all content completed.
// pub use crate::asset::{Asset, VisitAssetDependencies};
// pub use crate::assets::Assets;
// pub use crate::event::AssetEvent;
// pub use crate::handle::{ErasedHandle, Handle};
// pub use crate::ident::{AssetId, ErasedAssetId};
// pub use crate::path::AssetPath;

/// The most commonly used asset items.
pub mod prelude {
    pub use crate::asset::{Asset, AssetComponent, VisitAssetDependencies};
    pub use crate::assets::{AssetMut, Assets};
    pub use crate::event::{AssetEvent, AssetSourceEvent};
    pub use crate::handle::{AssetHandleProvider, ErasedHandle, Handle};
    pub use crate::ident::{AssetId, AssetIndex, AssetSourceId, ErasedAssetId};
    pub use crate::io::future::{ReadAllFuture, WriteAllFuture};
    pub use crate::io::watcher::AssetWatcher;
    pub use crate::io::{AssetReader, AssetReaderError, ErasedAssetReader, Reader, VecReader};
    pub use crate::io::{AssetWriter, AssetWriterError, ErasedAssetWriter, Writer};
    pub use crate::path::AssetPath;
    pub use crate::source::{AssetSource, AssetSources};
    pub use crate::source::{AssetSourceBuilder, AssetSourceBuilders};
}
