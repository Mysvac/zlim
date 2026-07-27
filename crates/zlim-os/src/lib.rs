#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

// ----------------------------------------------------------------------------
// Compilation config

/// Some macros used for compilation control.
pub mod cfg {
    pub(crate) use zlim_cfg::switch;

    zlim_cfg::define_alias! {
        #[cfg(target_os = "linux")] => linux,
        #[cfg(target_os = "macos")] => macos,
        #[cfg(target_family = "wasm")] => wasm,
        #[cfg(target_os = "android")] => android,
        #[cfg(target_family = "windows")] => windows,
        #[cfg(not(target_os = "linux"))] => not_linux,
        #[cfg(not(target_os = "macos"))] => not_macos,
        #[cfg(not(target_family = "wasm"))] => not_wasm,
        #[cfg(not(target_os = "android"))] => not_android,
        #[cfg(not(target_family = "windows"))] => not_windows,
    }
}

// ----------------------------------------------------------------------------
// Modules

pub mod dirs;
pub mod thread;
pub mod time;

// ----------------------------------------------------------------------------
// Special platform support

#[doc(hidden)]
pub mod sys {
    #[cfg(target_family = "windows")]
    pub use windows_sys;

    #[cfg(target_os = "android")]
    pub use android_activity;

    #[cfg(target_family = "wasm")]
    pub use js_sys;

    #[cfg(target_family = "wasm")]
    pub use wasm_bindgen;
}
