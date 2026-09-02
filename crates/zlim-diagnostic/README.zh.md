# zlim-diagnostic

zlim 引擎的基础诊断支持，改自 bevy_diagnostic。

## `DiagnosticsPlugin`

核心插件，唯一职责是注册 `Diagnostics` 资源 —— 一个具名指标的存储。

每个指标（`Diagnostic`）保存带时间戳的采样历史，并提供最新值 / 平均值 / 平滑值。
启用 `sysinfo_plugin` feature 时还会注册 `SystemInfo` 资源。

实际的指标数值由你自己的 job（通过 `Diagnostics::add_measurement`）或下列内置
插件写入。

## 内置插件

### `EntityCountPlugin` / `EntityCountDiagnosticsPlugin`

每帧跟踪主世界存活实体数，并提供 `entity_count` 诊断。

### `FrameCountPlugin` / `FrameCountDiagnosticsPlugin`

统计已完成的帧数，并提供 `frame_count`、`frame_time`（毫秒）与 `fps` 诊断。

### `LogDiagnosticsPlugin`

定时将启用的诊断输出到控制台，支持按路径过滤。

### `SystemInfoDiagnosticsPlugin`

系统与进程的 CPU / 内存占用（`sysinfo_plugin` feature；支持 linux / windows /
android / macOS）。
