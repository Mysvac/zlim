//! Asset loading, caching and lifecycle management for the zlim engine.
#![cfg_attr(docsrs, feature(doc_cfg))]

// -----------------------------------------------------------------------------
// 3rd

pub mod cfg {
    zlim_cfg::define_alias! {
        #[cfg(all(
            feature = "notify",
            any(target_os = "windows", target_os = "linux", target_os = "macos"),
        ))] => notify
    }
}

// -----------------------------------------------------------------------------
// 3rd

pub use uuid;

// -----------------------------------------------------------------------------
// Modules

pub mod asset;
pub mod handle;
pub mod ident;
pub mod io;
pub mod path;

mod utils;

// -----------------------------------------------------------------------------
// Exports

pub use crate::handle::{ErasedHandle, Handle};
pub use crate::utils::{BoxedFuture, EmptyPathStream, PathStream};

/// The most commonly used asset items.
pub mod prelude {
    pub use crate::asset::{Asset, AssetComponent, VisitAssetDependencies};
    pub use crate::handle::{AssetHandleProvider, ErasedHandle, Handle};
    pub use crate::ident::{AssetId, AssetIndex, AssetSourceId, ErasedAssetId, TypedAssetIndex};
    pub use crate::io::future::{ReadAllFuture, WriteAllFuture};
    pub use crate::io::watcher::AssetWatcher;
    pub use crate::io::{AssetReader, AssetReaderError, ErasedAssetReader, Reader, VecReader};
    pub use crate::io::{AssetSource, AssetSourceBuilder};
    pub use crate::io::{AssetSourceBuilders, AssetSourceEvent, AssetSources};
    pub use crate::io::{AssetWriter, AssetWriterError, ErasedAssetWriter, Writer};
    pub use crate::path::AssetPath;
}
