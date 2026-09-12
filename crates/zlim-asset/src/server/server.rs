#![expect(clippy::module_inception, reason = "For better structure.")]

use std::sync::Arc;

use zlim_core::borrow::{Res, ResMut};
use zlim_core::derive::Resource;
use zlim_core::derive::job_fn;
use zlim_diagnostic::{DiagnosticPath, Diagnostics};
use zlim_path::derive::TypePath;

use super::AssetServerData;

// -----------------------------------------------------------------------------
// AssetServer

/// Central coordinator for asset loading, caching, and lifecycle tracking.
///
/// [`AssetServer`] is a cheaply-cloneable handle to a sealed `AssetServerData` instance.
///
/// Add it to your app via `AssetPlugin` and access it through `Res<AssetServer>`.
#[derive(TypePath, Resource, Clone)]
#[repr(transparent)]
pub struct AssetServer(Arc<AssetServerData>);

// -----------------------------------------------------------------------------
// Diagnostic

impl AssetServer {
    /// Cumulative count of all load tasks started since the server was created.
    pub const STARTED_LOAD_COUNT: DiagnosticPath = DiagnosticPath::new("asset/started_load_count");
}

#[job_fn(type = AssetServerDiagnosticJob)]
fn asset_server_diagnostic_system(server: Res<AssetServer>, mut store: ResMut<Diagnostics>) {
    let started = server.0.get_started_load_tasks();
    store.add_measurement(&AssetServer::STARTED_LOAD_COUNT, || started as f64);
}

// -----------------------------------------------------------------------------
