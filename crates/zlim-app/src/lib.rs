//! The application layer: compose worlds, plugins, and a runner into a runnable app.
#![cfg_attr(docsrs, expect(internal_features, reason = "needed for fake_variadic"))]
#![cfg_attr(docsrs, feature(doc_cfg, rustdoc_internals))]

// -----------------------------------------------------------------------------
// Modules

mod app;
mod exit;
mod label;
mod plugin;

mod main_schedule;
mod panic_handler;
mod schedule_runner;
mod shutdown;

// -----------------------------------------------------------------------------
// Exports

pub use zlim_app_derive as derive;
pub use zlim_app_derive::AppLabel;

pub use app::{App, ExtractFn, RunnerFn, SubApp};
pub use exit::{AppExit, AppExitStage};
pub use label::{AppLabel, InternedAppLabel};
pub use plugin::{DuplicateStrategy, Plugin, PluginGroup, Plugins, PluginsState};

pub use shutdown::ShutdownPlugin;

pub use panic_handler::PanicHandlerPlugin;

pub use main_schedule::MainSchedulePlugin;
pub use main_schedule::{First, FixedMainLoopStage, Last, PostUpdate, PreUpdate, Update};
pub use main_schedule::{FixedFirst, FixedLast, FixedPostUpdate, FixedPreUpdate, FixedUpdate};
pub use main_schedule::{FixedMain, FixedMainScheduleOrder, Main, MainScheduleOrder};
pub use main_schedule::{PostStartup, PreStartup, RunFixedMainLoop, Startup};

pub use schedule_runner::{RunMode, ScheduleRunnerPlugin};

/// re-exports jobs
pub mod jobs {
    pub use crate::main_schedule::{RunFixedMainJob, RunFixedMainLoopJob, RunMainJob};
    pub use crate::shutdown::HandleExitSignal;
}

// -----------------------------------------------------------------------------
// Special platform support

#[doc(hidden)]
pub mod sys {
    zlim_os::cfg::android! {
        pub use zlim_os::sys::android_activity::AndroidApp;
        pub static ANDROID_APP: std::sync::OnceLock<AndroidApp> = std::sync::OnceLock::new();
    }
}
