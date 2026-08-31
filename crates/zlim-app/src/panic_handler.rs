//! provides panic handlers plugin
use crate::{App, Plugin};

/// Adds sensible panic handlers to Apps.
///
/// Adding this plugin will setup a panic hook appropriate to your target platform:
///
/// - On Wasm, uses [`console_error_panic_hook`], logging to the browser console.
/// - Other platforms are currently not setup.
///
/// [`console_error_panic_hook`]: https://crates.io/crates/console_error_panic_hook
#[derive(Debug, Default)]
pub struct PanicHandlerPlugin;

impl Plugin for PanicHandlerPlugin {
    fn apply(&self, _: &mut App) {
        static SET_HOOK: std::sync::Once = std::sync::Once::new();

        SET_HOOK.call_once(set_hook);
    }
}

#[cold]
fn set_hook() {
    cfg_select! {
        target_family = "wasm" => {
            std::panic::set_hook(Box::new(wasm_impls::hook));
        },
        feature = "trace" => {
            let default_hook = std::panic::take_hook();
            #[expect(clippy::print_stderr, reason = "panic output")]
            std::panic::set_hook(Box::new(move |info| {
                if zlim_core::cfg::backtrace!()
                    && zlim_core::error::handler::PANIC_BACKTRACE_CAPTURED.replace(false)
                    && let Some(msg) = info.payload_as_str()
                {
                    std::eprintln!("{msg}");
                } else {
                    default_hook(info);
                }

                #[cfg(feature = "trace")]
                std::eprintln!("\nspan trace:\n{}", tracing_error::SpanTrace::capture());
            }));
        },
       feature = "backtrace" => {
            let default_hook = std::panic::take_hook();
            #[expect(clippy::print_stderr, reason = "panic output")]
            std::panic::set_hook(Box::new(move |info| {
                if zlim_core::error::handler::PANIC_BACKTRACE_CAPTURED.replace(false)
                    && let Some(msg) = info.payload_as_str()
                {
                    std::eprintln!("{msg}");
                } else {
                    default_hook(info);
                }
            }));
        }
        _ => {}
    }
}

#[cfg(target_family = "wasm")]
mod wasm_impls {
    use std::panic::PanicHookInfo;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = console)]
        fn error(msg: String);

        type Error;

        #[wasm_bindgen(constructor)]
        fn new() -> Error;

        #[wasm_bindgen(structural, method, getter)]
        fn stack(error: &Error) -> String;
    }

    pub(super) fn hook(info: &PanicHookInfo<'_>) {
        let mut msg = info.to_string();

        #[cfg(feature = "trace")]
        {
            msg.push_str("\n\nTrace:\n\n");
            msg.push_str(&tracing_error::SpanTrace::capture().to_string());
            msg.push_str("\n\n");
            msg.push_str("-------------------------------------------------");
        }

        msg.push_str("\n\nStack:\n\n");
        let e = Error::new();
        let stack = e.stack();
        msg.push_str(&stack);
        msg.push_str("\n\n");

        error(msg);
    }
}
