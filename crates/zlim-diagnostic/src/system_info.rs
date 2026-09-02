use zlim_app::{App, Plugin};
use zlim_core::derive::Resource;
use zlim_reflect::derive::TypePath;

use crate::{DiagnosticPath, DiagnosticsPlugin};

/// Adds system information diagnostics such as CPU and memory usage.
///
/// Supported targets:
/// - linux
/// - windows
/// - android
/// - macOS
#[derive(Default)]
pub struct SystemInfoDiagnosticsPlugin;

impl Plugin for SystemInfoDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        if !app.contains_plugin::<DiagnosticsPlugin>() {
            app.add_plugins(DiagnosticsPlugin);
        }
    }
    fn apply(&self, app: &mut App) {
        internal::setup_plugin(app);
    }
}

impl SystemInfoDiagnosticsPlugin {
    /// Total system CPU usage in percent.
    pub const SYSTEM_CPU_USAGE: DiagnosticPath = DiagnosticPath::new("system/cpu_usage");
    /// Total system memory usage in percent.
    pub const SYSTEM_MEM_USAGE: DiagnosticPath = DiagnosticPath::new("system/mem_usage");
    /// Current process CPU usage in percent.
    pub const PROCESS_CPU_USAGE: DiagnosticPath = DiagnosticPath::new("process/cpu_usage");
    /// Current process memory usage in GiB.
    pub const PROCESS_MEM_USAGE: DiagnosticPath = DiagnosticPath::new("process/mem_usage");
}

/// Static system information for diagnostics and profiling UI.
#[derive(Debug, TypePath, Resource)]
pub struct SystemInfo {
    /// OS name and version.
    pub os: String,
    /// Kernel version.
    pub kernel: String,
    /// CPU model name.
    pub cpu: String,
    /// Physical core count.
    pub core_count: String,
    /// Total physical memory.
    pub memory: String,
}

#[cfg(all(
    not(feature = "dylib"),
    any(
        target_os = "linux",
        target_os = "windows",
        target_os = "android",
        target_os = "macos"
    )
))]
mod internal {
    use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
    use zlim_app::{App, MainSchedulePlugin, Update};
    use zlim_core::resource::Resource;
    use zlim_core::{borrow::ResMut, job_fn};
    use zlim_os::time::Instant;
    use zlim_reflect::derive::TypePath;

    use super::{SystemInfo, SystemInfoDiagnosticsPlugin};
    use crate::{AppDiagnosticExt, Diagnostic, Diagnostics};

    const BYTES_TO_GIB: f64 = 1.0 / 1024.0 / 1024.0 / 1024.0;

    #[derive(TypePath, Resource)]
    struct SysinfoState {
        system: System,
        last_refresh: Instant,
    }

    impl Default for SysinfoState {
        fn default() -> Self {
            Self {
                system: System::new_with_specifics(
                    RefreshKind::nothing()
                        .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
                        .with_memory(MemoryRefreshKind::everything()),
                ),
                // Avoid initial delay on first sampling.
                last_refresh: Instant::now() - sysinfo::MINIMUM_CPU_UPDATE_INTERVAL,
            }
        }
    }

    pub(super) fn setup_plugin(app: &mut App) {
        MainSchedulePlugin::warn_if_unset(app, "SystemInfoDiagnosticsPlugin");
        app.register_diagnostic(
            Diagnostic::new(SystemInfoDiagnosticsPlugin::SYSTEM_CPU_USAGE).with_suffix("%"),
        )
        .register_diagnostic(
            Diagnostic::new(SystemInfoDiagnosticsPlugin::SYSTEM_MEM_USAGE).with_suffix("%"),
        )
        .register_diagnostic(
            Diagnostic::new(SystemInfoDiagnosticsPlugin::PROCESS_CPU_USAGE).with_suffix("%"),
        )
        .register_diagnostic(
            Diagnostic::new(SystemInfoDiagnosticsPlugin::PROCESS_MEM_USAGE).with_suffix("GiB"),
        )
        .init_resource::<SysinfoState>()
        .schedule_entry(Update)
        .insert::<UpdateSystemInfoDiagnostics>(());
    }

