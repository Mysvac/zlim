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
pub mod schedule;
pub mod system;
pub mod table;
pub mod template;
pub mod tick;
pub mod time;
pub mod utils;
pub mod world;

// -----------------------------------------------------------------------------
// Macro Exports

/// Internal module, public for derive macros.
#[doc(hidden)]
pub mod __macro_exports__ {
    pub use zlim_ptr::OwningPtr;
    pub use zlim_reflect::Reflect;
    pub use zlim_reflect::TypePath;
    pub use zlim_reflect::db::TypeDatabase;
    pub use zlim_reflect::derive::TypePath as TypePathDerive;
    pub use zlim_reg::submit;
    pub use zlim_utils::debug::DebugLocation;
    pub use zlim_utils::str::intern_str;
}

// -----------------------------------------------------------------------------
// Jobs

/// zlim-core jobs
pub mod jobs {
    #[doc(inline)]
    pub use crate::message::jobs::UpdateMessagesSignal;
    #[doc(inline)]
    pub use crate::time::jobs::OptimizeDelayedCommands;
}

// -----------------------------------------------------------------------------
// macros

/// zlim-core macros
pub mod derive {
    #[doc(no_inline)]
    pub use crate::register_component;
    #[doc(no_inline)]
    pub use crate::register_job;
    #[doc(no_inline)]
    pub use crate::register_job_group;
    #[doc(no_inline)]
    pub use crate::register_resource;
    #[doc(inline)]
    pub use zlim_core_derive::{Bundle, Error, IntoTemplate};
    #[doc(inline)]
    pub use zlim_core_derive::{Component, Message, Resource};
    #[doc(inline)]
    pub use zlim_core_derive::{EntityLabel, QueryData, SystemParam};
    #[doc(inline)]
    pub use zlim_core_derive::{ScheduleLabel, ScheduleStage};
    #[doc(inline)]
    pub use zlim_core_derive::{job, job_fn, job_group};
}

// -----------------------------------------------------------------------------
// Prelude

/// zlim-core preludes
pub mod prelude {
    // doc(hidden): keeps this path out of autocomplete suggestions.

    #[doc(no_inline)]
    pub use crate::derive::{job, job_fn, job_group};

    #[doc(no_inline)]
    pub use crate::tick::{DetectChanges, DetectChangesMut, Tick};

    #[doc(no_inline)]
    pub use crate::world::{DeferredWorld, World, WorldCell};
    #[doc(no_inline)]
    pub use crate::world::{FromWorld, NonSendWorld, WorldId};

    // implicit use zlim_core_derive::Error
    #[doc(no_inline)]
    pub use crate::error::{Error, Severity, ZlimError};

    // implicit use zlim_core_derive::Resource
    #[doc(no_inline)]
    pub use crate::resource::{Resource, ResourceDB, ResourceId};

    #[doc(no_inline)]
    pub use crate::entity::{EntityId, EntityMap, EntityMapper};
    #[doc(no_inline)]
    pub use crate::entity::{EntityLabel, MapEntities};

    // implicit use zlim_core_derive::Component
    #[doc(no_inline)]
    pub use crate::component::{Component, ComponentDB, ComponentId};
    #[doc(no_inline)]
    pub use crate::component::{ComponentHook, HookContext};

    #[doc(no_inline)]
    pub use crate::ops::{Entity, EntityMut, EntityOwned, EntityRef};

    #[doc(no_inline)]
    pub use crate::borrow::{Mut, Ref, SliceMut, SliceRef};
    #[doc(no_inline)]
    pub use crate::borrow::{NonSend, NonSendMut, Res, ResMut};

    // implicit use zlim_core_derive::Bundle
    #[doc(no_inline)]
    pub use crate::bundle::Bundle;

    #[doc(no_inline)]
    pub use crate::command::{Command, EntityCommand};
    #[doc(no_inline)]
    pub use crate::command::{Commands, EntityCommands};

    #[doc(no_inline)]
    pub use crate::clone::EntityCloner;

    // implicit use zlim_core_derive::IntoTemplate
    #[doc(no_inline)]
    pub use crate::template::{EntityTemplate, IntoTemplate, Template, TemplateContext};

    #[doc(no_inline)]
    pub use crate::job::{IntoJob, Job, JobDB, JobId};
    #[doc(no_inline)]
    pub use crate::job::{JobGroup, JobGroupLabel, JobLabel};

    // implicit use zlim_core_derive::Message
    #[doc(no_inline)]
    pub use crate::message::MessageCursor;
    #[doc(no_inline)]
    pub use crate::message::{Message, MessageId, MessageKey, MessageQueue};
    #[doc(no_inline)]
    pub use crate::message::{MessageMutator, MessageReader, MessageWriter};

    #[doc(no_inline)]
    pub use crate::message::{ClampTickSignal, ReparentSignal};

    #[doc(no_inline)]
    pub use crate::derive::QueryData;
    #[doc(no_inline)]
    pub use crate::query::{Added, And, Changed, Children, Or, Parent, With, Without};
    #[doc(no_inline)]
    pub use crate::query::{Query, QueryIter, QuerySlice, QuerySliceIter, Single};
    #[doc(no_inline)]
    pub use crate::query::{QuerySingleError, QueryState, ReadOnlyQueryData};

    // implicit use zlim_core_derive::{ScheduleLabel, ScheduleStage}
    #[doc(no_inline)]
    pub use crate::schedule::{Schedule, ScheduleLabel, ScheduleStage, Schedules};

    #[doc(no_inline)]
    pub use crate::derive::SystemParam;
    #[doc(no_inline)]
    pub use crate::system::{ExclusiveMarker, If, Local, NonSendMarker};
    #[doc(no_inline)]
    pub use crate::system::{In, InMut, InRef, IntoSystem, System};
    #[doc(no_inline)]
    pub use crate::system::{SystemError, SystemHandle, SystemId};

    #[doc(no_inline)]
    pub use crate::time::{Fixed, Moment, Real, Time, TimeState, Virtual};
    #[doc(no_inline)]
    pub use crate::time::{TimeSnapshot, TimeUpdateStrategy, Timer, TimerMode};
}
