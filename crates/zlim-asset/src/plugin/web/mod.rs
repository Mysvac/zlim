//! HTTP(S) asset sources.
//!
//! The module holds [`WebAssetPlugin`], which registers the `http` and `https` sources when the
//! matching cargo feature is on, and the [`AssetReader`] behind them: on the native targets a shared
//! `ureq` agent drives the requests (with `blocking` carrying the blocking IO off the async thread),
//! and on wasm the `fetch` reader of `io::platform` does.
//!
//! With the `web_asset_cache` feature, the native reader keeps the bytes it fetched in an on-disk
//! cache under `.web-asset-cache`; that cache is a development tool (it validates the `url` only and
//! never expires an entry), which is why it is only compiled off wasm.
//!
//! [`AssetReader`]: crate::io::AssetReader

use zlim_app::{App, MainSchedulePlugin, Plugin, PluginExt};
use zlim_path::TypePath;

#[cfg(feature = "http")]
use crate::plugin::AppAssetExt;

use super::AssetPlugin;

// -----------------------------------------------------------------------------

#[cfg(not(target_family = "wasm"))]
mod cache;

#[cfg(not(target_family = "wasm"))]
pub use cache::{load_cache, save_cache};

#[cfg(any(feature = "http", feature = "https"))]
mod http;

// -----------------------------------------------------------------------------
// WebAssetPlugin

/// The asset source plugin for the network: it registers the `http` and `https` sources.
///
/// The sources themselves are opt-in. Without the `http` or the `https` cargo feature this plugin
/// registers nothing, so it is always safe to add to an app; with one of them, the matching source
/// is registered here and the reader behind it (`http.rs`) is what loads the assets.
///
/// It needs [`AssetPlugin`], which is the plugin that turns the registered sources into real ones,
/// and it is applied **before** it: a source can only be added to the builders until they are built,
/// and building them is the first thing [`AssetPlugin`] does.
#[derive(Debug, Default, TypePath)]
pub struct WebAssetPlugin;

impl Plugin for WebAssetPlugin {
    /// Orders this plugin against the two plugins it has to work with.
    ///
    /// `MainSchedulePlugin` comes first, so that the common schedules exist by the time this plugin
    /// is applied, and [`AssetPlugin`] comes *after* it, so that the sources registered below are
    /// still in the builders when it builds them.
    fn build(&mut self, app: &mut App) {
        MainSchedulePlugin::apply_before::<Self>(app);
        AssetPlugin::apply_after::<Self>(app); // `AssetPlugin` has to come after this plugin.
    }

    /// Registers the remote sources the enabled features provide.
    ///
    /// An app without [`AssetPlugin`] is reported: a source registered by a plugin like this one
    /// would never be built into a real source, and nothing would ever load from it.
    fn apply(&mut self, app: &mut App) {
        MainSchedulePlugin::warn_if_unset(app, "WebAssetPlugin");
        if !app.contains_plugin::<AssetPlugin>() {
            zlim_log::warn!("`WebAssetPlugin` is added but missing `AssetPlugin`.");
        }

        #[cfg(feature = "http")]
        app.register_asset_source("http", http::http_source_builder());

        #[cfg(feature = "https")]
        app.register_asset_source("https", http::https_source_builder());
    }
}

// -----------------------------------------------------------------------------
