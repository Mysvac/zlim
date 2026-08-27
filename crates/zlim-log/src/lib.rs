#![doc = include_str!("../README.md")]

// `zlim_log` is used as the crate alias inside the README's intra-doc links,
// so we re-export `crate` under that name (see the `extern crate self` in
// zlim-core for the same pattern).
extern crate self as zlim_log;

use core::fmt::{Debug, Formatter};
use tracing_subscriber::layer::Layered;
use tracing_subscriber::{EnvFilter, Layer, Registry};

// -----------------------------------------------------------------------------
// Modules

#[cfg(target_os = "android")]
mod android_layer;

#[cfg(feature = "trace_chrome")]
mod chrome_layer;

mod macros;

// -----------------------------------------------------------------------------
// trace_memory

#[cfg(feature = "trace_memory")]
#[global_allocator]
static GLOBAL: tracy_client::ProfiledAllocator<std::alloc::System> =
    tracy_client::ProfiledAllocator::new(std::alloc::System, 100);

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

type FilteredSubscriber = Layered<EnvFilter, CustomSubscriber>;

#[cfg(feature = "trace")]
type PreFormatSubscriber =
    Layered<tracing_error::ErrorLayer<FilteredSubscriber>, FilteredSubscriber>;

#[cfg(not(feature = "trace"))]
type PreFormatSubscriber = FilteredSubscriber;

pub type BoxedLayer = Box<dyn Layer<Registry> + Send + Sync + 'static>;
pub type BoxedFmtLayer = Box<dyn Layer<PreFormatSubscriber> + Send + Sync + 'static>;

// -----------------------------------------------------------------------------
// DEFAULT_FILTER

/// The default [`LogPlugin`] [`EnvFilter`].
pub const DEFAULT_FILTER: &str = concat!("wgpu=warn,", "naga=warn,",);

// -----------------------------------------------------------------------------
// LogPlugin

pub struct LogPlugin {
    /// Filters logs using the [`EnvFilter`] format
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

impl Debug for LogPlugin {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LogPlugin")
            .field("filter", &self.filter)
            .field("level", &self.level)
            .finish_non_exhaustive()
    }
}

impl Default for LogPlugin {
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
// LogPlugin apply

impl LogPlugin {
    fn build_filter_layer(&self) -> EnvFilter {
        use tracing_subscriber::filter::Directive;

        // We must manually parse and add the directives individually
        // because `EnvFilter` has no helper methods for adding multiple directives at once.
        let env_filters: String = std::env::var(EnvFilter::DEFAULT_ENV).unwrap_or_default();

        // Start with the default filters, then add the env filters afterwards,
        // so that the env filters can be used to selectively override the default filters
        let default_string = format!("{},{}", self.level, self.filter);
        let mut filters = EnvFilter::builder().parse_lossy(default_string.as_str());

        for x in env_filters.split(',').filter(|s| !s.is_empty()) {
            #[expect(clippy::print_stderr, reason = "logger is not ready yet")]
            match x.parse::<Directive>() {
                Ok(d) => filters = filters.add_directive(d),
                Err(e) => {
                    ::core::hint::cold_path();
                    std::eprintln!("LogPlugin failed to parse filter from env: {e}");
                }
            }
        }

        filters
    }

    pub fn apply(self) {
        use tracing::subscriber::set_global_default;
        use tracing_log::LogTracer;
        use tracing_subscriber::layer::SubscriberExt;

        #[cfg(feature = "trace")]
        let old_handler = std::panic::take_hook();

        #[cfg(feature = "trace")]
        #[expect(clippy::print_stderr, reason = "Allowed during logger setup")]
        std::panic::set_hook(Box::new(move |infos| {
            std::eprintln!("{}", tracing_error::SpanTrace::capture());
            old_handler(infos);
        }));

        let subscriber: Registry = Registry::default();

        let env_filter: EnvFilter = self.build_filter_layer();
        let subscriber: CustomSubscriber = subscriber.with(self.custom_layer);
        let subscriber: FilteredSubscriber = subscriber.with(env_filter);

        #[cfg(feature = "trace")]
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

                // zlim_render logs a `tracy.frame_mark` event every frame
                // at Level::INFO. Formatted logs should omit it.
                #[cfg(feature = "trace_tracy")]
                let skip_frame_mark = |meta: &tracing::Metadata<'_>| {
                    meta.fields().field("tracy.frame_mark").is_none()
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

        let logger_success = LogTracer::init().is_ok();
        let subscriber_success = set_global_default(subscriber).is_ok();

        if format_layer_ignored {
            tracing::info!("`format_layer` is ignored due to the unsupported platform.");
        }

        #[cfg(not(feature = "trace_tracy"))]
        if self.enable_tracy {
            let _ = enable_tracy_ignored;
            tracing::info!(
                "`LogPlugin::enable_tracy` is `true` but `trace_tracy` feature is not enabled, skipped."
            );
        }

        #[cfg(feature = "trace_tracy")]
        if self.enable_tracy && enable_tracy_ignored {
            tracing::info!("`enable_tracy` is ignored due to the unsupported platform.");
        } else if self.enable_tracy {
            tracing::warn!(
                "Tracing with Tracy is active, memory consumption will grow until a client is connected."
            );
        } else {
            tracing::info!(
                "`trace_tracy` feature is enabled but `LogPlugin::enable_tracy` is `false`, skipped."
            );
        }

        match (logger_success, subscriber_success) {
            (true, true) => (),
            (true, false) => tracing::error!(
                "Could not set global tracing subscriber as it is already set. Consider disabling LogPlugin."
            ),
            (false, true) => tracing::error!(
                "Could not set global logger as it is already set. Consider disabling LogPlugin."
            ),
            (false, false) => tracing::error!(
                "Could not set global logger and tracing subscriber as they are already set. Consider disabling LogPlugin."
            ),
        }
    }
}
