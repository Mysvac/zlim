#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, expect(internal_features, reason = "needed for fake_variadic"))]
#![cfg_attr(docsrs, feature(doc_cfg, rustdoc_internals))]

// ----------------------------------------------------------------------------

/// compilation configurations
pub mod cfg {
    zlim_cfg::define_alias! {
        #[cfg(any(feature = "debug", debug_assertions))] => debug,
    }
}

// ----------------------------------------------------------------------------

// Usually, we need to use `crate` in the crate itself and use `zlim_*` in
// doc testing. `zlim_derive_utils::crate_path` choose `zlim_*`, so we must
// have an `extern self` to ensure it can be used as an alias for `crate`.
extern crate self as zlim_reflect;

// ----------------------------------------------------------------------------
// Modules

pub mod db;
pub mod dynamic;
pub mod impls;
pub mod info;
pub mod ops;
pub mod path;

pub use zlim_reflect_derive as derive;

// ----------------------------------------------------------------------------
// Top-Level exports

pub use ops::Reflect;
pub use path::TypePath;
