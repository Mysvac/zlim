//! System / process CPU and memory usage diagnostics.

use zlim_diagnostic::DiagnosticPath;

// -----------------------------------------------------------------------------
// SystemInfoDiagnosticsPlugin

/// Adds system information diagnostics such as CPU and memory usage.
///
/// Gathering system information is a time-intensive task and therefore cannot
/// be done on every frame, so the sampling runs in the background task pool
/// and results are pushed into the `Diagnostics` resource of
/// `zlim-diagnostic` on a refresh interval.  Any diagnostics gathered by this
/// plugin may not be current when you access them.
///
/// Supported targets:
/// - linux
/// - windows
/// - android
/// - macOS
/// - freebsd
///
/// Works in both build modes (see the crate-level documentation): in a normal
/// build the samples come straight from the `sysinfo` dependency, while in a
/// `dylib` build they are fetched through the `zlim-sysinfo-dylib` isolation
/// layer.  On other platforms the plugin is a no-op that logs a warning.
///
/// Registered diagnostics (all percentages unless noted):
/// - [`SYSTEM_CPU_USAGE`](Self::SYSTEM_CPU_USAGE) — total system CPU usage in %.
/// - [`SYSTEM_MEM_USAGE`](Self::SYSTEM_MEM_USAGE) — total system memory usage in %.
/// - [`PROCESS_CPU_USAGE`](Self::PROCESS_CPU_USAGE) — current process CPU usage in %.
/// - [`PROCESS_MEM_USAGE`](Self::PROCESS_MEM_USAGE) — current process memory usage in GiB.
///
/// # See also
///
/// [`SystemInfoPlugin`](crate::SystemInfoPlugin) registers the static
/// [`SystemInfo`](crate::SystemInfo) resource; pair them if you also want
/// static host info.
#[derive(Debug, Default)]
pub struct SystemInfoDiagnosticsPlugin;

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

// -----------------------------------------------------------------------------
// Implementation

// Collects real measurements on the platforms `sysinfo` supports.  The
// concrete `sysinfo` re-exports come from the `sysinfo` crate itself in a
// normal build, or from the `zlim-sysinfo-dylib` isolation layer in a `dylib`
// build (the `sysinfo` dependency is then compiled into that separate
// dynamic library, so linking it into the engine cdylib never exceeds the
// linker's object limit).
#[cfg(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd",
))]
mod normal_impls {
    use core::pin::Pin;
    use core::task::{Context, Poll};
    use std::sync::Arc;

    #[cfg(feature = "dylib")]
    use zlim_sysinfo_dylib as sysinfo_impls;

    #[cfg(not(feature = "dylib"))]
    use sysinfo as sysinfo_impls;

    use sysinfo_impls::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
    use sysinfo_impls::{MINIMUM_CPU_UPDATE_INTERVAL, Pid, ProcessesToUpdate};

    use atomic_waker::AtomicWaker;
    use zlim_app::{App, First, MainSchedulePlugin, Plugin, Startup, Update};
    use zlim_core::borrow::{Res, ResMut};
    use zlim_core::command::Commands;
    use zlim_core::system::If;
    use zlim_core::world::World;
    use zlim_core::{derive::Resource, job_fn};
    use zlim_diagnostic::{Diagnostic, Diagnostics, DiagnosticsPlugin};
    use zlim_os::time::Instant;
    use zlim_reflect::derive::TypePath;
    use zlim_task::AsyncTaskPool;
    use zlim_utils::sync::ArrayQueue;

    use super::SystemInfoDiagnosticsPlugin;

    // ---------------------------------------------------------------------
    // Helper
    //
    // A single background task (`DiagnosticTask`) owns the `sysinfo::System`
    // handle and refreshes it at most once per
    // `sysinfo::MINIMUM_CPU_UPDATE_INTERVAL`.  Results are pushed through a
    // small lock-free queue into the `SysinfoTask` resource; per-frame jobs
    // wake the task (`WakeDiagnosticsTask` in `First`) and drain the queue
    // into `Diagnostics` (`ReadDiagnosticsTask` in `Update`).

