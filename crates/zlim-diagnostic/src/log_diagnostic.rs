use core::time::Duration;

use zlim_app::{App, MainSchedulePlugin, Plugin, PostUpdate};
use zlim_core::borrow::{Res, ResMut};
use zlim_core::derive::Resource;
use zlim_core::job_fn;
use zlim_core::time::{Real, Time, Timer, TimerMode};
use zlim_log::info;
use zlim_reflect::derive::TypePath;
use zlim_utils::hash::HashSet;

use crate::{Diagnostic, DiagnosticPath, Diagnostics, DiagnosticsPlugin};

/// Mutable logging state used by [`LogDiagnosticsPlugin`].
#[derive(TypePath, Resource)]
pub struct LogDiagnosticsState {
    timer: Timer,
    filter: Option<HashSet<DiagnosticPath>>,
}

impl LogDiagnosticsState {
    /// Sets the interval used for periodic logs.
    pub fn set_timer_duration(&mut self, duration: Duration) {
        self.timer.set_duration(duration);
        self.timer.set_elapsed(Duration::ZERO);
    }

    /// Adds one path to the allow-list, returning true if inserted.
    pub fn add_filter(&mut self, diagnostic_path: DiagnosticPath) -> bool {
        if let Some(filter) = &mut self.filter {
            filter.insert(diagnostic_path)
        } else {
            self.filter = Some(HashSet::from_iter([diagnostic_path]));
            true
        }
    }

    /// Extends the allow-list with multiple paths.
    pub fn extend_filter(&mut self, iter: impl IntoIterator<Item = DiagnosticPath>) {
        if let Some(filter) = &mut self.filter {
            filter.extend(iter);
        } else {
            self.filter = Some(HashSet::from_iter(iter));
        }
    }

    /// Removes one path from the allow-list.
    pub fn remove_filter(&mut self, diagnostic_path: &DiagnosticPath) -> bool {
        if let Some(filter) = &mut self.filter {
            filter.remove(diagnostic_path)
        } else {
            false
        }
    }

    /// Clears allow-list entries while preserving filtering mode.
    pub fn clear_filter(&mut self) {
        if let Some(filter) = &mut self.filter {
            filter.clear();
        }
    }

    /// Enables filtering with an initially empty allow-list.
    pub fn enable_filtering(&mut self) {
        self.filter = Some(HashSet::new());
    }

    /// Disables filtering.
    pub fn disable_filtering(&mut self) {
        self.filter = None;
    }
}

/// An App Plugin that logs diagnostics to the console.
///
/// Diagnostics are collected by plugins such as [`FrameCountDiagnosticsPlugin`]
/// or can be provided by the user.
///
/// When no diagnostics are provided, this plugin does nothing.
///
/// [`FrameCountDiagnosticsPlugin`]: crate::FrameCountDiagnosticsPlugin
pub struct LogDiagnosticsPlugin {
    /// Time to wait between logs.
    pub wait_duration: Duration,
    /// Optional allow-list of diagnostic paths.
    pub filter: Option<HashSet<DiagnosticPath>>,
}

impl Default for LogDiagnosticsPlugin {
    fn default() -> Self {
        Self {
            wait_duration: Duration::from_secs(1),
            filter: None,
        }
    }
}

impl LogDiagnosticsPlugin {
    /// Creates a plugin that logs only diagnostics in `filter`.
    pub fn filtered(filter: HashSet<DiagnosticPath>) -> Self {
        Self {
            filter: Some(filter),
            ..Self::default()
        }
    }
}

impl LogDiagnosticsPlugin {
    fn log_diagnostic(path_width: usize, diagnostic: &Diagnostic) {
        let Some(value) = diagnostic.smoothed() else {
            return;
        };

        if diagnostic.max_history_length() == 0 {
            info!(
                target: "zlim_diagnostic",
                "{path:<path_width$}: {value:>.6}{suffix:}",
                path = diagnostic.path().as_str(),
                suffix = diagnostic.suffix(),
            );
            return;
        }

        let Some(average) = diagnostic.average() else {
            return;
        };

        info!(
            target: "zlim_diagnostic",
            // Suffix is only used for 's' or 'ms' currently,
            // so we reserve two columns for it; however,
            // Do not reserve columns for the suffix in the average
            // The ) hugging the value is more aesthetically pleasing
            "{path:<path_width$}: {value:>11.6}{suffix:2} (avg {average:>.6}{suffix:})",
            path = diagnostic.path().as_str(),
            suffix = diagnostic.suffix(),
        );
    }

    fn log_diagnostics(state: &LogDiagnosticsState, diagnostics: &Diagnostics) {
        let mut path_width = 0;

        if let Some(filter) = &state.filter {
            for path in filter {
                if let Some(diagnostic) = diagnostics.get(path)
                    && diagnostic.is_enabled
                {
                    let width = diagnostic.path().as_str().len();
                    path_width = path_width.max(width);
                }
            }
            for path in filter {
                if let Some(diagnostic) = diagnostics.get(path)
                    && diagnostic.is_enabled
                {
                    Self::log_diagnostic(path_width, diagnostic);
                }
            }
            return; // <--
        }

        // else: log all diagnostics
        for diagnostic in diagnostics.iter() {
            if diagnostic.is_enabled {
                let width = diagnostic.path().as_str().len();
                path_width = path_width.max(width);
            }
        }

        for diagnostic in diagnostics.iter() {
            if diagnostic.is_enabled {
                Self::log_diagnostic(path_width, diagnostic);
            }
        }
    }
}

#[job_fn(type = LogDiagnostics, name = "zlim_diagnostic::LogDiagnostics")]
fn log_diagnostics_system(
    mut state: ResMut<LogDiagnosticsState>,
    time: Res<Time<Real>>,
    diagnostics: Res<Diagnostics>,
) {
    if state.timer.tick(time.delta()).is_finished() {
        LogDiagnosticsPlugin::log_diagnostics(&state, &diagnostics);
    }
}

impl Plugin for LogDiagnosticsPlugin {
    fn build(&mut self, app: &mut App) {
        if !app.contains_plugin::<DiagnosticsPlugin>() {
            app.add_plugins(DiagnosticsPlugin);
        }
        MainSchedulePlugin::apply_before::<Self>(app);
    }

    fn apply(&mut self, app: &mut App) {
        MainSchedulePlugin::warn_if_unset(app, "LogDiagnosticsPlugin");

        let world = app.main_world_mut();

        world.insert_resource(LogDiagnosticsState {
            timer: Timer::new(self.wait_duration, TimerMode::Repeating),
            filter: self.filter.take(),
        });

        let schedule = world.schedule_entry(PostUpdate);
        schedule.insert::<LogDiagnostics>(());
    }
}
