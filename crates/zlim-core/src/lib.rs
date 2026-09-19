#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, expect(internal_features, reason = "needed for fake_variadic"))]
#![cfg_attr(docsrs, feature(doc_cfg, rustdoc_internals))]
#![expect(unsafe_code, reason = "performance optimization")]

// -----------------------------------------------------------------------------

/// Compilation configurations.
pub mod cfg {
    zlim_cfg::define_alias! {
        #[cfg(any(feature = "debug", debug_assertions))] => debug,
        #[cfg(feature = "backtrace")] => backtrace,
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
pub use zlim_core_derive::{job, job_fn, job_group};

// -----------------------------------------------------------------------------
// Modules

pub mod borrow;
pub mod bundle;
pub mod clone;
pub mod command;
pub mod component;
pub mod entity;
pub mod error;
pub mod init;
pub mod job;
pub mod label;
pub mod message;
pub mod ops;
pub mod query;
pub mod resource;
pub mod scene;
pub mod schedule;
pub mod system;
pub mod table;
pub mod tick;
pub mod time;
pub mod utils;
pub mod world;

// -----------------------------------------------------------------------------
// Macro Exports

/// Internal module, public for derive macros.
#[doc(hidden)]
pub mod __macro_exports__ {
    pub use serde::Deserialize as __Deserialize;
    pub use serde::Serialize as __Serialize;
    pub use zlim_path::TypePath as __TypePath;
    pub use zlim_path::derive::TypePath as __TypePathDerive;
    pub use zlim_ptr::OwningPtr as __OwningPtr;
    pub use zlim_reg::submit as __submit;
    pub use zlim_utils::debug::DebugLocation as __DebugLocation;
    pub use zlim_utils::str::intern_str as __intern_str;
}

// -----------------------------------------------------------------------------
// Prelude

/// zlim-core prelude
pub mod prelude {
    // doc(hidden): keeps this path out of autocomplete suggestions.

    #[doc(hidden)]
    pub use zlim_core_derive::{job, job_fn, job_group};

    #[doc(hidden)]
    pub use crate::tick::{DetectChanges, DetectChangesMut, Tick};

    #[doc(hidden)]
    pub use crate::world::{DeferredWorld, World, WorldCell};
    #[doc(hidden)]
    pub use crate::world::{FromWorld, NonSendWorld, WorldId};

    // implicit use zlim_core_derive::Error
    #[doc(hidden)]
    pub use crate::error::{Error, Severity, ZlimError};

    // implicit use zlim_core_derive::Resource
    #[doc(hidden)]
    pub use crate::resource::{Resource, ResourceDB, ResourceId};

    #[doc(hidden)]
    pub use crate::entity::{EntityId, EntityMap, EntityMapper, MapEntities};

    // implicit use zlim_core_derive::Component
    #[doc(hidden)]
    pub use crate::component::{Component, ComponentDB, ComponentId};
    #[doc(hidden)]
    pub use crate::component::{ComponentHook, HookContext};

    #[doc(hidden)]
    pub use crate::ops::{Entity, EntityMut, EntityOwned, EntityRef};

    #[doc(hidden)]
    pub use crate::borrow::{Mut, Ref, SliceMut, SliceRef};
    #[doc(hidden)]
    pub use crate::borrow::{NonSend, NonSendMut, Res, ResMut};

    // implicit use zlim_core_derive::Bundle
    #[doc(hidden)]
    pub use crate::bundle::{Bundle, DataBundle};

    #[doc(hidden)]
    pub use crate::command::{Command, EntityCommand};
    #[doc(hidden)]
    pub use crate::command::{Commands, EntityCommands};

    #[doc(hidden)]
    pub use crate::clone::EntityCloner;

    #[doc(hidden)]
    pub use crate::job::{IntoJob, Job, JobDB, JobId};
    #[doc(hidden)]
    pub use crate::job::{JobGroup, JobGroupLabel, JobLabel};

    // implicit use zlim_core_derive::Message
    #[doc(hidden)]
    pub use crate::message::MessageCursor;
    #[doc(hidden)]
    pub use crate::message::{Message, MessageId, MessageKey, MessageQueue};
    #[doc(hidden)]
    pub use crate::message::{MessageMutator, MessageReader, MessageWriter};

    #[doc(hidden)]
    pub use crate::message::{ClampTickSignal, ReparentSignal};

    #[doc(hidden)]
    pub use crate::query::{Added, And, Changed, Children, Or, Parent, With, Without};
    #[doc(hidden)]
    pub use crate::query::{Query, QueryIter, QuerySlice, QuerySliceIter, Single};
    #[doc(hidden)]
    pub use crate::query::{QuerySingleError, QueryState, ReadOnlyQueryData};
    #[doc(hidden)]
    pub use zlim_core_derive::QueryData;

    // implicit use zlim_core_derive::{ScheduleLabel, ScheduleStage}
    #[doc(hidden)]
    pub use crate::schedule::{Schedule, ScheduleLabel, ScheduleStage, Schedules};

    #[doc(hidden)]
    pub use crate::system::{ExclusiveMarker, If, Local, NonSendMarker};
    #[doc(hidden)]
    pub use crate::system::{In, InMut, InRef, IntoSystem, System};
    #[doc(hidden)]
    pub use crate::system::{SystemError, SystemHandle, SystemId};
    #[doc(hidden)]
    pub use zlim_core_derive::SystemParam;

    #[doc(hidden)]
    pub use crate::time::{Fixed, Real, Time, TimeState, Virtual};
    #[doc(hidden)]
    pub use crate::time::{TimeSnapshot, TimeUpdateStrategy, Timer, TimerMode};
}
