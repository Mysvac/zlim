//! The job system: schedulable units of work executed by schedules.
//!
//! A [`Job`] is the runtime interface for a piece of game logic. Jobs are
//! described by [`JobDB`] metadata, identified by a [`JobId`], and grouped
//! into [`JobGroup`]s via [`JobGroupLabel`].
//!
//! - `#[job_fn]` turns a plain function into a job, and `job!` wraps an
//!   arbitrary system expression (e.g. a `pipe` pipeline); both generate a
//!   [`JobLabel`] marker type together with a [`JobDB`] descriptor.
//! - `job_group!` generates a [`JobGroupLabel`] marker type that groups jobs
//!   with an optional run condition and strong/weak ordering chains.
//! - [`IntoJob`] converts functions and systems into boxed jobs, choosing
//!   between strict and permissive access registration.
//! - [`JobDB::collect`] and [`JobGroup::collect`] load the statically
//!   registered jobs and groups into their registries, typically once at
//!   startup; `register_job!` / `register_job_group!` add [`JobLabel`] /
//!   [`JobGroupLabel`] types to the CTOR registry.

mod db;
mod group;
mod ident;
mod into_job;
mod job;

pub use db::{JobDB, JobLabel, JobReg};
pub use group::{JobGroup, JobGroupLabel, JobGroupReg};
pub use ident::JobId;
pub use into_job::{IntoJob, IntoJobResult};
pub use job::Job;

pub use zlim_core_derive::{job, job_fn, job_group};
