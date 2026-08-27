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
        zlim_cfg::switch! {
            zlim_os::cfg::wasm => {
                static SET_HOOK: std::sync::Once = std::sync::Once::new();
                SET_HOOK.call_once(|| {
                    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
                });
            }
            zlim_core::cfg::backtrace => {
                static SET_HOOK: std::sync::Once = std::sync::Once::new();
                SET_HOOK.call_once(|| {
                    let current_hook = std::panic::take_hook();
                    let hook = zlim_core::error::zlim_error_panic_hook(current_hook);
                    std::panic::set_hook(Box::new(hook));
                });
            }
            _ => {}
        }
    }
}
