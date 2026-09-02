# zlim-diagnostic

Diagnostics support for the zlim engine, adapted from
[`bevy_diagnostic`](https://github.com/bevyengine/bevy/tree/main/crates/bevy_diagnostic).

## `DiagnosticsPlugin`

The core plugin. Its only job is to register the `Diagnostics` resource — a
store of named metrics. Each metric (`Diagnostic`) keeps a history of
timestamped samples and provides latest / average / smoothed values. With the
`sysinfo_plugin` feature it also registers the `SystemInfo` resource.

Actual metric values are pushed either by your own jobs (via
`Diagnostics::add_measurement`) or by the built-in plugins below.

## Built-in plugins

### `EntityCountPlugin` / `EntityCountDiagnosticsPlugin`

Tracks the alive-entity count of the main world once per frame; exposes an
`entity_count` diagnostic.

### `FrameCountPlugin` / `FrameCountDiagnosticsPlugin`

Counts completed frames; exposes `frame_count`, `frame_time` (ms) and `fps`
diagnostics.

### `LogDiagnosticsPlugin`

Logs enabled diagnostics to the console on a timer, with optional path
filtering.

### `SystemInfoDiagnosticsPlugin`

CPU / memory usage of system and process (`sysinfo_plugin` feature; supported
on linux / windows / android / macOS).