    #[derive(TypePath, Resource)]
    struct SysinfoTask {
        _task: zlim_task::Task<()>,
        queue: Arc<ArrayQueue<SysinfoRefreshData>>,
        waker: Arc<AtomicWaker>,
    }

    /// One refresh of all four measured values.
    struct SysinfoRefreshData {
        system_cpu_usage: f64,
        system_mem_usage: f64,
        process_cpu_usage: f64,
        process_mem_usage: f64,
    }

    /// The background future that periodically samples the system.
    struct DiagnosticTask {
        pid: Pid,
        system: System,
        last_refresh: Instant,
        sender: Arc<ArrayQueue<SysinfoRefreshData>>,
        waker: Arc<AtomicWaker>,
    }

    // ---------------------------------------------------------------------
    // Helper Implementation

    impl SysinfoRefreshData {
        fn new(system: &mut System, pid: Pid) -> Self {
            system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
            system.refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage());
            system.refresh_memory();

            let system_cpu_usage: f64 = system.global_cpu_usage() as f64;
            let total_mem: f64 = system.total_memory() as f64;
            let used_mem: f64 = system.used_memory() as f64;
            let system_mem_usage: f64 = used_mem / total_mem * 100.0;

            let mut process_mem_usage: f64 = 0.0;
            let mut process_cpu_usage: f64 = 0.0;

            const BYTES_TO_GIB: f64 = 1.0 / 1024.0 / 1024.0 / 1024.0;

            if let Some(p) = system.process(pid) {
                process_mem_usage = p.memory() as f64 * BYTES_TO_GIB;
                process_cpu_usage = p.cpu_usage() as f64 / system.cpus().len() as f64;
            }

