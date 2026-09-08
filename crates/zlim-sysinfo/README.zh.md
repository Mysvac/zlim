# zlim-sysinfo

系统信息收集与诊断，改自`bevy_diagnostic`。

## `SystemInfo` / `SystemInfoPlugin`

静态主机信息（OS / 内核 / CPU 型号 / 核心数 / 总内存）存入 `Resource`，
在启动时填充一次。

## `SystemInfoDiagnosticsPlugin`

后台采样 CPU / 内存占用，并推入 `zlim-diagnostic` 的 `Diagnostics` 资源：

- `SYSTEM_CPU_USAGE` — 系统总 CPU 占用（%）。
- `SYSTEM_MEM_USAGE` — 系统总内存占用（%）。
- `PROCESS_CPU_USAGE` — 当前进程 CPU 占用（%）。
- `PROCESS_MEM_USAGE` — 当前进程内存占用（GiB）。

采样在后台任务中进行，间隔不低于 `sysinfo::MINIMUM_CPU_UPDATE_INTERVAL`，
因此读取到的值可能不是最新的。

## 支持平台

linux / windows / android / macOS / freebsd。其他平台上插件为空操作
（仅记录告警），`SystemInfo` 保持 `"Unknown"` 字段。

## Feature

- `dylib` —— 依赖 `zlim-sysinfo-dylib` 隔离层，将 sysinfo 的重导出改经它
  而非直接使用 `sysinfo` crate，避免对象数溢出。
