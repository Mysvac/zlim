# zlim-log

A logging library built on [`tracing`](https://crates.io/crates/tracing) with
a built-in [`log`](https://crates.io/crates/log) bridge.

## Initializing the Global Logger

Use `LogConfig` to initialize the logger:

```rust
use zlim_log::{LogConfig, info};

// Call once at startup to install the global logger and tracing subscriber.
LogConfig::default().apply();
```

## Configurable Parameters

| Field | Purpose |
|-------|---------|
| `filter` | Content filter |
| `level` | Global minimum log level |
| `format_layer` | Override the default formatting output layer |
| `custom_layer` | Append a custom [`Layer`] |

### Content Filter

Uses the `Targets` syntax, controlling which logs are emitted by
**target + level**. For example:

```rust
use zlim_log::LogConfig;

let config = LogConfig {
    filter: "wgpu=warn,naga=warn,zlim_core=debug".to_string(),
    ..Default::default()
};
```

Allows spaces, e.g. `"wgpu= warn ,naga =warn"`; leading and trailing whitespace
is properly trimmed.

When unset, the default filter is used; see the internal code for details.

`LogConfig`'s internal `filter` is merged with the `RUST_LOG` environment
variable.

We use the simpler `Targets` filter rather than the more powerful `EnvFilter`
in order to optimize logging performance as much as possible.

### Log Level

`level` sets the global minimum output level (`TRACE` / `DEBUG` / `INFO` /
`WARN` / `ERROR`).

- Defaults to `Level::DEBUG` in debug builds.
- Defaults to `Level::INFO` in release builds.

### FormatLayer

`format_layer: Option<BoxedFormatLayer>` is used to override the default
formatting output layer on desktop.

On platforms such as macOS, Android, and WASM, `format_layer` has no effect;
they have platform-specific log output.

On regular platforms (Windows, Linux, etc.), `format_layer` defaults to
outputting log messages to `stderr`. If the user explicitly provides a
`format_layer`, it **replaces** the default.

`format_layer` is the innermost layer of the subscriber, so it is the layer
that turns an event into output.

### CustomLayer

`custom_layer: Option<BoxedCustomLayer>` lets you append a custom
[`Layer`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/trait.Layer.html)
to the subscriber stack.

`custom_layer` does not override the implementation of other layers; it is
empty by default.

`custom_layer` is applied outside `format_layer`, so its layer type has to name
the subscriber the format layer is attached to; see [Layer Order](#layer-order).

## Layer Order

The subscriber is built by chaining layers onto a `Registry`, from the
innermost layer to the outermost one:

| # | Layer | Condition |
|---|-------|-----------|
| 1 | `format_layer`, or the layer of the platform | always |
| 2 | `custom_layer` | when it is set |
| 3 | Chrome tracing (`tracing-chrome`) | `trace_chrome` |
| 4 | `ErrorLayer` of `tracing-error` | `trace_error` |
| 5 | The `Targets` filter built from `filter` and `level` | always |

Every layer wraps the ones chained before it, which is why the type of a layer
has to name the subscriber it is attached to: `custom_layer` is a
`BoxedCustomLayer`, which names `Layered<BoxedFormatLayer, Registry>` rather
than `Registry`. The filter is chained last, so it is the outermost layer and
the one that decides which events reach the layers below it.

## Features

- `default_log_level`: Controls the static log level, eliminating
  unnecessary log statements at compile time to speed up third-party crates.
  In debug mode, the `log` crate is limited to `info`, and the `tracing`
  crate is limited to `debug`. In release mode, the `log` crate is limited
  to `warn`, and the `tracing` crate is limited to `info`. This library's
  macros are re-exported from `tracing` and are not affected by `log`.

- `debug`: Adjusts `LogConfig`'s default log level to `Debug`; overridable,
  no special effect.

- `trace_error`: Enables `tracing-error`, records error span stacks via
  `ErrorLayer`, and modifies the panic hook to print the `SpanTrace` on
  panic.

- `trace_chrome`: Enables `tracing-chrome`, exporting the Chrome tracing
  format (JSON); use the `TRACE_CHROME` environment variable to specify the
  output file path.

The Tracy profiler is no longer part of this crate; use `zlim-tracy` for it.

## Logging Macros

This library re-exports most of the `tracing` crate's logging macros:

- `trace!` / `debug!` / `info!` / `warn!` / `error!`

- `trace_span!` / `debug_span!` / `info_span!` / `warn_span!` / `error_span!`

Additionally, this library provides the `once` series, which logs only when
the corresponding statement is executed for the first time:

- `trace_once!` / `debug_once!` / `info_once!` / `warn_once!` / `error_once!`

---

Modified from [bevy_log](https://github.com/bevyengine/bevy/tree/main/crates/bevy_log).
