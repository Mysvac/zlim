//! Scheduling: building and executing job graphs.
//!
//! A [`Schedule`] owns the jobs (and job groups) it should run, derives a
//! dependency graph from their ordering/access constraints, and dispatches
//! them through a [`JobExecutor`] — [`SingleThreadedExecutor`] or
//! [`MultiThreadedExecutor`]. [`Schedules`] is the per-world collection of
//! named schedules.
//!
//! # Example
//!
//! ```rust
//! use zlim_core::prelude::*;
//! use zlim_core::schedule::ScheduleLabel;
//!
//! /// Labels identify schedules within a world's [`Schedules`] collection.
//! #[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
//! enum Stage {
//!     Update,
//!     Render,
//! }
//!
//! #[job_fn(type = RenderFrame, name = "render_frame")]
//! fn render_frame() {}
//!
//! // Schedules are stored per world and executed by label.
//! let mut world = World::alloc();
//! world.schedules_mut().entry(Stage::Update).insert::<RenderFrame>();
//! assert!(world.schedules().contains(Stage::Update));
//! world.run_schedule(Stage::Update);
//! ```
//!
//! [`Schedule`]: crate::schedule::Schedule
//! [`JobExecutor`]: crate::schedule::JobExecutor
//! [`SingleThreadedExecutor`]: crate::schedule::SingleThreadedExecutor
//! [`MultiThreadedExecutor`]: crate::schedule::MultiThreadedExecutor
//! [`Schedules`]: crate::schedule::Schedules

// -----------------------------------------------------------------------------
// Modules

mod executor;
mod graph;
mod label;
mod schedule;
mod schedules;

pub use executor::{ConflictTable, JobExecutor, JobSchedule, JobScheduleView};
pub use executor::{ExecutorKind, MultiThreadedExecutor, SingleThreadedExecutor};
pub use graph::{Dag, DiGraph, Node, SccIterator, SccNodes, ToposortError};
pub use label::{AnonymousSchedule, InternedScheduleLabel, ScheduleLabel};
pub use schedule::Schedule;
pub use schedules::Schedules;

// -----------------------------------------------------------------------------
