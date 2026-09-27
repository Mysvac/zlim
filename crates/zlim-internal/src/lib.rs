#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub use zlim_cfg as cfg;
pub use zlim_ptr as ptr;
pub use zlim_reg as reg;

pub use zlim_log as log;
pub use zlim_os as os;
pub use zlim_utils as utils;

pub use zlim_reflect as reflect;
pub use zlim_task as task;
pub use zlim_tracy as tracy;

pub use zlim_core as core;

pub use zlim_app as app;

pub use zlim_math as math;

pub use zlim_shape as shape;

pub use zlim_curve as curve;

pub use zlim_color as color;

pub use zlim_transform as transform;

pub use zlim_diagnostic as diagnostic;

pub use zlim_asset as asset;

pub use zlim_scene as scene;

#[cfg(feature = "zlim-sample")]
pub use zlim_sample as sample;

#[cfg(feature = "zlim-sysinfo")]
pub use zlim_sysinfo as sysinfo;

/// Zlim macros.
///
/// The macro related to the log has not been re exported.
/// Please use `log::xxx!` directly, such as `log::warn!()`.
///
/// Some macros used internally also has not be re-exported
/// here (such as `into_owning!` in `zlim_ptr`).
pub mod derive {
    #[doc(no_inline)]
    pub use zlim_app::derive::*;
    #[doc(no_inline)]
    pub use zlim_asset::derive::*;
    #[doc(no_inline)]
    pub use zlim_core::derive::*;
    #[doc(no_inline)]
    pub use zlim_reflect::derive::*;
    #[doc(no_inline)]
    pub use zlim_scene::derive::*;
}

/// Zlim plugins.
pub mod plugins {
    #[doc(no_inline)]
    pub use zlim_app::plugins::*;
    #[doc(no_inline)]
    pub use zlim_asset::plugins::*;
    #[doc(no_inline)]
    pub use zlim_diagnostic::plugins::*;
    #[doc(no_inline)]
    pub use zlim_scene::plugins::*;
    #[doc(no_inline)]
    pub use zlim_transform::plugins::*;

    #[doc(no_inline)]
    #[cfg(feature = "zlim-sysinfo")]
    pub use zlim_sysinfo::plugins::*;
}

/// Zlim preludes.
pub mod prelude {
    // doc(hidden): keeps this path out of autocomplete suggestions.
    #[doc(no_inline)]
    pub use zlim_app::prelude::*;
    #[doc(no_inline)]
    pub use zlim_asset::prelude::*;
    #[doc(no_inline)]
    pub use zlim_color::prelude::*;
    #[doc(no_inline)]
    pub use zlim_core::prelude::*;
    #[doc(no_inline)]
    pub use zlim_log::prelude::*;
    #[doc(no_inline)]
    pub use zlim_math::prelude::*;
    #[doc(no_inline)]
    pub use zlim_os::prelude::*;
    #[doc(no_inline)]
    pub use zlim_reflect::prelude::*;
    #[doc(no_inline)]
    pub use zlim_scene::prelude::*;
    #[doc(no_inline)]
    pub use zlim_shape::prelude::*;
    #[doc(no_inline)]
    pub use zlim_task::prelude::*;
    #[doc(no_inline)]
    pub use zlim_transform::prelude::*;
}
