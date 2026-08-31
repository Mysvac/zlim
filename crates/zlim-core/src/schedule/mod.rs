//! Scheduling: building and executing job graphs.
//!
//! A [`Schedule`] owns the jobs, job groups, and stages it should run,
//! derives a dependency graph from their ordering and access constraints,
//! and dispatches them through a [`JobExecutor`] —
//! [`SingleThreadedExecutor`] or [`MultiThreadedExecutor`].  [`Schedules`]
//! is the per-world collection of named schedules.
//!
//! # Hierarchy
//!
//! `Schedule > ScheduleStage > JobGroup > Job`:
//!
//! - [`Job`] — a unit of logic against the world, produced from plain
//!   functions with [`job_fn`] / `job!`.
//! - [`JobGroup`] — a named collection of jobs with ordering constraints
//!   between them, declared with `job_group!`.
//! - [`ScheduleStage`] — a named grouping of jobs and job groups with a
//!   begin/end marker pair, declared with `#[derive(ScheduleStage)]`.
//! - [`Schedule`] — the executable container; owns all of the above plus
//!   the compiled [`JobSchedule`] and an executor.
//!
//! # Stages
//!
//! **Every job and every job group belongs to a stage.**  The insert APIs
//! ([`Schedule::insert`], [`Schedule::insert_group`], and the `*_by_name`
//! variants) take a [`ScheduleStage`] value for this purpose:
//!
//! - Inserting into the **anonymous stage** `()` records no stage at all —
//!   no begin/end markers are created and no `StageEntry` exists.
//! - Inserting into a **named stage** auto-creates it, inserting its
//!   `StageBegin` / `StageEnd` marker jobs.  The stage adds ordering only:
//!   the job/group keeps its own group value, `begin` runs **strong-before**
//!   the stage's jobs (and each group's `GroupBegin`), and `end` runs
//!   **weak-after** them (each group's `GroupEnd`).
//!
//! Stages can also be created or removed directly with
//! [`Schedule::insert_stage`] / [`Schedule::remove_stage`]; removing a
//! stage cascades to every job and group recorded in it.
//!
//! # Ordering
//!
//! [`JobGroup`] and [`Schedule::insert_order`] /
//! [`Schedule::insert_weak_order`] / [`Schedule::insert_relaxed_order`]
//! express three levels of ordering constraints:
//!
//! - `order` (strong) — the next job runs only after the previous job
//!   completed **successfully**, and its deferred commands are definitely
//!   visible.
//! - `weak_order` — the next job runs after the previous job completed,
//!   whether successfully or skipped, and its deferred commands are
//!   definitely visible.
//! - `relaxed_order` — like `weak_order`, but deferred commands may not be
//!   visible yet.
//!
//! Strong edges also gate the *run condition*: if a strong predecessor is
//! skipped, its successors are skipped as well.  In multi-threaded mode the
//! schedule inserts internal `ApplyDeferred` sync points for deferred jobs
//! so the visibility guarantees hold.
//!
//! # Executors
//!
//! The default executor is chosen from [`ExecutorKind::default`]
//! (multi-threaded when the task pool supports it).  [`Schedule::with_executor`]
//! plugs in a specific one: [`SingleThreadedExecutor`] runs jobs serially in
//! topological order, [`MultiThreadedExecutor`] runs independent jobs in
//! parallel while serializing access-conflicting ones via a
//! [`ConflictTable`].
//!
//! # Example
//!
//! ```rust
//! use zlim_core::prelude::*;
//! use zlim_core::job::JobDB;
//!
//! /// Labels identify schedules within a world's [`Schedules`] collection.
//! #[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
//! enum MainLoop {
//!     Update,
//!     Render,
//! }
//!
//! /// Stages group jobs inside a schedule.
//! #[derive(TypePath, ScheduleStage)]
//! enum FixedStage {
//!     PreUpdate,
//!     Update,
//! }
//!
//! #[job_fn(type = PhysicsStep, name = "physics_step")]
//! fn physics_step() {}
//!
//! JobDB::collect();
//!
//! // Schedules are stored per world and executed by label.  Every job
//! // belongs to a stage; a named stage is auto-created with begin/end
//! // markers, while the anonymous stage `()` carries none.
//! let mut world = World::alloc();
//! world
//!     .schedules_mut()
//!     .entry(MainLoop::Update)
//!     .insert::<PhysicsStep>(FixedStage::Update);
//!
//! assert!(world.schedules().contains(MainLoop::Update));
//! world.run_schedule(MainLoop::Update);
//! ```
//!
//! [`Schedule`]: crate::schedule::Schedule
//! [`ScheduleStage`]: crate::schedule::ScheduleStage
//! [`Schedules`]: crate::schedule::Schedules
//! [`JobSchedule`]: crate::schedule::JobSchedule
//! [`JobExecutor`]: crate::schedule::JobExecutor
//! [`SingleThreadedExecutor`]: crate::schedule::SingleThreadedExecutor
//! [`MultiThreadedExecutor`]: crate::schedule::MultiThreadedExecutor
//! [`ConflictTable`]: crate::schedule::ConflictTable
//! [`ExecutorKind::default`]: crate::schedule::ExecutorKind::default
//! [`Schedule::insert`]: crate::schedule::Schedule::insert
//! [`Schedule::insert_group`]: crate::schedule::Schedule::insert_group
//! [`Schedule::insert_stage`]: crate::schedule::Schedule::insert_stage
//! [`Schedule::remove_stage`]: crate::schedule::Schedule::remove_stage
//! [`Schedule::insert_order`]: crate::schedule::Schedule::insert_order
//! [`Schedule::insert_weak_order`]: crate::schedule::Schedule::insert_weak_order
//! [`Schedule::insert_relaxed_order`]: crate::schedule::Schedule::insert_relaxed_order
//! [`Schedule::with_executor`]: crate::schedule::Schedule::with_executor
//! [`Job`]: crate::job::Job
//! [`JobGroup`]: crate::job::JobGroup
//! [`job_fn`]: zlim_core_derive::job_fn

// -----------------------------------------------------------------------------
// Modules

mod executor;
mod graph;
mod label;
mod schedule;
mod schedules;
mod stage;

pub use executor::{ConflictTable, JobExecutor, JobSchedule, JobScheduleView};
pub use executor::{ExecutorKind, MultiThreadedExecutor, SingleThreadedExecutor};
pub use graph::{Dag, DiGraph, Node, SccIterator, SccNodes, ToposortError};
pub use label::{AnonymousSchedule, InternedScheduleLabel, ScheduleLabel};
pub use schedule::Schedule;
pub use schedules::{MissingSchedule, Schedules};
pub use stage::ScheduleStage;

pub use zlim_core_derive::{ScheduleLabel, ScheduleStage};

// -----------------------------------------------------------------------------
