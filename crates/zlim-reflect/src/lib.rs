#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, expect(internal_features, reason = "needed for fake_variadic"))]
#![cfg_attr(docsrs, feature(doc_cfg, rustdoc_internals))]

// -----------------------------------------------------------------------------

/// compilation configurations
pub mod cfg {
    zlim_cfg::define_alias! {
        #[cfg(any(feature = "debug", debug_assertions))] => debug,
    }
}

// -----------------------------------------------------------------------------

// Usually, we need to use `crate` in the crate itself and use `zlim_*` in
// doc testing. `zlim_derive_utils::crate_path` choose `zlim_*`, so we must
// have an `extern self` to ensure it can be used as an alias for `crate`.
extern crate self as zlim_reflect;

// -----------------------------------------------------------------------------
// Modules

pub mod db;
pub mod dynamic;
pub mod info;
pub mod ops;
pub mod path;
pub mod remote;
pub mod serde;

#[doc(hidden)]
pub mod impls;

// -----------------------------------------------------------------------------
// Top-Level exports

pub use db::TypeDB;

// implicit use derive::Reflect
pub use ops::Reflect;

// implicit use derive::TypePath
pub use path::TypePath;

// -----------------------------------------------------------------------------
// Macros

/// The reflect macros.
pub mod derive {
    #[doc(no_inline)]
    pub use crate::register_reflect;
    #[doc(inline)]
    pub use zlim_reflect_derive::Reflect;
    #[doc(inline)]
    pub use zlim_reflect_derive::TypePath;
    #[doc(inline)]
    pub use zlim_reflect_derive::impl_reflect;
}

// -----------------------------------------------------------------------------
// Prelude

/// The reflect preludes.
pub mod prelude {
    #[doc(no_inline)]
    pub use crate::db::TypeDB;
    // implicit use derive::Reflect
    #[doc(no_inline)]
    pub use crate::ops::Reflect;
    // implicit use derive::TypePath
    #[doc(no_inline)]
    pub use crate::path::TypePath;
}