    #[job_fn(type = UpdateSystemInfoDiagnostics)]
    fn update_system_information_diagnostics(
        mut diagnostics: ResMut<Diagnostics>,
        mut state: ResMut<SysinfoState>,
    ) {
        if state.last_refresh.elapsed() < sysinfo::MINIMUM_CPU_UPDATE_INTERVAL {
            return;
        }

        state.last_refresh = Instant::now();

        let pid = match sysinfo::get_current_pid() {
            Ok(pid) => pid,
            Err(_) => return,
        };

        state
            .system
            .refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
        state
            .system
            .refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage());
        state.system.refresh_memory();

        let system_cpu_usage = state.system.global_cpu_usage() as f64;
        let total_mem = state.system.total_memory() as f64;
        let used_mem = state.system.used_memory() as f64;
        let system_mem_usage = if total_mem > 0.0 {
            used_mem / total_mem * 100.0
        } else {
            0.0_f64
        };

        let process_mem_usage = state
            .system
            .process(pid)
            .map(|p| p.memory() as f64 * BYTES_TO_GIB)
            .unwrap_or(0.0);

        let process_cpu_usage = state
            .system
            .process(pid)
            .map(|p| p.cpu_usage() as f64 / state.system.cpus().len().max(1) as f64)
            .unwrap_or(0.0);

        diagnostics.add_measurement(&SystemInfoDiagnosticsPlugin::SYSTEM_CPU_USAGE, || {
            system_cpu_usage
        });
        diagnostics.add_measurement(&SystemInfoDiagnosticsPlugin::SYSTEM_MEM_USAGE, || {
            system_mem_usage
        });
        diagnostics.add_measurement(&SystemInfoDiagnosticsPlugin::PROCESS_CPU_USAGE, || {
            process_cpu_usage
        });
        diagnostics.add_measurement(&SystemInfoDiagnosticsPlugin::PROCESS_MEM_USAGE, || {
            process_mem_usage
        });
    }

    impl Default for SystemInfo {
        fn default() -> Self {
            let system = System::new_with_specifics(
                RefreshKind::nothing()
                    .with_cpu(CpuRefreshKind::nothing())
                    .with_memory(MemoryRefreshKind::nothing().with_ram()),
            );

            let system_info = Self {
                os: System::long_os_version().unwrap_or_else(|| String::from("not available")),
                kernel: System::kernel_version().unwrap_or_else(|| String::from("not available")),
                cpu: system
                    .cpus()
                    .first()
                    .map(|cpu| cpu.brand().trim().to_string())
                    .unwrap_or_else(|| String::from("not available")),
                core_count: System::physical_core_count()
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| String::from("not available")),
                memory: format!("{:.1} GiB", system.total_memory() as f64 * BYTES_TO_GIB),
            };

            zlim_log::info!(target: "zlim_diagnostic", "{system_info:?}");
            system_info
        }
    }
}

#[cfg(not(all(
    not(feature = "dylib"),
    any(
        target_os = "linux",
        target_os = "windows",
        target_os = "android",
        target_os = "macos"
    )
)))]
mod internal {
    use super::SystemInfo;
    use zlim_app::App;

    pub(super) fn setup_plugin(_app: &mut App) {
        zlim_log::warn!(
            target: "zlim_diagnostic",
            "SystemInfoDiagnosticsPlugin is not supported on this platform/configuration"
        );
    }

    impl Default for SystemInfo {
        fn default() -> Self {
            let unknown = "Unknown";
            Self {
                os: unknown.into(),
                kernel: unknown.into(),
                cpu: unknown.into(),
                core_count: unknown.into(),
                memory: unknown.into(),
            }
        }
    }
}
