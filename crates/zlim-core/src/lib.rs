#![cfg_attr(docsrs, expect(internal_features, reason = "needed for fake_variadic"))]
#![cfg_attr(docsrs, feature(doc_cfg, rustdoc_internals))]
#![expect(unsafe_code, reason = "performance optimization")]

//! The ECS (entity–component–system) core of the `zlim` game engine.
//!
//! This crate provides the data model and orchestration layers that make up
//! the engine's runtime:
//!
//! - [`World`], the central container holding entities, components, resources,
//!   messages, and schedules, with a safe-access layer ([`WorldCell`] and
//!   [`DeferredWorld`]).
//! - Entities ([`EntityId`]) and components ([`Component`]) stored in
//!   columnar [`table`]s, composed into [`bundle`]s, and referenced through
//!   the [`borrow`] system.
//! - The [`job`] system ([`Job`], [`JobDB`], [`JobGroup`]) that turns
//!   functions into schedulable units of work.
//! - The [`schedule`] system ([`Schedule`], [`Schedules`], and single- or
//!   multi-threaded executors) that orders and runs jobs.
//! - A double-buffered [`message`] pipeline ([`Message`], [`MessageQueue`],
//!   and the [`MessageReader`] / [`MessageWriter`] / [`MessageMutator`] system
//!   parameters).
//! - Wrap-around-safe change detection through [`tick`]s.
//! - The [`error`] types ([`ZlimError`], [`Severity`]) used across the engine.
//!
//! Most user-facing types are re-exported from [`prelude`].
//!
//! [`World`]: world::World
//! [`WorldCell`]: world::WorldCell
//! [`DeferredWorld`]: world::DeferredWorld
//! [`EntityId`]: entity::EntityId
//! [`Component`]: component::Component
//! [`table`]: table
//! [`bundle`]: bundle
//! [`borrow`]: borrow
//! [`job`]: mod@job
//! [`Job`]: job::Job
//! [`JobDB`]: job::JobDB
//! [`JobGroup`]: job::JobGroup
//! [`schedule`]: schedule
//! [`Schedule`]: schedule::Schedule
//! [`Schedules`]: schedule::Schedules
//! [`message`]: message
//! [`Message`]: message::Message
//! [`MessageQueue`]: message::MessageQueue
//! [`MessageReader`]: message::MessageReader
//! [`MessageWriter`]: message::MessageWriter
//! [`MessageMutator`]: message::MessageMutator
//! [`tick`]: tick
//! [`error`]: error
//! [`ZlimError`]: error::ZlimError
//! [`Severity`]: error::Severity
//! [`prelude`]: prelude

// -----------------------------------------------------------------------------

/// Compilation configurations.
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
pub mod slot;
pub mod system;
pub mod table;
pub mod tick;
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
}

// -----------------------------------------------------------------------------
// Prelude

pub mod prelude {
    pub use crate::{register_component, register_job, register_job_group, register_resource};
    pub use zlim_core_derive::{job, job_fn, job_group};
    pub use zlim_reflect::TypePath;

    pub use crate::borrow::{Mut, NonSend, NonSendMut, Ref, Res, ResMut, SliceMut, SliceRef};
    pub use crate::bundle::{Bundle, DataBundle};
    pub use crate::clone::EntityCloner;
    pub use crate::command::CommandQueue;
    pub use crate::command::{Command, Commands, EntityCommand, EntityCommands};
    pub use crate::component::{Component, ComponentDB, Required};
    pub use crate::component::{ComponentHook, ComponentId, Components, HookContext};
    pub use crate::entity::{EntityId, EntityMap, EntityMapper, MapEntities};
    pub use crate::error::{Error, Severity, ZlimError};
    pub use crate::job::{IntoJob, Job, JobDB, JobGroup, JobGroupLabel, JobId, JobLabel};
    pub use crate::message::MessageCursor;
    pub use crate::message::{Message, MessageId, MessageKey, MessageQueue};
    pub use crate::message::{MessageMutator, MessageReader, MessageWriter};
    pub use crate::ops::{Entity, EntityMut, EntityOwned, EntityRef};
    pub use crate::query::{Added, And, Changed, Or, Query, With, Without};
    pub use crate::query::{ArchetypeFilter, QueryData, QueryFilter, QuerySlice};
    pub use crate::query::{QueryIter, QuerySingleError, QuerySliceIter, Single};
    pub use crate::query::{QueryState, ReadOnlyQueryData};
    pub use crate::resource::{Resource, ResourceDB};
    pub use crate::schedule::{Schedule, ScheduleLabel, Schedules};
    pub use crate::system::{ExclusiveMarker, Local, NonSendMarker};
    pub use crate::system::{In, InMut, InRef, IntoSystem, SystemParam};
    pub use crate::system::{System, SystemError, SystemHandle, SystemId};
    pub use crate::tick::{DetectChanges, DetectChangesMut, Tick};
    pub use crate::world::{DeferredWorld, World, WorldCell};
    pub use crate::world::{FromWorld, WorldId};
}
