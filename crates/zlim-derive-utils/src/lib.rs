#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![forbid(unsafe_code)]

extern crate proc_macro;

// -----------------------------------------------------------------------------
// Modules

mod manifest;

// -----------------------------------------------------------------------------
// Exports

#[doc(inline)]
pub use manifest::crate_path;
