//! ECS system abstraction: the [`System`] trait, its [`SystemParam`] extractors,
//! system construction via [`IntoSystem`], and the scheduler access model.
//!
//! # Systems
//!
//! A *system* is a unit of logic that runs against a [`World`] (or a
//! [`WorldCell`] view of it).  The [`System`] trait is the type-erased
//! contract every runnable system implements.  Instead of implementing
//! [`System`] by hand, users build systems from plain functions and closures
//! through [`IntoSystem`], which wraps them in a function system.
//!
//! # System parameters
//!
//! System functions declare the data they need as *system parameters* — types
//! implementing [`SystemParam`].  The crate provides:
//!
//! - resource access: [`Res`] / [`ResMut`] (defined in `crate::borrow`),
//! - entity queries: [`Query`] / [`Single`] (defined in `crate::query`),
//! - system-local state: [`Local`],
//! - deferred commands: [`Commands`] (defined in `crate::command`),
//! - input values: [`In`] / [`InMut`] / [`InRef`],
//! - scheduling hints: [`ExclusiveMarker`] / [`NonSendMarker`].
//!
//! Tuples of parameters (up to 12 elements) also implement [`SystemParam`],
//! and custom parameters can be composed from existing ones with
//! `#[derive(SystemParam)]`.
//!
//! # Building and running systems
//!
//! Functions and closures are converted into systems with [`IntoSystem`]
//! (`into_system`, `pipe`, `map`, `with_input`).  Every system is identified
//! by a stable [`SystemId`] and can be cached in a [`World`] behind a
//! type-safe [`SystemHandle`].
//!
//! [`World`]: crate::world::World
//! [`WorldCell`]: crate::world::WorldCell
//! [`Res`]: crate::borrow::Res
//! [`ResMut`]: crate::borrow::ResMut
//! [`Query`]: crate::query::Query
//! [`Single`]: crate::query::Single
//! [`Commands`]: crate::command::Commands

mod access;
mod error;
mod function;
mod input;
mod into_system;
mod meta;
mod param;
mod params;
mod registry;
mod system;

pub use access::{AccessTable, ComponentAccess};
pub use access::{FilterParam, FilterParamBuilder};
pub use error::{SystemError, SystemParamError};
pub use input::SystemInput;
pub use input::{In, InMut, InRef};
pub use into_system::IntoSystem;
pub use meta::{SystemFlags, SystemId, SystemMeta};
pub use param::SystemParam;
pub use params::*;
pub use registry::{SystemHandle, Systems};
pub use system::System;

pub use zlim_core_derive::SystemParam;
