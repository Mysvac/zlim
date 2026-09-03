//! Per-frame counters and frame diagnostics.
//!
//! [`FrameCountPlugin`] counts the completed frames of the main world in the
//! [`FrameCount`] resource — [`UpdateFrameCount`] increments it once per
//! frame in the `Last` stage.  [`FrameCountDiagnosticsPlugin`] additionally
//! samples the `frame_count`, `frame_time`, and `fps` diagnostics.

use zlim_app::{App, Last, MainSchedulePlugin, Plugin, Update};
use zlim_core::borrow::{Res, ResMut};
use zlim_core::derive::Resource;
use zlim_core::job_fn;
use zlim_core::time::{Real, Time};
use zlim_reflect::derive::TypePath;

use core::sync::atomic::{AtomicU32, Ordering};

use crate::{AppDiagnosticExt, DEFAULT_MAX_HISTORY_LENGTH, DiagnosticsPlugin};
use crate::{Diagnostic, DiagnosticPath, Diagnostics};

// -----------------------------------------------------------------------------
// FrameCount & FrameCountPlugin

/// The number of completed frames since app start.
///
/// [`FrameCount`] stores the frame counter as an atomic `u32`.
/// [`UpdateFrameCount`] increments it once per frame in the `Last` stage, so
/// after `n` calls to `App::update` the value is `n`.
///
/// Both the increment and the load use [`Ordering::Relaxed`] — no stronger
/// ordering is required, because the schedule orders the write (in `Last`)
/// before any reader of the same or a later frame.
#[derive(TypePath, Resource, Debug, Default)]
pub struct FrameCount(AtomicU32);

impl FrameCount {
    /// Returns the number of frames counted so far.
    ///
    /// Uses a relaxed load; the value is only advanced once per frame by
    /// [`UpdateFrameCount`].
    pub fn get(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Increments [`FrameCount`] once per frame.
///
/// Runs in the `Last` stage of the main schedule, after the frame's regular
/// jobs, and bumps the counter with a relaxed atomic add.
#[job_fn(type = UpdateFrameCount, name = "zlim_diagnostic::UpdateFrameCount")]
fn update_frame_count(count: Res<FrameCount>) {
    count.0.fetch_add(1, Ordering::Relaxed);
}

/// Counts the completed frames of the main world.
///
/// Registers the [`FrameCount`] resource and inserts the
/// [`UpdateFrameCount`] job into the `Last` stage, so the counter advances
/// exactly once per frame.  Pair it with
/// [`FrameCountDiagnosticsPlugin`] to expose the counter as a diagnostic.
///
/// Requires the schedules defined by [`MainSchedulePlugin`] — usually
/// already present through [`App::new`].
///
/// [`MainSchedulePlugin`]: zlim_app::MainSchedulePlugin
/// [`App::new`]: zlim_app::App::new
#[derive(Debug, Default)]
pub struct FrameCountPlugin;

impl Plugin for FrameCountPlugin {
    fn build(&self, app: &mut App) {
        MainSchedulePlugin::apply_before::<Self>(app);
    }

    fn apply(&self, app: &mut App) {
        MainSchedulePlugin::warn_if_unset(app, "FrameCountPlugin");

        let world = app.main_world_mut();

        world.init_resource::<FrameCount>();
        world.schedule_entry(Last).insert::<UpdateFrameCount>(());
    }
}

// -----------------------------------------------------------------------------
// FrameCountDiagnosticsPlugin

/// Adds the `frame_count`, `frame_time`, and `fps` diagnostics to an app.
///
/// Depends on [`FrameCountPlugin`] (added automatically if missing).  Every
/// frame in `Update` it samples the current [`FrameCount`] and the real-time
/// delta into the [`Diagnostics`] resource.
#[derive(Debug)]
pub struct FrameCountDiagnosticsPlugin {
    /// Number of samples kept in history.
    pub max_history_length: usize,
    /// Smoothing factor used by exponential moving average.
    pub smoothing_factor: f64,
}

impl Default for FrameCountDiagnosticsPlugin {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_HISTORY_LENGTH)
    }
}

