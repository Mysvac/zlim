#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub use zlim_cfg as cfg;
pub use zlim_ptr as ptr;
pub use zlim_reg as reg;

pub use zlim_log as log;
pub use zlim_os as os;
pub use zlim_utils as utils;

pub use zlim_path as path;
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

#[cfg(feature = "zlim-sample")]
pub use zlim_sample as sample;

#[cfg(feature = "zlim-sysinfo")]
pub use zlim_sysinfo as sysinfo;

/// zlim macros
pub mod derive {
    #[doc(hidden)]
    pub use zlim_app::derive::*;
    #[doc(hidden)]
    pub use zlim_asset::derive::*;
    #[doc(hidden)]
    pub use zlim_core::derive::*;
    #[doc(hidden)]
    pub use zlim_path::derive::*;
}

/// zlim preludes
pub mod prelude {
    // doc(hidden): keeps this path out of autocomplete suggestions.
    #[doc(hidden)]
    pub use zlim_app::prelude::*;
    #[doc(hidden)]
    pub use zlim_asset::prelude::*;
    #[doc(hidden)]
    pub use zlim_color::prelude::*;
    #[doc(hidden)]
    pub use zlim_core::prelude::*;
    #[doc(hidden)]
    pub use zlim_log::prelude::*;
    #[doc(hidden)]
    pub use zlim_math::prelude::*;
    #[doc(hidden)]
    pub use zlim_os::prelude::*;
    #[doc(hidden)]
    pub use zlim_path::prelude::*;
    #[doc(hidden)]
    pub use zlim_shape::prelude::*;
    #[doc(hidden)]
    pub use zlim_task::prelude::*;
    #[doc(hidden)]
    pub use zlim_transform::prelude::*;
}
