# zlim-log

A logging library built on [`tracing`](https://crates.io/crates/tracing) with
a built-in [`log`](https://crates.io/crates/log) bridge.

## Initializing the Global Logger

Use [`LogPlugin`](zlim_log::LogPlugin) to initialize the logger:

```rust
use zlim_log::{LogPlugin, info};

// Call once at startup to install the global logger and tracing subscriber.
LogPlugin::default().apply();
```

## Configurable Parameters

| Field | Purpose |
|-------|---------|
| `filter` | Content filter |
| `level` | Global minimum log level |
| `custom_layer` | Append a custom [`Layer`] |
| `format_layer` | Override the default formatting output layer |
| `enable_tracy` | Whether to enable Tracy streaming |

### Content Filter

Uses the `EnvFilter` syntax (the same as `RUST_LOG`), controlling which logs
are emitted by **target + level**. For example:

```rust
use zlim_log::LogPlugin;

let plugin = LogPlugin {
    filter: "wgpu=warn,naga=warn,zlim_core=debug".to_string(),
    ..Default::default()
};
```

When unset, the default filter is used; see the internal code for details.

`LogPlugin`'s internal `filter` is merged with the `RUST_LOG` environment
variable.

### Log Level

`level` sets the global minimum output level (`TRACE` / `DEBUG` / `INFO` /
`WARN` / `ERROR`).

- Defaults to `Level::DEBUG` in debug builds.
- Defaults to `Level::INFO` in release builds.

### CustomLayer

`custom_layer: Option<BoxedLayer>` lets you append a custom
[`Layer`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/trait.Layer.html)
to the subscriber stack.

`custom_layer` does not override the implementation of other layers; it is
empty by default.

### FormatLayer

`format_layer: Option<BoxedFmtLayer>` is used to override the default
formatting output layer on desktop.

On platforms such as macOS, Android, and WASM, `format_layer` has no effect;
they have platform-specific log output.

On regular platforms (Windows, Linux, etc.), `format_layer` defaults to
outputting log messages to `stderr`. If the user explicitly provides a
`format_layer`, it **replaces** the default.

### EnableTracy

Controls whether `trace_tracy` is actually enabled.

This field is ignored when the `trace_tracy` feature is not enabled.

See the Feature section below for details.

## Features

- `default_log_level`: Controls the static log level, eliminating
  unnecessary log statements at compile time to speed up third-party crates.
  In debug mode, the `log` crate is limited to `info`, and the `tracing`
  crate is limited to `debug`. In release mode, the `log` crate is limited
  to `warn`, and the `tracing` crate is limited to `info`. This library's
  macros are re-exported from `tracing` and are not affected by `log`.

- `debug`: Adjusts `LogPlugin`'s default log level to `Debug`; overridable,
  no special effect.

- `trace`: Enables `tracing-error`, records error span stacks via
  `ErrorLayer`, and modifies the panic hook to print the `SpanTrace` on
  panic.

- `trace_tracy`: Enables the `tracing-tracy` crate for profiling. When
  `LogPlugin`'s `enable_tracy` field is `true`, events are streamed to
  Tracy.

- `trace_memory`: Enables `tracy-client` to support Tracy memory profiling.

- `trace_chrome`: Enables `tracing-chrome`, exporting the Chrome tracing
  format (JSON); use the `TRACE_CHROME` environment variable to specify the
  output file path.

## Logging Macros

This library re-exports most of the `tracing` crate's logging macros:

- `trace!` / `debug!` / `info!` / `warn!` / `error!`

- `trace_span!` / `debug_span!` / `info_span!` / `warn_span!` / `error_span!`

Additionally, this library provides the `once` series, which logs only when
the corresponding statement is executed for the first time:

- `trace_once!` / `debug_once!` / `info_once!` / `warn_once!` / `error_once!`

---

Modified from [bevy_log](https://github.com/bevyengine/bevy/tree/main/crates/bevy_log).
