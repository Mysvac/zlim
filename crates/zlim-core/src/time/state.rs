//! The [`TimeState`] and [`TimeSnapshot`] resources.

use core::time::Duration;

use zlim_reflect::derive::TypePath;

use crate::derive::Resource;

use super::Time;
use super::fixed::Fixed;

// -----------------------------------------------------------------------------
// TimeState

/// Engine-owned per-frame time state.
///
/// This resource is installed at [`World::alloc`] time:
///
/// - `prev` — the fixed clock at the previous step (the interpolation start
///   of [`TimeSnapshot`]);
/// - `frame` — monotonic frame index of the world;
/// - `accumulator` — the fixed-step remainder, accumulated from the virtual
///   clock's delta each frame and consumed one step at a time by
///   `step_fixed`.
///
/// [`World::alloc`]: crate::world::World::alloc
#[derive(TypePath, Resource, Debug, Copy, Clone, PartialEq)]
#[type_path = "zlim_core::time::TimeState"]
pub struct TimeState {
    /// The last settled fixed-step state (interpolation start).
    pub prev: Time<Fixed>,
    /// Monotonic frame index.
    pub frame: u64,
    /// Time left over after the last fixed step, in `[0, timestep)`.
    pub accumulator: Duration,
}

impl Default for TimeState {
    fn default() -> Self {
        Self {
            prev: Time::default(),
            frame: 0,
            accumulator: Duration::ZERO,
        }
    }
}

// -----------------------------------------------------------------------------
// TimeSnapshot

/// An immutable snapshot of the fixed-interpolation state at one frame
/// boundary.
///
/// The engine refreshes the snapshot on each [`World::step_fixed`] call;
/// pipelined render worlds receive it with the frame data they render.
/// Render systems interpolate between [`prev`](Self::prev) and
/// [`curr`](Self::curr) using [`alpha`](Self::alpha).
///
/// [`World::step_fixed`]: crate::world::World::step_fixed
#[derive(Default, TypePath, Resource, Debug, Copy, Clone, PartialEq)]
#[type_path = "zlim_core::time::TimeSnapshot"]
pub struct TimeSnapshot {
    /// The last settled fixed-step state (interpolation start).
    pub prev: Time<Fixed>,
    /// The current settled fixed-step state (interpolation end).
    pub curr: Time<Fixed>,
    /// Monotonic frame index of this snapshot.
    pub frame: u64,
    /// Interpolation factor in `[0, 1)`: how far `curr` is past `prev`.
    pub alpha: f32,
}

// -----------------------------------------------------------------------------
