#![doc = include_str!("../README.md")]

use core::fmt::{Debug, Formatter};

use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::Layered;
use tracing_subscriber::{Layer, Registry};

// -----------------------------------------------------------------------------
// Modules

#[cfg(target_os = "android")]
mod android_layer;

#[cfg(feature = "trace_chrome")]
mod chrome_layer;

mod macros;

// -----------------------------------------------------------------------------
// Re-exoprt

pub use tracing::span::EnteredSpan;
pub use tracing::{Event, Level, Span};
pub use tracing::{debug, debug_span};
pub use tracing::{error, error_span};
pub use tracing::{info, info_span};
pub use tracing::{trace, trace_span};
pub use tracing::{warn, warn_span};

pub use tracing;
pub use tracing_subscriber;

// -----------------------------------------------------------------------------
// Alias

/// A [`Layer`] that replaces the default formatting output layer.
///
/// It is attached to the [`Registry`] itself, because it is the innermost layer
/// of the subscriber.
pub type BoxedFormatLayer = Box<dyn Layer<Registry> + Send + Sync + 'static>;

/// A [`Layer`] that is added on top of the formatting layer.
///
/// Its type names the subscriber it is attached to — the registry with the
/// formatting layer on it — because it sits outside the formatting layer.
pub type BoxedCustomLayer =
    Box<dyn Layer<Layered<BoxedFormatLayer, Registry>> + Send + Sync + 'static>;

// -----------------------------------------------------------------------------
// DEFAULT_FILTER

/// The default [`LogConfig`] filter.
pub const DEFAULT_FILTER: &str = concat!("wgpu=warn,", "naga=warn,",);

// -----------------------------------------------------------------------------
// LogConfig

/// Configuration for the global `tracing` subscriber and `log` bridge.
///
/// A `LogConfig` bundles a [`Targets`]-based filter, a default level, and
/// optional extra layers (a custom formatting layer and a custom user layer).
/// Calling [`LogConfig::apply`] consumes the config and installs a global
/// subscriber built from it.
///
/// The layers are chained onto the registry in the order the [crate level
/// documentation] lists them: the formatting layer is the innermost one and the
/// filter is the outermost one.
///
/// ```rust, no_run
/// # use zlim_log::LogConfig;
/// LogConfig::default().apply();
/// ```
///
/// See [crate level documentation] for details.
///
/// [crate level documentation]: crate
pub struct LogConfig {
    /// Filters out logs that are "less than" the given level.
    pub level: Level,

    /// Filters logs using the [`Targets`] format.
    pub filter: String,

    /// Override the default [`tracing_subscriber::fmt::Layer`] with a custom one.
    ///
    /// This is the innermost layer, so it is the one that turns events into output.
    pub format_layer: Option<BoxedFormatLayer>,

    /// Optionally add an extra [`Layer`] to the tracing subscriber.
    ///
    /// It is attached outside the formatting layer, so it sees the events that
    /// the formatting layer is about to handle.
    pub custom_layer: Option<BoxedCustomLayer>,
}

impl Debug for LogConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LogConfig")
            .field("filter", &self.filter)
            .field("level", &self.level)
            .finish_non_exhaustive()
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            filter: DEFAULT_FILTER.to_string(),
            #[cfg(any(debug_assertions, feature = "debug"))]
            level: Level::DEBUG,
            #[cfg(not(any(debug_assertions, feature = "debug")))]
            level: Level::INFO,
            custom_layer: None,
            format_layer: None,
        }
    }
}

// -----------------------------------------------------------------------------
// LogConfig apply

impl LogConfig {
    fn build_filter_layer(level: Level, filter: String) -> Targets {
        // We must manually parse and add the directives individually
        // because `EnvFilter` has no helper methods for adding multiple directives at once.
        #[cfg(not(target_family = "wasm"))]
        let env_filters: String = std::env::var("RUST_LOG").unwrap_or_default();

        #[cfg(target_family = "wasm")]
        let env_filters: String = String::new();

        let mut targets = Targets::new().with_default(level);

        let filters = filter + "," + &env_filters;

        #[expect(clippy::allow_attributes, reason = "unexpected in wasm")]
        #[allow(clippy::print_stderr, reason = "logger is not ready yet")]
        for x in filters.split(',') {
            let x = x.trim_ascii();
            if x.is_empty() {
                continue;
            }

            if let Some((name, level)) = x.split_once('=')
                && !name.trim_ascii_end().is_empty()
                && let Ok(l) = level.trim_ascii_start().parse::<Level>()
            {
                targets = targets.with_target(name.trim_ascii_end(), l);
            } else if x.parse::<Level>().is_ok() {
                ::core::hint::cold_path();
                std::eprintln!(
                    "LogConfig's filter or `RUST_LOG` env contains a bare level segment `{x}`, \
                    which was ignored because LogConfig already sets a `level` field."
                );
                continue; // ignored
            } else {
                ::core::hint::cold_path();
                std::eprintln!(
                    "LogConfig failed to parse env segment `{x}`: expected either a bare level \
                    (e.g. `info`) or a `target=level` pair (e.g. `my_crate=warn`), where the \
                    level is one of: off, error, warn, info, debug, trace."
                );
            }
        }

        targets
    }

