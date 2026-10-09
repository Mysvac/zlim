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
pub use uuid::Uuid;

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

mod reflect;

// -----------------------------------------------------------------------------
// prelude

/// The asset jobs.
pub mod jobs {
    #[doc(inline)]
    pub use crate::assets::jobs::HandleAssetDropEvents;
    #[doc(inline)]
    pub use crate::assets::jobs::HandleAssetEvents;
    #[doc(inline)]
    pub use crate::processor::jobs::StartAssetProcessServer;
    #[doc(inline)]
    pub use crate::server::jobs::AssetServerDiagnostic;
    #[doc(inline)]
    pub use crate::server::jobs::ClearFinishedAssetTask;
    #[doc(inline)]
    pub use crate::server::jobs::HandleAssetSaveCommands;
    #[doc(inline)]
    pub use crate::server::jobs::HandleAssetSeverEvents;
}

/// The asset plugins.
pub mod plugins {
    #[doc(no_inline)]
    pub use crate::plugin::{AssetPlugin, WebAssetPlugin};
}

/// The asset preludes.
pub mod prelude {
    // implicit use zlim_asset_derive::Asset;
    #[doc(no_inline)]
    pub use crate::asset::Asset;
    #[doc(no_inline)]
    pub use crate::assets::{AssetMut, Assets};
    #[doc(no_inline)]
    pub use crate::change::AssetChanged;
    #[doc(no_inline)]
    pub use crate::event::AssetEvent;
    #[doc(no_inline)]
    pub use crate::handle::{ErasedHandle, Handle};
    #[doc(no_inline)]
    pub use crate::ident::{AssetId, AssetSourceId};
    #[doc(no_inline)]
    pub use crate::path::AssetPath;
    #[doc(no_inline)]
    pub use crate::plugin::{AppAssetExt, WorldAssetExt};
    #[doc(no_inline)]
    pub use crate::plugin::{AssetDiagnosticsPlugin, AssetPlugin, WebAssetPlugin};
    #[doc(no_inline)]
    pub use crate::server::{AssetServer, AssetServerMode};
}

// -----------------------------------------------------------------------------
