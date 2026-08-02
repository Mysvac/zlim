#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![expect(clippy::std_instead_of_alloc, reason = "proc-macro crate")]

extern crate proc_macro;

// ----------------------------------------------------------------------------
// Modules

mod manifest;

// ----------------------------------------------------------------------------
// Exports

#[doc(inline)]
pub use manifest::crate_path;
