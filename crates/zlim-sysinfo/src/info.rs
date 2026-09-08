//! Static host system information.

use zlim_app::Plugin;
use zlim_core::derive::Resource;
use zlim_reflect::derive::TypePath;

/// Static system information for diagnostics and profiling UI.
///
/// Populated once on registration by [`SystemInfoPlugin`].  On platforms
/// unsupported by [`sysinfo`] every field is `"Unknown"`; the `dylib` build
/// mode reads real values through the `zlim-sysinfo-dylib` isolation layer
/// (see the crate-level documentation).
///
/// [`sysinfo`]: https://crates.io/crates/sysinfo
#[derive(Debug, TypePath, Resource)]
pub struct SystemInfo {
    /// OS name and version.
    pub os: &'static str,
    /// Kernel version.
    pub kernel: &'static str,
    /// CPU model name.
    pub cpu: &'static str,
    /// Physical core count.
    pub core_count: &'static str,
    /// Total physical memory.
    pub memory: &'static str,
}

/// Registers the static [`SystemInfo`] resource.
///
/// The fields are read once (through [`Default`]) when the resource is first
/// inserted, so this is cheap after startup.
///
/// Pair it with [`SystemInfoDiagnosticsPlugin`] to also track live CPU / memory usage.
///
/// [`SystemInfoDiagnosticsPlugin`]: crate::SystemInfoDiagnosticsPlugin
#[derive(Debug, Default)]
pub struct SystemInfoPlugin;

impl Plugin for SystemInfoPlugin {
    fn apply(&mut self, app: &mut zlim_app::App) {
        app.init_resource::<SystemInfo>();
    }
}

// -----------------------------------------------------------------------------
// Implementation

// Collects real host info on the platforms `sysinfo` supports.  The concrete
// `sysinfo` re-exports come from the `sysinfo` crate itself in a normal
// build, or from the `zlim-sysinfo-dylib` isolation layer in a `dylib` build
// (the `sysinfo` dependency is then compiled into that separate dynamic
// library, so it is never linked into the engine cdylib).
//
// Unsupported targets: every field stays "Unknown".
#[cfg(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd",
))]
mod normal_impls {
    #[cfg(feature = "dylib")]
    use zlim_sysinfo_dylib as sysinfo_impls;

    #[cfg(not(feature = "dylib"))]
    use sysinfo as sysinfo_impls;

    use sysinfo_impls::{MemoryRefreshKind, RefreshKind, System};
    use zlim_utils::str::intern_str;

    use super::SystemInfo;

    impl Default for SystemInfo {
        fn default() -> Self {
            const BYTES_TO_GIB: f64 = 1.0 / 1024.0 / 1024.0 / 1024.0;

            let mem_kind = MemoryRefreshKind::nothing().with_ram();
            let kind = RefreshKind::nothing().with_memory(mem_kind);
            let sys = System::new_with_specifics(kind);

            let os_string = System::long_os_version();
            let kernel_string = System::long_os_version();
            let cpu_str = sys.cpus().first().map(|cpu| cpu.brand().trim());
            let count_str = System::physical_core_count().map(|x| x.to_string());
            let memory_string = format!("{:.1} GiB", sys.total_memory() as f64 * BYTES_TO_GIB);

            let system_info = SystemInfo {
                os: intern_str(os_string.as_deref().unwrap_or("not available")),
                kernel: intern_str(kernel_string.as_deref().unwrap_or("not available")),
                cpu: intern_str(cpu_str.unwrap_or("not available")),
                core_count: intern_str(count_str.as_deref().unwrap_or("not available")),
                // Convert from Bytes to GibiBytes since it's probably what people expect most of the time
                memory: intern_str(&memory_string),
            };

            zlim_log::info!("{system_info:?}");
            system_info
        }
    }
}

// Unsupported target: every field stays "Unknown".
#[cfg(not(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd",
)))]
mod unsupport_impls {
    use super::SystemInfo;

    impl Default for SystemInfo {
        fn default() -> Self {
            Self {
                os: "Unknown",
                kernel: "Unknown",
                cpu: "Unknown",
                core_count: "Unknown",
                memory: "Unknown",
            }
        }
    }
}
