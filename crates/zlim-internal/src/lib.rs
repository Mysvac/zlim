#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub use zlim_cfg as cfg;
pub use zlim_ptr as ptr;
pub use zlim_reg as reg;

pub use zlim_os as os;
pub use zlim_utils as utils;

pub use zlim_reflect as reflect;
pub use zlim_task as task;

pub use zlim_core as core;

pub mod derive {
    pub use zlim_core::derive::*;
    pub use zlim_reflect::derive::*;
}
