//! The asset plugin: the server, its sources, the importer, and the per-asset-type jobs.
//!
//! [`AssetPlugin`] is what makes the asset system usable: it builds the default and `embedded`
//! asset sources, inserts the [`AssetServer`] and the event pump, and registers the built-in asset
//! types. Concrete asset types are registered through [`AppAssetExt::init_asset`], which needs the
//! server and the standard schedules, so it runs once `AssetPlugin` has been applied.
//!
//! # Processed mode
//!
//! In [`AssetServerMode::Processed`] the app reads the *processed* side, which means something has
//! to write it. That something is [`AssetProcessServer`]: the plugin builds one over the same
//! sources, inserts it as a resource, and schedules `StartAssetProcessServer` in `Startup`, so
//! the import happens once every plugin has registered its loaders and processors. The app's server
//! and the importer share one loader registry — a loader registered through [`AppAssetExt`] is the
//! one the processors load with — while the processors themselves live only on the importer:
//!
//! ```ignore
//! app.add_plugins((
//!     AssetPlugin {
//!         server_mode: AssetServerMode::Processed,
//!         ..AssetPlugin::default()
//!     },
//!     MyAssetPlugin, // ordered after `AssetPlugin`, registers the processors
//! ));
//! ```
//!
//! Plugins in zlim are lazy: `App::add_plugins` only stores them, and every plugin's `build` /
//! `apply` runs in [`App::build`]. The usual ordering is therefore
//!
//! ```ignore
//! app.add_plugins(AssetPlugin::default()).build();
//! app.init_asset::<MyAsset>().register_asset_loader(MyLoader);
//! ```
//!
//! or, for code that has to stay inside the plugin lifecycle, from another plugin's [`apply`] — but
//! then that plugin has to be ordered after this one with [`AssetPlugin::apply_before`], because
//! the apply order of plugins that do not order themselves relative to each other is unspecified:
//!
//! ```ignore
//! impl Plugin for MyPlugin {
//!     fn build(&mut self, app: &mut App) {
//!         AssetPlugin::apply_before::<Self>(app);
//!     }
//!
//!     fn apply(&mut self, app: &mut App) {
//!         app.init_asset::<MyAsset>().register_asset_loader(MyLoader);
//!     }
//! }
//! ```
//!
//! [`AssetServer`]: crate::server::AssetServer
//! [`AssetServerMode::Processed`]: crate::server::AssetServerMode::Processed
//! [`AssetProcessServer`]: crate::processor::AssetProcessServer
//! [`App::build`]: zlim_app::App::build
//! [`apply`]: zlim_app::Plugin::apply
//! [`AssetPlugin::apply_before`]: zlim_app::PluginExt::apply_before

// -----------------------------------------------------------------------------
// Modules

mod diagnostics;
mod plugin;
mod traits;
pub mod web;

pub use diagnostics::AssetDiagnosticsPlugin;
pub use plugin::AssetPlugin;
pub use traits::{AppAssetExt, WorldAssetExt};
pub use web::WebAssetPlugin;

// -----------------------------------------------------------------------------
// Default paths

/// The default path of the unprocessed asset source, relative to the asset root.
pub const DEFAULT_UNPROCESSED_FILE_PATH: &str = "assets";

/// The default path of the processed asset source, relative to the asset root.
pub const DEFAULT_PROCESSED_FILE_PATH: &str = "imported_assets/default";

// -----------------------------------------------------------------------------
