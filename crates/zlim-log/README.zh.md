# zlim-log

日志库，基于[`tracing`](https://crates.io/crates/tracing)，内置了[`log`](https://crates.io/crates/log) 桥接。

## 初始化全局日志

使用 [`LogPlugin`](zlim_log::LogPlugin) 初始化日志器：

```rust
use zlim_log::{LogPlugin, info};

// 在启动时调用一次,安装全局 logger 与 tracing subscriber。
LogPlugin::default().apply();
```

## 可配置参数

| 字段 | 作用 |
|------|------|
| `filter` | 内容过滤器 |
| `level` | 全局最低日志级别 |
| `custom_layer` | 追加一个自定义 [`Layer`] |
| `format_layer` | 覆盖默认的格式化输出层 |
| `enable_tracy` | 是否启用 Tracy 流式采集 |

### 内容过滤器

使用 `EnvFilter` 语法(与 `RUST_LOG` 相同)，按 **target + 级别**控制哪些日志会输出，例如:

```rust
let plugin = LogPlugin {
    filter: "wgpu=warn,naga=warn,zlim_core=debug".to_string(),
    ..Default::default()
};
```

未指定时使用默认过滤器，请参考内部代码。

`LogPlugin` 内部的 `filter` 会与环境变量中的 `RUST_LOG` 合并。

### 日志级别

`level` 用于设置全局最低输出级别(`TRACE` / `DEBUG` / `INFO` / `WARN` / `ERROR`)。

- debug 构建时默认为 `Level::DEBUG`。
- release 构建时默认为 `Level::INFO`。

### CustomLayer

`custom_layer: Option<BoxedLayer>` 允许你向 subscriber 栈追加一层自定义
[`Layer`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/trait.Layer.html)。


`custom_layer` 不会覆盖其他 Layer 的实现，默认为空。

### FormatLayer

`format_layer: Option<BoxedFmtLayer>` 用于覆盖桌面端默认的格式化输出层。

在 macos、android、wasm 等平台，format_layer 不会生效，它们有平台特定的日志输出。

在常规平台（win、linux等），format_layer 默认会向 `stderr` 输出日志信息。
如果此时用户显式提供了 format_layer，则会**替换**默认值。

### EnableTracy

用于控制 `trace_tracy` 是否实际启用。

当 `trace_tracy` feature 未启用时，此字段被忽略。

详细内容请看下方的 Feature 部分。

## Features

- `default_log_level`: 用于控制的静态日志级别，编译期消除日志不必要的日志语句，以加速第三方库的执行。
  debug 模式时，`log` crate 被限制为 `info`，`tracing` crate 被限制为 `debug` 。
  release 模式时，`log` crate 被限制为 `warn`，`tracing` crate 被限制为 `info` 。
  本库的宏重导出于 `tracing`，不受 `log` 影响。 

- `debug`: 调整 `LogPlugin` 的默认日志级别为 `Debug`，可被覆盖，没什么特殊效果。

- `trace`: 启用 `tracing-error`，通过 `ErrorLayer` 记录错误 span 栈，修改 panic hook，panic 时打印 `SpanTrace`。

- `trace_tracy`: 启用 `tracing-tracy` crate，用于性能分析。当 `LogPlugin` 的 `enable_tracy` 字段为 `true` 时，向 Tracy 流式发送事件。

- `trace_memory`: 启用 `tracy-client`，以支持 Tracy 内存分析。

- `trace_chrome`:  启用 `tracing-chrome`,导出 Chrome tracing 格式(JSON)，用环境变量 `TRACE_CHROME` 指定输出文件路径。

## 日志宏

本库重导出了 `tracing` crate 的大部分日志宏：

- `trace!` / `debug!` / `info!` / `warn!` / `error!`

- `trace_span!` / `debug_span!` / `info_span!` / `warn_span!` / `error_span!`

此外，本库额外提供了 `once` 系列，进行对应语句第一次被执行时输出日志。

- `trace_once!` / `debug_once!` / `info_once!` / `warn_once!` / `error_once!`

---

修改自 bevy_log 。
