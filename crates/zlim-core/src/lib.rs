#![expect(unsafe_code, reason = "performance optimization")]
#![cfg_attr(docsrs, feature(doc_cfg))]

// -----------------------------------------------------------------------------

/// compilation configurations
pub mod cfg {
    zlim_cfg::define_alias! {
        #[cfg(any(feature = "debug", debug_assertions))] => debug,
    }
}

// -----------------------------------------------------------------------------
// Extern Self

// Usually, we need to use `crate` in the crate itself and use `zlim_*` in
// doc testing. `zlim_derive_utils::crate_path` choose `zlim_*`, so we must
// have an `extern self` to ensure it can be used as an alias for `crate`.
extern crate self as zlim_core;

// -----------------------------------------------------------------------------
// Macros

pub use zlim_core_derive as derive;

// -----------------------------------------------------------------------------
// Re-exports

pub use zlim_ptr::OwningPtr;

// -----------------------------------------------------------------------------
// Modules

pub mod borrow;
pub mod bundle;
pub mod clone;
pub mod component;
pub mod entity;
pub mod error;
pub mod message;
pub mod ops;
pub mod resource;
pub mod scene;
pub mod schedule;
pub mod script;
pub mod slot;
pub mod table;
pub mod tick;
pub mod utils;
pub mod world;

// -----------------------------------------------------------------------------
// Modules

#[doc(hidden)]
pub mod __macro_exports__ {
    pub use serde::Deserialize as __Deserialize;
    pub use serde::Serialize as __Serialize;
    pub use zlim_reflect::ops::Reflect as __Reflect;
    pub use zlim_reflect::path::TypePath as __TypePath;
}
