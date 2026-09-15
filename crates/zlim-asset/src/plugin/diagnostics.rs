use zlim_app::{App, MainSchedulePlugin, Plugin, PluginExt, Update};
use zlim_diagnostic::{AppDiagnosticExt, Diagnostic, DiagnosticsPlugin};

use super::AssetPlugin;
use crate::server::{AssetServer, AssetServerDiagnosticJob};

/// Adds the asset server diagnostics to an [`App`].
///
/// No setup is needed beforehand: [`DiagnosticsPlugin`] is added when it is missing, and
/// registering a diagnostic creates the `Diagnostics` resource on demand.
#[derive(Debug, Default)]
pub struct AssetDiagnosticsPlugin;

impl Plugin for AssetDiagnosticsPlugin {
    fn build(&mut self, app: &mut App) {
        if !app.contains_plugin::<DiagnosticsPlugin>() {
            app.add_plugins(DiagnosticsPlugin);
            zlim_log::info!(
                "`DiagnosticsPlugin` was added as a dependency af `AssetDiagnosticsPlugin`"
            );
        }

        MainSchedulePlugin::apply_before::<Self>(app);
    }

    fn apply(&mut self, app: &mut App) {
        MainSchedulePlugin::warn_if_unset(app, "AssetDiagnosticsPlugin");
        if !app.contains_plugin::<AssetPlugin>() {
            zlim_log::warn!(
                "`AssetDiagnosticsPlugin` is added but missing \
                `AssetPlugin` diagnostic jobs may be ignored."
            );
        }

        app.register_diagnostic(
            Diagnostic::new(AssetServer::STARTED_LOAD_COUNT)
                .with_suffix(" loads")
                .with_smoothing_factor(0.0)
                .with_max_history_length(0),
        );

        app.add_job::<AssetServerDiagnosticJob>(Update, ());
    }
}
