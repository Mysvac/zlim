use core::fmt::{Debug, Write};
use std::ffi::CString;

use tracing::field::Field;
use tracing::span::{Attributes, Record};
use tracing::{Event, Id, Level, Metadata, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::field::Visit;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

pub(super) struct AndroidLayer;

// [0] : String Buffer
// [1] : A flag, "should pad a space"
struct StringRecorder(String, bool);

impl Visit for StringRecorder {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        if field.name() == "message" {
            if self.0.is_empty() {
                self.0 = format!("{:?}", value)
            } else {
                self.0 = format!("{:?}\n{}", value, self.0)
            }
        } else {
            if self.1 {
                // following args
                write!(self.0, " ").unwrap();
            } else {
                // first arg
                self.1 = true;
            }
            write!(self.0, "{} = {:?};", field.name(), value).unwrap();
        }
    }
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for AndroidLayer {
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut new_debug_record = StringRecorder(String::new(), false);

        attrs.record(&mut new_debug_record);

        if let Some(span_ref) = ctx.span(id) {
            span_ref
                .extensions_mut()
                .insert::<StringRecorder>(new_debug_record);
        }
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        if let Some(span_ref) = ctx.span(id) {
            if let Some(debug_record) = span_ref.extensions_mut().get_mut::<StringRecorder>() {
                values.record(debug_record);
            }
        }
    }

    #[expect(unsafe_code, reason = "`__android_log_write` is unsafe")]
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        fn sanitize(string: &str) -> CString {
            let bytes: &[u8] = string.as_bytes();

            if !bytes.contains(&0) {
                return CString::new(bytes.to_vec()).unwrap();
            }
            // slow path
            ::core::hint::cold_path();

            let bytes: Vec<u8> = bytes.iter().copied().filter(|byte| *byte != 0).collect();

            CString::new(bytes).unwrap()
        }

        let mut recorder = StringRecorder(String::new(), false);

        event.record(&mut recorder);

        let meta: &Metadata<'_> = event.metadata();

        let priority = match *meta.level() {
            Level::TRACE => android_log_sys::LogPriority::VERBOSE,
            Level::DEBUG => android_log_sys::LogPriority::DEBUG,
            Level::INFO => android_log_sys::LogPriority::INFO,
            Level::WARN => android_log_sys::LogPriority::WARN,
            Level::ERROR => android_log_sys::LogPriority::ERROR,
        };

        // SAFETY: Called only on Android platforms. priority is guaranteed to be in
        // range of c_int. The provided tag and message are null terminated properly.
        unsafe {
            android_log_sys::__android_log_write(
                priority as android_log_sys::c_int,
                sanitize(meta.name()).as_ptr(),
                sanitize(&recorder.0).as_ptr(),
            );
        }
    }
}
