# zlim-sysinfo

System information collection and diagnostics, adapted from `bevy_diagnostic`.

## `SystemInfo` / `SystemInfoPlugin`

Static host information (OS / kernel / CPU model / core count / total
memory) stored in a `Resource`, populated once at startup.

## `SystemInfoDiagnosticsPlugin`

Samples CPU / memory usage in the background and pushes them into the
`Diagnostics` resource of `zlim-diagnostic`:

- `SYSTEM_CPU_USAGE` — total system CPU usage in %.
- `SYSTEM_MEM_USAGE` — total system memory usage in %.
- `PROCESS_CPU_USAGE` — current process CPU usage in %.
- `PROCESS_MEM_USAGE` — current process memory usage in GiB.

Sampling runs in a background task at most once per
`sysinfo::MINIMUM_CPU_UPDATE_INTERVAL`, so values may not be current when
read.

## Supported targets

linux / windows / android / macOS / freebsd. On other targets the plugins
are no-ops that log a warning and `SystemInfo` keeps its `"Unknown"` fields.

## Feature flags

- `dylib` — depend on the `zlim-sysinfo-dylib` isolation layer and route
  the sysinfo re-exports through it instead of the `sysinfo` crate directly.