impl FrameCountDiagnosticsPlugin {
    /// Frames per second.
    pub const FPS: DiagnosticPath = DiagnosticPath::new("fps");

    /// Total frames since app start.
    pub const FRAME_COUNT: DiagnosticPath = DiagnosticPath::new("frame_count");

    /// Frame time in milliseconds.
    pub const FRAME_TIME: DiagnosticPath = DiagnosticPath::new("frame_time");

    /// Creates a plugin using the provided history length.
    ///
    /// The exponential moving-average smoothing factor is derived from the
    /// history length (`2 / (length + 1)`).
    pub fn new(max_history_length: usize) -> Self {
        Self {
            max_history_length,
            smoothing_factor: 2.0 / (max_history_length as f64 + 1.0),
        }
    }
}

/// Samples `frame_count`, `frame_time`, and `fps` every update.
///
/// Runs once per frame in `Update`, after [`UpdateFrameCount`] has advanced
/// the counter in the `Last` stage of the **previous** frame.  `frame_time`
/// and `fps` are only pushed when the measured delta is non-zero.
#[job_fn(type = UpdateFrameCountDiagnostics, name = "zlim_diagnostic::UpdateFrameCountDiagnostics")]
fn diagnostic_system(
    mut diagnostics: ResMut<Diagnostics>,
    time: Res<Time<Real>>,
    frame_count: Res<FrameCount>,
) {
    diagnostics.add_measurement(&FrameCountDiagnosticsPlugin::FRAME_COUNT, || {
        frame_count.get() as f64
    });

    let delta_seconds = time.delta_secs_f64();

    if delta_seconds != 0.0 {
        diagnostics.add_measurement(&FrameCountDiagnosticsPlugin::FRAME_TIME, || {
            delta_seconds * 1000.0
        });
        diagnostics.add_measurement(&FrameCountDiagnosticsPlugin::FPS, || 1.0 / delta_seconds);
    }
}

impl Plugin for FrameCountDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        if !app.contains_plugin::<FrameCountPlugin>() {
            app.add_plugins(FrameCountPlugin);
        }
        if !app.contains_plugin::<DiagnosticsPlugin>() {
            app.add_plugins(DiagnosticsPlugin);
        }

        MainSchedulePlugin::apply_before::<Self>(app);
    }

    fn apply(&self, app: &mut App) {
        MainSchedulePlugin::warn_if_unset(app, "FrameCountDiagnosticsPlugin");

        app.register_diagnostic(
            Diagnostic::new(Self::FRAME_TIME)
                .with_suffix("ms")
                .with_max_history_length(self.max_history_length)
                .with_smoothing_factor(self.smoothing_factor),
        )
        .register_diagnostic(
            Diagnostic::new(Self::FPS)
                .with_max_history_length(self.max_history_length)
                .with_smoothing_factor(self.smoothing_factor),
        )
        .register_diagnostic(
            Diagnostic::new(Self::FRAME_COUNT)
                .with_smoothing_factor(0.0)
                .with_max_history_length(0),
        )
        .schedule_entry(Update)
        .insert::<UpdateFrameCountDiagnostics>(());
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use zlim_app::App;

    /// Verifies that [`UpdateFrameCount`] advances [`FrameCount`] exactly once
    /// per frame: after `n` calls to `App::update`, the counter reads `n`.
    ///
    /// Unlike the entity-count counter, no entities are spawned — the counter
    /// only increments, so the value is checked directly each frame.
    #[test]
    fn frame_count_increments_per_frame() {
        let mut app = App::new();
        app.add_plugins(FrameCountPlugin).build();

        for expected in 1..=100_u32 {
            app.update();
            assert_eq!(app.main_world().resource::<FrameCount>().get(), expected);
        }
    }
}
