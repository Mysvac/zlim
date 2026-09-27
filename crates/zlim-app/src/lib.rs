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
pub use main_schedule::{PostStartup, PreStartup, RunFixedMainLoop, SpawnScene, Startup};

pub use schedule_runner::{RunMode, ScheduleRunnerPlugin};

pub use crate::derive::{AppLabel, zlim_main};

// -----------------------------------------------------------------------------
// jobs

/// The app jobs.
pub mod jobs {
    #[doc(inline)]
    pub use crate::main_schedule::{RunFixedMainJob, RunFixedMainLoopJob, RunMainJob};
    #[doc(inline)]
    pub use crate::shutdown::HandleExitSignal;
}

// -----------------------------------------------------------------------------
// macros

/// The app macros.
pub mod derive {
    #[doc(inline)]
    pub use zlim_app_derive::{AppLabel, zlim_main};
}

// -----------------------------------------------------------------------------
// plugins

/// The app plugins.
pub mod plugins {
    #[doc(no_inline)]
    pub use crate::MainSchedulePlugin;
    #[doc(no_inline)]
    pub use crate::{PanicHandlerPlugin, ScheduleRunnerPlugin, ShutdownPlugin};
}

// -----------------------------------------------------------------------------
// prelude

/// The app preludes.
pub mod prelude {
    #[doc(no_inline)]
    pub use crate::AppExit;
    #[doc(no_inline)]
    pub use crate::derive::{AppLabel, zlim_main};
    #[doc(no_inline)]
    pub use crate::{App, SubApp};
    #[doc(no_inline)]
    pub use crate::{First, Last, PostUpdate, PreUpdate, Update};
    #[doc(no_inline)]
    pub use crate::{FixedFirst, FixedLast, FixedMainLoopStage};
    #[doc(no_inline)]
    pub use crate::{FixedMain, FixedMainScheduleOrder, Main, MainScheduleOrder};
    #[doc(no_inline)]
    pub use crate::{FixedPostUpdate, FixedPreUpdate, FixedUpdate, Startup};
    #[doc(no_inline)]
    pub use crate::{Plugin, PluginExt, PluginGroup};
    #[doc(no_inline)]
    pub use crate::{PostStartup, PreStartup, RunFixedMainLoop, SpawnScene};
}

// -----------------------------------------------------------------------------
// Special platform support

#[doc(hidden)]
pub mod sys {
    pub use zlim_task::designate_main_thread;

    zlim_os::cfg::android! {
        pub use zlim_os::sys::android_activity::AndroidApp;
        pub static ANDROID_APP: std::sync::OnceLock<AndroidApp> = std::sync::OnceLock::new();
    }
}