            Self {
                system_cpu_usage,
                system_mem_usage,
                process_cpu_usage,
                process_mem_usage,
            }
        }
    }

    impl DiagnosticTask {
        fn new(pid: Pid, queue: Arc<ArrayQueue<SysinfoRefreshData>>) -> Self {
            let kind = RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
                .with_memory(MemoryRefreshKind::everything());

            Self {
                pid,
                system: System::new_with_specifics(kind),
                // Avoids initial delay on first refresh
                last_refresh: Instant::now() - MINIMUM_CPU_UPDATE_INTERVAL,
                sender: queue,
                waker: Arc::new(AtomicWaker::new()),
            }
        }
    }

    impl Future for DiagnosticTask {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            self.waker.register(cx.waker());

            if self.last_refresh.elapsed() > MINIMUM_CPU_UPDATE_INTERVAL {
                self.last_refresh = Instant::now();

                let pid = self.pid;
                let data = SysinfoRefreshData::new(&mut self.system, pid);
                let _ = self.sender.force_push(data);
            }

            // Always reschedules
            Poll::Pending
        }
    }

    // ---------------------------------------------------------------------
    // Jobs

    /// Registers the four system-info diagnostics (with their units).
    #[cold]
    fn initialize_sysinfo_resource(world: &mut World) {
        let diagnostics = world.resource_mut_or_init::<Diagnostics>().into_inner();

        diagnostics
            .add(Diagnostic::new(SystemInfoDiagnosticsPlugin::SYSTEM_CPU_USAGE).with_suffix("%"));
        diagnostics
            .add(Diagnostic::new(SystemInfoDiagnosticsPlugin::SYSTEM_MEM_USAGE).with_suffix("%"));
        diagnostics
            .add(Diagnostic::new(SystemInfoDiagnosticsPlugin::PROCESS_CPU_USAGE).with_suffix("%"));
        diagnostics.add(
            Diagnostic::new(SystemInfoDiagnosticsPlugin::PROCESS_MEM_USAGE).with_suffix("GiB"),
        );
    }

    /// Spawns the background sampling task (`Startup`) and stores a handle to
    /// it (`SysinfoTask`).
    #[job_fn(type = InitDiagnosticsTask, name = "zlim_sysinfo::InitDiagnosticsTask")]
    fn init_diagnostic_task(mut commands: Commands) {
        let pid = match sysinfo_impls::get_current_pid() {
            Ok(pid) => pid,
            Err(e) => {
                zlim_log::warn!(
                    "Failed to get current process ID: {e}. \
                    `SystemInfoDiagnosticsPlugin` will be skipped, \
                    and internal records will remain empty."
                );
                return;
            }
        };

        let queue = Arc::new(ArrayQueue::new(2));
        let diagnostic_task = DiagnosticTask::new(pid, queue.clone());
        let waker = Arc::clone(&diagnostic_task.waker);
        let _task = AsyncTaskPool::get().spawn(diagnostic_task);

        let sysinfo_task = SysinfoTask {
            _task,
            queue,
            waker,
        };

        commands.insert_resource(sysinfo_task);
    }

    /// Wakes the background task once per frame (`First`) so it refreshes as
    /// soon as its minimum interval has elapsed.
    #[job_fn(type = WakeDiagnosticsTask, name = "zlim_sysinfo::WakeDiagnosticsTask")]
    fn wake_diagnostic_task(task: If<Res<SysinfoTask>>) {
        task.waker.wake();
    }

    /// Drains any fresh samples from the background task into `Diagnostics`
    /// (`Update`).
    #[job_fn(type = ReadDiagnosticsTask, name = "zlim_sysinfo::ReadDiagnosticsTask")]
    fn read_diagnostic_task(mut diagnostics: If<ResMut<Diagnostics>>, task: Res<SysinfoTask>) {
        while let Some(data) = task.queue.pop() {
            let diagnostics: &mut Diagnostics = diagnostics.0.as_mut();
            diagnostics.add_measurement(&SystemInfoDiagnosticsPlugin::SYSTEM_CPU_USAGE, || {
                data.system_cpu_usage
            });
            diagnostics.add_measurement(&SystemInfoDiagnosticsPlugin::SYSTEM_MEM_USAGE, || {
                data.system_mem_usage
            });
            diagnostics.add_measurement(&SystemInfoDiagnosticsPlugin::PROCESS_CPU_USAGE, || {
                data.process_cpu_usage
            });
            diagnostics.add_measurement(&SystemInfoDiagnosticsPlugin::PROCESS_MEM_USAGE, || {
                data.process_mem_usage
            });
        }
    }

    // ---------------------------------------------------------------------
    // Plugin

    impl Plugin for SystemInfoDiagnosticsPlugin {
        fn build(&self, app: &mut App) {
            if !app.contains_plugin::<DiagnosticsPlugin>() {
                app.add_plugins(DiagnosticsPlugin);
            }
            MainSchedulePlugin::apply_before::<Self>(app);
        }

        fn apply(&self, app: &mut App) {
            MainSchedulePlugin::warn_if_unset(app, "SystemInfoDiagnosticsPlugin");

            let world = app.main_world_mut();
            initialize_sysinfo_resource(world);

            world
                .schedule_entry(Startup)
                .insert::<InitDiagnosticsTask>(());
            world
                .schedule_entry(First)
                .insert::<WakeDiagnosticsTask>(());
            world
                .schedule_entry(Update)
                .insert::<ReadDiagnosticsTask>(());
        }
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd",
)))]
mod unsupport_impls {
    use super::SystemInfoDiagnosticsPlugin;
    use zlim_app::{App, Plugin};

    impl Plugin for SystemInfoDiagnosticsPlugin {
        fn apply(&self, _app: &mut App) {
            zlim_log::warn!(
                "Current platform does not support SystemInfoDiagnosticsPlugin; \
                the plugin has been disabled."
            );
        }
    }
}
