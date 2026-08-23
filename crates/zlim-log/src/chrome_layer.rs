#![expect(unsafe_code, reason = "Just keep alive, does not read and write")]
#![expect(clippy::print_stderr, reason = "Allowed during logger setup")]

use tracing::Subscriber;
use tracing_chrome::ChromeLayer;
use tracing_chrome::ChromeLayerBuilder;
use tracing_chrome::EventOrSpan;
use tracing_chrome::FlushGuard;
use tracing_subscriber::fmt::FormattedFields;
use tracing_subscriber::fmt::format::DefaultFields;
use tracing_subscriber::registry::LookupSpan;

struct FlushGuardCell {
    _g: FlushGuard,
}

unsafe impl Sync for FlushGuardCell {}
unsafe impl Send for FlushGuardCell {}

static FLUSH_GUARD: std::sync::OnceLock<FlushGuardCell> = std::sync::OnceLock::new();

pub(super) fn chrome_layer<S>() -> Option<ChromeLayer<S>>
where
    S: Subscriber + for<'s> LookupSpan<'s> + Send + Sync,
{
    let mut layer: ChromeLayerBuilder<S> = ChromeLayerBuilder::new();

    if let Ok(path) = std::env::var("TRACE_CHROME") {
        layer = layer.file(path);
    }

    let name_func = |event_or_span: &EventOrSpan<S>| -> String {
        match event_or_span {
            EventOrSpan::Event(event) => event.metadata().name().to_owned(),
            EventOrSpan::Span(span) => {
                if let Some(fields) = span.extensions().get::<FormattedFields<DefaultFields>>() {
                    format!("{}: {}", span.metadata().name(), fields.fields.as_str())
                } else {
                    span.metadata().name().to_owned()
                }
            }
        }
    };

    let (layer, guard) = layer.name_fn(Box::new(name_func)).build();

    if FLUSH_GUARD.set(FlushGuardCell { _g: guard }).is_ok() {
        return Some(layer);
    }

    ::core::hint::cold_path();

    std::eprintln!("Could not set chrome layer as it is already set.");
    None
}
