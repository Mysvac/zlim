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
// tracy_memory

#[cfg(feature = "tracy_memory")]
use tracy_client::ProfiledAllocator as TracyAllocator;

#[cfg(feature = "tracy_memory")]
#[global_allocator]
static GLOBAL: TracyAllocator<std::alloc::System> = TracyAllocator::new(std::alloc::System, 100);

#[cfg(feature = "tracy_demangle")]
tracy_client::register_demangler!();

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

type CustomSubscriber = Layered<Option<BoxedLayer>, Registry>;

type FilteredSubscriber = Layered<Targets, CustomSubscriber>;

#[cfg(feature = "trace_error")]
type PreFormatSubscriber =
    Layered<tracing_error::ErrorLayer<FilteredSubscriber>, FilteredSubscriber>;

#[cfg(not(feature = "trace_error"))]
type PreFormatSubscriber = FilteredSubscriber;

pub type BoxedLayer = Box<dyn Layer<Registry> + Send + Sync + 'static>;
pub type BoxedFmtLayer = Box<dyn Layer<PreFormatSubscriber> + Send + Sync + 'static>;

// -----------------------------------------------------------------------------
// DEFAULT_FILTER

/// The default [`LogConfig`] filter.
pub const DEFAULT_FILTER: &str = concat!("wgpu=warn,", "naga=warn,",);

// -----------------------------------------------------------------------------
// LogConfig

/// Configuration for the global `tracing` subscriber and `log` bridge.
///
/// A `LogConfig` bundles a [`Targets`]-based filter, a default level, and
/// optional extra layers (custom user layer, custom formatting layer, and
/// Tracy streaming). Calling [`LogConfig::apply`] consumes the config and
/// installs a global subscriber built from it.
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
    /// Filters logs using the [`Targets`] format.
    pub filter: String,

    /// Filters out logs that are "less than" the given level.
    pub level: Level,

    /// Optionally add an extra [`Layer`] to the tracing subscriber
    pub custom_layer: Option<BoxedLayer>,

    /// Override the default [`tracing_subscriber::fmt::Layer`] with a custom one.
    pub format_layer: Option<BoxedFmtLayer>,

    /// Whether to stream events to the Tracy profiler or collector.
    ///
    /// Ignored if `trace_tracy` feature is not enabled.
    pub enable_tracy: bool,
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
            #[cfg(feature = "trace_tracy")]
            enable_tracy: true,
            #[cfg(not(feature = "trace_tracy"))]
            enable_tracy: false,
        }
    }
}

// -----------------------------------------------------------------------------
// LogConfig apply

impl LogConfig {
    fn build_filter_layer(&self) -> Targets {
        // We must manually parse and add the directives individually
        // because `EnvFilter` has no helper methods for adding multiple directives at once.
        #[cfg(not(target_family = "wasm"))]
        let env_filters: String = std::env::var("RUST_LOG").unwrap_or_default();

        #[cfg(target_family = "wasm")]
        let env_filters: String = String::from("");

        let mut targets = Targets::new().with_default(self.level);

        let filters = env_filters + "," + &self.filter;

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

        let targets: Targets = self.build_filter_layer();
        let subscriber: CustomSubscriber = subscriber.with(self.custom_layer);
        let subscriber: FilteredSubscriber = subscriber.with(targets);

        #[cfg(feature = "trace_error")]
        let subscriber = subscriber.with(tracing_error::ErrorLayer::default());

        cfg_select! {
            target_family = "wasm" => {
                let enable_tracy_ignored: bool = true;
                let format_layer_ignored = self.format_layer.is_some();
                let wasm_layer_config = tracing_wasm::WASMLayerConfig::default();
                let subscriber = subscriber.with(tracing_wasm::WASMLayer::new(wasm_layer_config));
            }
            target_os = "ios" => {
                let enable_tracy_ignored: bool = true;
                let format_layer_ignored = self.format_layer.is_some();
                let subscriber = subscriber.with(tracing_oslog::OsLogger::default());
            }
            target_os = "android" => {
                let enable_tracy_ignored: bool = false;
                let format_layer_ignored = self.format_layer.is_some();
                #[cfg(feature = "trace_tracy")]
                let tracy_layer = self.enable_tracy.then(|| tracing_tracy::TracyLayer::default());
                #[cfg(feature = "trace_tracy")]
                let subscriber = subscriber.with(tracy_layer);
                let subscriber = subscriber.with(android_layer::AndroidLayer);
            }
            _ => {
                let enable_tracy_ignored: bool = false;
                let format_layer_ignored: bool = false;
                let format_layer: BoxedFmtLayer = self.format_layer.unwrap_or_else(|| {
                    // note: the implementation of `Default` reads from the env var NO_COLOR
                    // to decide whether to use ANSI color codes, which is common convention
                    // https://no-color.org/
                    let layer = tracing_subscriber::fmt::Layer::default();
                    Box::new(layer.with_writer(std::io::stderr))
                });

                // `zlim_render` logs a `tracy.frame_mark` event every frame at
                // Level::INFO for `tracing-tracy`. Formatted logs should omit it.
                #[cfg(feature = "trace_tracy")]
                let skip_frame_mark = |meta: &tracing::Metadata<'_>| {
                    meta.is_span() || meta.fields().field("tracy.frame_mark").is_none()
                };

                #[cfg(feature = "trace_tracy")]
                let format_layer = format_layer.with_filter(tracing_subscriber::filter::FilterFn::new(skip_frame_mark));
                let subscriber = subscriber.with(format_layer);

                #[cfg(feature = "trace_chrome")]
                let chrome_layer = chrome_layer::chrome_layer();
                #[cfg(feature = "trace_chrome")]
                let subscriber = subscriber.with(chrome_layer);

                #[cfg(feature = "trace_tracy")]
                let tracy_layer = self.enable_tracy.then(|| tracing_tracy::TracyLayer::default());
                #[cfg(feature = "trace_tracy")]
                let subscriber = subscriber.with(tracy_layer);
            }
        }

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

        #[cfg(not(feature = "trace_tracy"))]
        if self.enable_tracy {
            let _ = enable_tracy_ignored;
            tracing::info!(
                "`LogConfig::enable_tracy` is `true` but `trace_tracy` feature is not enabled, skipped."
            );
        }

        #[cfg(feature = "trace_tracy")]
        if self.enable_tracy && enable_tracy_ignored {
            tracing::warn!(
                "`LogConfig::enable_tracy` is ignored on this platform, but the Tracy client \
                may still be active. If Tracy is not working as expected, consider disabling \
                the `trace_tracy` feature."
            );
        } else if self.enable_tracy {
            tracing::warn!(
                "Tracing with Tracy is active. Memory consumption will grow once a \
                collector connects; if `tracy_broadcast` is enabled, the program may \
                already be broadcasting."
            );
        } else {
            tracing::warn!(
                "`LogConfig::enable_tracy` is `false`, but the Tracy client is still linked \
                and active because the `trace_tracy` feature is enabled. This is likely not \
                what you intended; disable the `trace_tracy` feature to opt out."
            );
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
