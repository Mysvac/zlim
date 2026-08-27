//! Time tracking and timing utilities.
//!
//! Time lives in **resources**, advanced automatically by the engine once per
//! frame; systems just read it:
//!
//! - [`Time`] — the game clock most systems read (delta, elapsed, …).
//! - [`Time<Real>`] — wall-clock time.
//! - [`Time<Virtual>`] — game time with pausing and speed scaling.
//! - [`Time<Fixed>`] — fixed-timestep time, advanced in whole steps.
//! - [`TimeSnapshot`] — a frozen per-frame view for interpolation, e.g. in a
//!   render world.
//!
//! In most cases, [`Time`] is a mirror of [`Time<Virtual>`].
//!
//! But in the FixedMain loop of the app, [`Time`] will be a mirror of [`Time<Fixed>`].
//!
//! # Examples
//!
//! A system reads the game clock directly from its resource:
//!
//! ```rust, no_run
//! # use zlim_core::prelude::*;
//! #
//! fn update(time: Res<Time>) {
//!     // Frame-rate-independent logic uses `time.delta()`.
//!     let _ = time.delta();
//! }
//! ```
//!
//! # Update Strategy
//!
//! See [`TimeUpdateStrategy`] for details.
//!
//! By default, the [`Time`], [`Time<Real>`], [`Time<Virtual>`] is updated by
//! [`World::refresh_metadata`] function. [`Time<Fixed>`] and [`TimeSnapshot`]
//! is updated by [`World::step_fixed`] function.
//!
//! For the sub world, consider disabling default updates and then copying time
//! from the main world in the app's world-extract stage.
//!
//! # Timers
//!
//! [`Timer`] (with [`TimerMode`]) is a countdown timer built on
//! [`Stopwatch`]; both track elapsed time and support pausing.
//!
//! # Conditions & delayed commands
//!
//! `conditions` provides timing-based run conditions (`on_timer`,
//! `once_after_delay`, …), and `delayed` lets [`Commands`] queue commands
//! that are applied after a delay (see [`Commands::delayed`]).
//!
//! [`World::step_fixed`]: crate::world::World::step_fixed
//! [`World::refresh_metadata`]: crate::world::World::refresh_metadata
//! [`Commands`]: crate::command::Commands
//! [`Commands`]: crate::command::Commands
//! [`Commands::delayed`]: crate::command::Commands::delayed

// -----------------------------------------------------------------------------
// Modules

mod cache;
mod conditions;
mod delayed;
mod fixed;
mod real;
mod state;
mod stopwatch;
mod strategy;
mod time;
mod timer;
mod virt;

// -----------------------------------------------------------------------------
// Re-exports

pub use conditions::*;
pub use fixed::Fixed;
pub use real::Real;
pub use state::{TimeSnapshot, TimeState};
pub use stopwatch::Stopwatch;
pub use strategy::TimeUpdateStrategy;
pub use time::{Time, TimeContext};
pub use timer::{Timer, TimerMode};
pub use virt::Virtual;

pub use delayed::OptimizeDelayedCommands;
pub use delayed::{DelayedCommandQueue, DelayedCommandQueues, DelayedCommands};

pub(crate) use cache::TimeCache;
pub(crate) use delayed::queue_delayed_commands;
