#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

// -----------------------------------------------------------------------------
// Compile Config

/// Compilation configurations.
pub mod cfg {
    zlim_cfg::define_alias! {
        #[cfg(all(
            feature = "watch",
            any(target_os = "windows", target_os = "linux", target_os = "macos"),
        ))] => watch
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

pub use futures_lite::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
pub use uuid;

// -----------------------------------------------------------------------------
// Modules

pub mod asset;
pub mod assets;
pub mod change;
pub mod error;
pub mod event;
pub mod handle;
pub mod ident;
pub mod io;
pub mod loaded;
pub mod loader;
pub mod meta;
pub mod path;
pub mod plugin;
pub mod processor;
pub mod render;
pub mod saver;
pub mod server;
pub mod source;
pub mod transaction;
pub mod transformer;
pub mod utils;

// -----------------------------------------------------------------------------
// jobs re-exports

/// The asset jobs.
pub mod jobs {
    pub use crate::assets::jobs::HandleAssetDropEvents;
    pub use crate::assets::jobs::HandleAssetEvents;
    pub use crate::server::jobs::AssetServerDiagnostic;
    pub use crate::server::jobs::ClearFinishedAssetTask;
    pub use crate::server::jobs::HandleAssetSaveCommands;
    pub use crate::server::jobs::HandleAssetSeverEvents;
}

// -----------------------------------------------------------------------------
// jobs re-exports

/// The asset prelude.
pub mod prelude {
    // implicit use zlim_asset_derive::Asset;
    #[doc(hidden)]
    pub use crate::asset::Asset;
    #[doc(hidden)]
    pub use crate::assets::{AssetMut, Assets};
    #[doc(hidden)]
    pub use crate::change::AssetChanged;
    #[doc(hidden)]
    pub use crate::event::AssetEvent;
    #[doc(hidden)]
    pub use crate::handle::{ErasedHandle, Handle};
    #[doc(hidden)]
    pub use crate::ident::{AssetId, AssetSourceId};
    #[doc(hidden)]
    pub use crate::path::AssetPath;
    #[doc(hidden)]
    pub use crate::plugin::{AppAssetExt, WorldAssetExt};
    #[doc(hidden)]
    pub use crate::plugin::{AssetDiagnosticsPlugin, AssetPlugin, WebAssetPlugin};
    #[doc(hidden)]
    pub use crate::server::{AssetServer, AssetServerMode};
}

// -----------------------------------------------------------------------------
