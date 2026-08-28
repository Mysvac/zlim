#![doc = include_str!("../README.md")]
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
    pub use zlim_ptr::OwningPtr as __OwningPtr;
    pub use zlim_reflect::derive::TypePath as __TypePathDerive;
    pub use zlim_reflect::ops::Reflect as __Reflect;
    pub use zlim_reflect::path::TypePath as __TypePath;
    pub use zlim_reg::submit as __submit;
    pub use zlim_utils::debug::DebugLocation as __DebugLocation;
    pub use zlim_utils::str::intern_str as __intern_str;
}

// -----------------------------------------------------------------------------
// Prelude

/// zlim-core prelude
pub mod prelude {
    pub use crate::{register_component, register_job, register_job_group, register_resource};
    pub use zlim_core_derive::{job, job_fn, job_group};
    pub use zlim_reflect::derive::TypePath;

    pub use crate::tick::{DetectChanges, DetectChangesMut, Tick};

    pub use crate::world::{DeferredWorld, World, WorldCell};
    pub use crate::world::{FromWorld, NonSendWorld, WorldId};

    pub use crate::error::{Error, Severity, ZlimError};

    pub use crate::resource::{Resource, ResourceDB};

    pub use crate::entity::{EntityId, EntityMap, EntityMapper, MapEntities};

    pub use crate::component::{Component, ComponentDB, ComponentId, Required};
    pub use crate::component::{ComponentHook, Components, HookContext};

    pub use crate::ops::{Entity, EntityMut, EntityOwned, EntityRef};

    pub use crate::borrow::{Mut, Ref, SliceMut, SliceRef};
    pub use crate::borrow::{NonSend, NonSendMut, Res, ResMut};

    pub use crate::bundle::{Bundle, DataBundle};

    pub use crate::command::{Command, EntityCommand};
    pub use crate::command::{CommandQueue, Commands, EntityCommands};

    pub use crate::clone::EntityCloner;

    pub use crate::job::{IntoJob, Job, JobDB, JobId};
    pub use crate::job::{JobGroup, JobGroupLabel, JobLabel};

    pub use crate::message::MessageCursor;
    pub use crate::message::{Message, MessageId, MessageKey, MessageQueue};
    pub use crate::message::{MessageMutator, MessageReader, MessageWriter};

    pub use crate::query::{Added, And, Changed, Or, Query, With, Without};
    pub use crate::query::{ArchetypeFilter, Children, Parent, QueryData, QueryFilter, QuerySlice};
    pub use crate::query::{QueryIter, QuerySingleError, QuerySliceIter, Single};
    pub use crate::query::{QueryState, ReadOnlyQueryData};

    pub use crate::schedule::{Schedule, ScheduleLabel, ScheduleStage, Schedules};

    pub use crate::system::{ExclusiveMarker, If, Local, NonSendMarker};
    pub use crate::system::{In, InMut, InRef, IntoSystem, SystemParam};
    pub use crate::system::{System, SystemError, SystemHandle, SystemId};

    pub use crate::time::{Fixed, Real, Time, TimeState, Virtual};
    pub use crate::time::{TimeSnapshot, TimeUpdateStrategy, Timer, TimerMode};
}
