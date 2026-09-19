#![doc = include_str!("../README.md")]
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
pub use plugin::{DuplicateStrategy, Plugin, PluginExt};
pub use plugin::{PluginGroup, Plugins, PluginsState};

pub use shutdown::ShutdownPlugin;

pub use panic_handler::PanicHandlerPlugin;

pub use main_schedule::MainSchedulePlugin;
pub use main_schedule::{First, FixedMainLoopStage, Last, PostUpdate, PreUpdate, Update};
pub use main_schedule::{FixedFirst, FixedLast, FixedPostUpdate, FixedPreUpdate, FixedUpdate};
pub use main_schedule::{FixedMain, FixedMainScheduleOrder, Main, MainScheduleOrder};
pub use main_schedule::{PostStartup, PreStartup, RunFixedMainLoop, Startup};

pub use schedule_runner::{RunMode, ScheduleRunnerPlugin};

// -----------------------------------------------------------------------------
// jobs

/// The app jobs.
pub mod jobs {
    pub use crate::main_schedule::{RunFixedMainJob, RunFixedMainLoopJob, RunMainJob};
    pub use crate::shutdown::HandleExitSignal;
}

// -----------------------------------------------------------------------------
// prelude

/// The app preludes.
pub mod prelude {
    #[doc(hidden)]
    pub use crate::app::{App, SubApp};
    #[doc(hidden)]
    pub use crate::exit::AppExit;
    #[doc(hidden)]
    pub use crate::main_schedule::{First, Last, PostUpdate, PreUpdate, Update};
    #[doc(hidden)]
    pub use crate::main_schedule::{FixedFirst, FixedLast, FixedMainLoopStage};
    #[doc(hidden)]
    pub use crate::main_schedule::{FixedMain, FixedMainScheduleOrder, Main, MainScheduleOrder};
    #[doc(hidden)]
    pub use crate::main_schedule::{FixedPostUpdate, FixedPreUpdate, FixedUpdate};
    #[doc(hidden)]
    pub use crate::main_schedule::{PostStartup, PreStartup, RunFixedMainLoop, Startup};
    #[doc(hidden)]
    pub use crate::plugin::{Plugin, PluginExt, PluginGroup};
    #[doc(hidden)]
    pub use zlim_app_derive::{AppLabel, zlim_main};
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
