#![expect(clippy::len_without_is_empty, reason = "useless")]
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
// Modules

pub mod borrow;
pub mod bundle;
pub mod clone;
pub mod command;
pub mod component;
pub mod entity;
pub mod error;
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
// Macro Exports

/// Internal module, public for dervie macros.
#[doc(hidden)]
pub mod __macro_exports__ {
    pub use serde::Deserialize as __Deserialize;
    pub use serde::Serialize as __Serialize;
    pub use zlim_reflect::ops::Reflect as __Reflect;
    pub use zlim_reflect::path::TypePath as __TypePath;
}

// -----------------------------------------------------------------------------
// Prelude

pub mod prelude {
    pub use crate::{register_component, register_resource};

    pub use crate::bundle::{Bundle, DataBundle};
    pub use crate::clone::EntityCloner;
    pub use crate::command::{Command, EntityCommand};
    pub use crate::component::Component;
    pub use crate::entity::{EntityId, EntityMap, MapEntities};
    pub use crate::error::{Error, Severity, ZlimError};
    pub use crate::ops::{EntityMut, EntityOwned, EntityRef};
    pub use crate::resource::Resource;
    pub use crate::tick::{DetectChanges, DetectChangesMut};
    pub use crate::world::{DeferredWorld, World};
}
