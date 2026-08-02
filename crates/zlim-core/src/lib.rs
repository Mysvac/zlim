#![expect(unsafe_code, reason = "performance optimization")]
#![cfg_attr(docsrs, feature(doc_cfg))]

// ----------------------------------------------------------------------------
// Extern Self

// Usually, we need to use `crate` in the crate itself and use `zlim_*` in
// doc testing. `zlim_derive_utils::crate_path` choose `zlim_*`, so we must
// have an `extern self` to ensure it can be used as an alias for `crate`.
extern crate self as zlim_core;

// ----------------------------------------------------------------------------
// Macros

pub use zlim_core_derive as derive;

// ----------------------------------------------------------------------------
// Modules

pub mod component;
pub mod entity;
pub mod error;
pub mod message;
pub mod model;
pub mod scene;
pub mod schedule;
pub mod script;
pub mod tick;
pub mod world;

pub mod utils;
