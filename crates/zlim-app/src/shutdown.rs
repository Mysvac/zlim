use core::sync::atomic::AtomicU32;
use core::sync::atomic::Ordering::{Acquire, SeqCst};

use zlim_core::job_fn;
use zlim_core::message::MessageWriter;
use zlim_utils::sync::SpinLock;

use crate::MainSchedulePlugin;
use crate::{App, AppExit, Plugin, Update};

/// Graceful shutdown plugin for terminal signal handling (Ctrl+C).
///
/// # Behavior
///
/// - First `Ctrl+C` or `gracefully_exit()` call → emits `AppExit` for clean shutdown
/// - Second call → forces immediate exit via `std::process::exit`.
///
/// # Exit Handlers
///
/// Handlers registered via [`on_exit`] are invoked **once** during the first
/// graceful exit attempt. They run before the `AppExit` event is emitted
/// and are suitable for cleanup tasks (e.g., saving state, waking event loops).
///
/// See [`on_exit`] and [`gracefully_exit`] for details.
///
/// [`on_exit`]: ShutdownPlugin::on_exit
/// [`gracefully_exit`]: ShutdownPlugin::gracefully_exit
#[derive(Debug, Default)]
pub struct ShutdownPlugin;

static SHOULD_EXIT: AtomicU32 = AtomicU32::new(0);

/// Handlers run when the app is asked to exit via `Ctrl+C`.
static ON_EXIT_HANDLERS: SpinLock<Vec<Box<dyn FnOnce() + Send>>> = SpinLock::new(Vec::new());

// -----------------------------------------------------------------------------
// Plugin Implementation

impl ShutdownPlugin {
    /// ShutdownPlugin AppExit code
    pub const EXIT_CODE: u8 = 130;

    /// Registers a `handler` that is invoked when `gracefully_exit` is called.
    ///
    /// Handlers are executed **once** during the first graceful exit attempt,
    /// before the `AppExit` event is emitted.
    ///
    /// This can be used to e.g. waking a sleeping event loop so it can observe the [`AppExit`].
    pub fn on_exit(handler: impl FnOnce() + Send + 'static) {
        ON_EXIT_HANDLERS.lock().push(Box::new(handler));
    }

    /// Triggers a graceful application shutdown.
    ///
    /// When called the first time, it sends the [`AppExit`] event to all apps using
    /// this plugin to make them gracefully exit.
    ///
    /// If called more than once, it exits immediately through `std::process::exit`.
    ///
    /// On supported platforms, `gracefully_exit` will be called automatically when
    /// the process receives a termination signal (ctrl+c).
    ///
    /// On unsupported platforms, users must call this function manually to stop the program.
    pub fn gracefully_exit() {
        if SHOULD_EXIT.fetch_add(1, SeqCst) > 0 {
            zlim_log::error!("Received more than one ctrl+c. Skipping graceful shutdown.");
            std::process::exit(Self::EXIT_CODE.into());
        };

        let mut guard = ON_EXIT_HANDLERS.lock();
        let handlers = core::mem::take(&mut *guard);
        ::core::mem::drop(guard);
        handlers.into_iter().for_each(|f| f());
    }
}

impl Plugin for ShutdownPlugin {
    fn build(&self, app: &mut App) {
        if app.contains_plugin::<MainSchedulePlugin>() {
            app.add_plugin_order::<MainSchedulePlugin, Self>();
        }
    }

    fn apply(&self, app: &mut App) {
        MainSchedulePlugin::warn_if_unset(app, "ShutdownPlugin");

        #[cfg(any(all(unix, not(target_os = "horizon")), windows))]
        match ctrlc::try_set_handler(ShutdownPlugin::gracefully_exit) {
            Ok(()) => {
                zlim_log::debug!("ShutdownPlugin: Default on_signal handler install succeed");
            }
            Err(ctrlc::Error::MultipleHandlers) => {
                zlim_log::info!(
                    "Skipping installing default terminal signal handler as one was already \
                    installed.\n  Please call `ShutdownPlugin::gracefully_exit` in your own \
                    handler if you still want graceful exit."
                );
            }
            Err(err) => zlim_log::warn!("Failed to set `Ctrl+C` handler: {err}"),
        }

        let world = app.main_world_mut();
        world.schedule_entry(Update).insert::<HandleExitSignal>(());
    }
}

#[job_fn(type = HandleExitSignal, name = "zlim_app::jobs::HandleExitSignal")]
fn handle_exit_signal(mut app_exit_writer: MessageWriter<AppExit>) {
    if SHOULD_EXIT.load(Acquire) > 0 {
        app_exit_writer.write(AppExit::from_code(ShutdownPlugin::EXIT_CODE));
    }
}
