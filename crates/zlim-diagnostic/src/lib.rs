#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]

// -----------------------------------------------------------------------------
// Config

/// Default max history length for new diagnostics.
pub const DEFAULT_MAX_HISTORY_LENGTH: usize = 120;

// -----------------------------------------------------------------------------
// Diagnostics

mod diagnostic;

pub use diagnostic::{AppDiagnosticExt, Diagnostic, Diagnostics};
pub use diagnostic::{DiagnosticMeasurement, DiagnosticPath};

// -----------------------------------------------------------------------------
// EntityCount

mod entity_count;
pub use entity_count::EntityCountDiagnosticsPlugin;
pub use entity_count::{EntityCount, EntityCountPlugin};
pub use entity_count::{UpdateEntityCount, UpdateEntityCountDiagnostics};

// -----------------------------------------------------------------------------
// FrameCount

mod frame_count;
pub use frame_count::FrameCountDiagnosticsPlugin;
pub use frame_count::{FrameCount, FrameCountPlugin};
pub use frame_count::{UpdateFrameCount, UpdateFrameCountDiagnostics};

// -----------------------------------------------------------------------------
// Log

mod log_diagnostic;
pub use log_diagnostic::{LogDiagnosticsPlugin, LogDiagnosticsState};

// -----------------------------------------------------------------------------
// Plugin

/// Adds core diagnostics resources to an App.
#[derive(Default)]
pub struct DiagnosticsPlugin;

impl zlim_app::Plugin for DiagnosticsPlugin {
    fn apply(&self, app: &mut zlim_app::App) {
        app.init_resource::<Diagnostics>();
    }
}

// -----------------------------------------------------------------------------
