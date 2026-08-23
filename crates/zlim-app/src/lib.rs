#![cfg_attr(docsrs, expect(internal_features, reason = "needed for fake_variadic"))]
#![cfg_attr(docsrs, feature(doc_cfg, rustdoc_internals))]

// -----------------------------------------------------------------------------
// Modules

mod app;
mod exit;
mod label;
mod plugin;

// -----------------------------------------------------------------------------
// Exports

pub use zlim_app_derive as derive;
pub use zlim_app_derive::AppLabel;

pub use app::*;
pub use exit::AppExit;
pub use label::{AppLabel, InternedAppLabel};
pub use plugin::PlaceholderPlugin;
pub use plugin::{DuplicateStrategy, PluginsState};
pub use plugin::{Plugin, PluginGroup, Plugins};

// -----------------------------------------------------------------------------
// Special platform support

#[doc(hidden)]
pub mod sys {
    zlim_os::cfg::android! {
        pub use zlim_os::sys::android_activity::AndroidApp;
        pub static ANDROID_APP: std::sync::OnceLock<AndroidApp> = std::sync::OnceLock::new();
    }
}