    pub fn apply(self) {
        use tracing::subscriber::set_global_default;
        use tracing_log::LogTracer;
        use tracing_subscriber::layer::SubscriberExt;

        let subscriber: Registry = Registry::default();

        cfg_select! {
            target_family = "wasm" => {
                let format_layer_ignored = self.format_layer.is_some();
                let wasm_layer_config = tracing_wasm::WASMLayerConfig::default();
                let format_layer: BoxedFormatLayer = Box::new(tracing_wasm::WASMLayer::new(wasm_layer_config));
                let subscriber = subscriber.with(format_layer);
            }
            target_os = "ios" => {
                let format_layer_ignored = self.format_layer.is_some();
                let format_layer: BoxedFormatLayer = Box::new(tracing_oslog::OsLogger::default());
                let subscriber = subscriber.with(format_layer);
            }
            target_os = "android" => {
                let format_layer_ignored = self.format_layer.is_some();
                let format_layer: BoxedFormatLayer = Box::new(android_layer::AndroidLayer);
                let subscriber = subscriber.with(format_layer);
            }
            _ => {
                let format_layer_ignored: bool = false;
                let format_layer: BoxedFormatLayer = self.format_layer.unwrap_or_else(|| {
                    // note: the implementation of `Default` reads from the env var NO_COLOR
                    // to decide whether to use ANSI color codes, which is common convention
                    // https://no-color.org/
                    let layer = tracing_subscriber::fmt::Layer::default();
                    Box::new(layer.with_writer(std::io::stderr))
                });

                let subscriber = subscriber.with(format_layer);
            }
        }

        let subscriber = subscriber.with(self.custom_layer);

        #[cfg(feature = "trace_chrome")]
        let subscriber = subscriber.with(chrome_layer::chrome_layer());

        #[cfg(feature = "trace_error")]
        let subscriber = subscriber.with(tracing_error::ErrorLayer::default());

        let targets: Targets = Self::build_filter_layer(self.level, self.filter);
        let subscriber = subscriber.with(targets);

        let level_filter = match self.level {
            Level::TRACE => ::tracing_log::log::LevelFilter::Trace,
            Level::DEBUG => ::tracing_log::log::LevelFilter::Debug,
            Level::INFO => ::tracing_log::log::LevelFilter::Info,
            Level::WARN => ::tracing_log::log::LevelFilter::Warn,
            Level::ERROR => ::tracing_log::log::LevelFilter::Error,
        };

        let logger_success = LogTracer::builder()
            .with_max_level(level_filter)
            .init()
            .is_ok();

        let subscriber_success = set_global_default(subscriber).is_ok();

        if format_layer_ignored {
            tracing::info!("`format_layer` is ignored due to the unsupported platform.");
        }

        match (logger_success, subscriber_success) {
            (true, true) => (),
            (true, false) => tracing::error!(
                "Could not set global tracing subscriber as it is already set. Consider disabling LogConfig."
            ),
            (false, true) => tracing::error!(
                "Could not set global logger as it is already set. Consider disabling LogConfig."
            ),
            (false, false) => tracing::error!(
                "Could not set global logger and tracing subscriber as they are already set. Consider disabling LogConfig."
            ),
        }
    }
}

// -----------------------------------------------------------------------------
// prelude

/// The log prelude.
pub mod prelude {
    // doc(hidden): keeps this path out of autocomplete suggestions.
    #[doc(hidden)]
    pub use crate::{debug, debug_once, debug_span};
    #[doc(hidden)]
    pub use crate::{error, error_once, error_span};
    #[doc(hidden)]
    pub use crate::{info, info_once, info_span};
    #[doc(hidden)]
    pub use crate::{trace, trace_once, trace_span};
    #[doc(hidden)]
    pub use crate::{warn, warn_once, warn_span};
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::LogConfig;
    use core::time::Duration;
    use std::time::Instant;

    /// Number of independent single-shot samples.
    const SAMPLES: usize = 20;

    /// Measures a single `tracing::debug!` call.
    fn time_one_log() -> Duration {
        let start = Instant::now();
        tracing::info!("benchmark log");
        start.elapsed()
    }

    /// Measures a single span enter/exit pair.
    fn time_one_span() -> Duration {
        let start = Instant::now();
        let span = tracing::info_span!("benchmark_span");
        span.in_scope(|| core::hint::black_box(()));
        start.elapsed()
    }

    /// Runs `f` `SAMPLES` times and returns the median duration.
    fn median_of(mut samples: Vec<Duration>) -> Duration {
        samples.sort_unstable();
        samples[samples.len() / 2]
    }

    #[test]
    #[ignore = "manual trigger"]
    #[expect(clippy::print_stdout, reason = "test result display")]
    fn measure_span_and_log() {
        LogConfig::default().apply();

        // Warm up: initialize the subscriber and thread-local caches.
        let _ = time_one_log();
        let _ = time_one_span();

        let log_samples: Vec<_> = (0..SAMPLES).map(|_| time_one_log()).collect();
        let span_samples: Vec<_> = (0..SAMPLES).map(|_| time_one_span()).collect();

        let log_median = median_of(log_samples);
        let span_median = median_of(span_samples);

        println!("samples: {SAMPLES}");
        println!("log  median: {log_median:?}");
        println!("span median: {span_median:?}");
    }
}
