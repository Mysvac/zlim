//! The job system: schedulable units of work executed by schedules.
//!
//! # Jobs are special systems
//!
//! A [`Job`] is the runtime interface of a piece of game logic and can be
//! thought of as a **special kind of [`System`]**:
//!
//! - It holds a **stable string name** — a job is identified by a [`JobId`]
//!   combining a `name` and a `group` (both `&'static str`), so it can be
//!   looked up and referenced by name at runtime.
//!
//! - Its **input parameters are empty** — a job runs against the [`World`]
//!   with `System<Input = ()>`; it never takes a system input.
//!
//! - Its **output is normalized into the standard `ZlimResult`** — through
//!   [`IntoJobResult`] a job may return `()`, `bool`, `Result<(), E>` or
//!   `Result<bool, E>` (where `E: Into<ZlimError>`).
//!   Every form is mapped into the scheduler's standard `Result<(), SystemError>`,
//!   with failures wrapped in the standard [`ZlimError`]. A `false` / `Ok(false)`
//!   result maps to [`SystemError::None`] — a benign early exit that prevents
//!   dependent jobs from running.
//!
//! # Job groups
//!
//! A [`JobGroup`] organizes jobs into a batch: it gives the batch a name,
//! an optional run condition, and strong/weak/relaxed ordering chains
//! between its jobs.
//!
//! Every job belongs to a [`JobGroup`]: the group name is part of the
//! job's [`JobId`]. When no group is specified (a job is inserted into a
//! [`Schedule`] directly, or constructed with an empty group name) the job
//! belongs to the **anonymous group** ([`JobGroup::ANONYMOUS`], created via
//! [`JobId::isolated`]).
//!
//! # Defining jobs
//!
//! The `#[job_fn]`, `job!` and `job_group!` macros generate the type-level
//! labels ([`JobLabel`] / [`JobGroupLabel`]) together with their metadata:
//!
//! ## `#[job_fn]` — turn a plain function into a job
//!
//! The attribute generates a marker type implementing [`JobLabel`], whose
//! [`JobDB`] builds a boxed [`Job`] from the annotated function. For
//! non-generic markers the job auto-registers itself at startup.
//!
//! ```rust
//! use zlim_core::prelude::*;
//!
//! #[job_fn(type = GreetJob, name = "greet")]
//! fn greet() {}
//!
//! // The database constructor builds a boxed job for a group:
//! let mut job = (GreetJob::database().ctor)("my_group");
//! assert_eq!(job.id().name(), "greet");
//! assert_eq!(job.id().group(), "my_group");
//! ```
//!
//! `run_if` gates the job with one or more conditions (systems returning
//! `bool` or `Result<bool, E>`); `strict: false` relaxes access conflict
//! reporting:
//!
//! ```rust
//! use zlim_core::prelude::*;
//!
//! #[job_fn(
//!     type = GatedJob,
//!     name = "gated",
//!     strict = false,
//!     run_if = should_run,
//! )]
//! fn gated() {}
//!
//! fn should_run() -> bool {
//!     true
//! }
//!
//! assert!(!GatedJob::database().run_if.is_empty());
//! ```
//!
//! `run_if` support multi systems, required all to be met:
//!
//! ```text
//! run_if = [should_run1, should_run2],
//! ```
//!
//! ## `job!` — wrap an arbitrary system expression
//!
//! The function-like `job!` macro wraps any expression implementing
//! [`IntoSystem`] — a plain function, a closure, or a `pipe` pipeline —
//! into a job. The syntax uses `:` separators, and the `system` argument
//! names the expression:
//!
//! ```rust
//! use zlim_core::prelude::*;
//!
//! fn compute() {}
//!
//! job! {
//!     type: ComputeJob,
//!     name: "compute",
//!     system: compute,
//!     strict: true,
//! }
//!
//! let mut job = (ComputeJob::database().ctor)("my_group");
//! assert_eq!(job.id().name(), "compute");
//! ```
//!
//! ## `job_group!` — group jobs under one name
//!
//! The function-like `job_group!` macro generates a [`JobGroupLabel`]
//! marker. Job slots accept either a [`JobLabel`] type or a plain string
//! name; `order` / `weak_order` / `relaxed_order` declare ordering chains
//! over the slot names (see [`JobGroup`] for their semantics):
//!
//! ```rust
//! use zlim_core::prelude::*;
//!
//! #[job_fn(type = JobA, name = "group_job_a")]
//! fn job_a() {}
//!
//! job_group! {
//!     type: MyGroup,
//!     name: "my_group",
//!     jobs: [JobA, "group_job_b"],
//!     order: [["group_job_b", JobA]],
//! }
//!
//! let group = MyGroup::group();
//! assert_eq!(group.name, "my_group");
//!
//! // The job list is prefixed with internal begin/end markers:
//! assert_eq!(group.jobs[0].name(), "zlim_core::GroupBegin");
//! assert_eq!(group.jobs[1].name(), "zlim_core::GroupEnd");
//! assert_eq!(group.jobs[2].name(), "group_job_a");
//! ```
//!
//! # Registration
//!
//! [`JobDB`] holds the static metadata (name, constructor, run conditions,
//! source location) of every registered job; [`JobGroup`] holds the
//! resolved group metadata. `register_job!` / `register_job_group!` submit
//! [`JobLabel`] / [`JobGroupLabel`] types to the CTOR registry so they are
//! registered before `main`:
//!
//! ```rust
//! use zlim_core::prelude::*;
//!
//! #[job_fn(type = MyJob, name = "my_job")]
//! fn my_job() {}
//!
//! register_job!(MyJob);
//!
//! // Loads the statically-registered jobs into the global registry, once:
//! JobDB::collect();
//! assert!(JobDB::get("my_job").is_some());
//! ```
//!
//! Generic markers cannot be auto-registered (a CTOR static may not
//! reference generic parameters); they must be registered manually per
//! instantiation with [`JobDB::register`] / [`JobGroup::register`].
//!
//! [`World`]: crate::world::World
//! [`System`]: crate::system::System
//! [`IntoSystem`]: crate::system::IntoSystem
//! [`SystemError::None`]: crate::system::SystemError::None
//! [`Schedule`]: crate::schedule::Schedule
//! [`ZlimError`]: crate::error::ZlimError

mod db;
mod group;
mod ident;
mod into_job;
mod job;

pub use db::{JobDB, JobLabel};
pub use group::{JobGroup, JobGroupLabel};
pub use ident::JobId;
pub use into_job::{IntoJob, IntoJobResult};
pub use job::Job;

#[doc(hidden)]
pub use db::__JobReg__;

#[doc(hidden)]
pub use group::__JobGroupReg__;

pub use zlim_core_derive::{job, job_fn, job_group};
