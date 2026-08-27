//! How the world's [`TimeUpdateStrategy`] sources each frame's real time.

use core::time::Duration;

use zlim_os::time::Instant;

/// Configuration for how the engine advances [`Time<Real>`] each frame.
///
/// The default, [`Automatic`](Self::Automatic), samples the wall clock.
/// The manual variants are used for deterministic tests, networking, replay,
/// and headless simulation: they feed an explicit instant or duration instead
/// of the system clock.  [`None`](Self::None) freezes the world's clocks —
/// used by worlds whose time is supplied externally (e.g. the render world,
/// which receives the main world's clocks during extraction).
///
/// [`Time<Real>`]: crate::time::Time
///
/// # Examples
///
/// ```rust
/// # use std::time::Duration;
/// # use zlim_core::prelude::*;
/// # use zlim_core::time::TimeUpdateStrategy;
/// #
/// // A deterministic world:
/// // every `refresh_metadata` advances real time by 16ms.
/// let mut world = World::alloc();
///
/// let strategy = TimeUpdateStrategy::ManualDuration(Duration::from_millis(16));
/// world.set_time_strategy(strategy);
///
/// World::refresh_metadata(&mut world); // advances times
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimeUpdateStrategy {
    /// The world's clocks are not advanced automatically; time must be
    /// supplied externally (e.g. by extraction from the main world).
    None,
    /// Real time is taken from `Instant::now()` each frame.
    #[default]
    Automatic,
    /// Real time is advanced to the given [`Instant`] each frame.
    ///
    /// The instant must be manually updated each frame for time to progress.
    ManualInstant(Instant),
    /// Real time is incremented by the given [`Duration`] each frame.
    ManualDuration(Duration),
    /// Real time is incremented by `factor` fixed timesteps each frame, so a
    /// frame always runs exactly `factor` fixed steps.
    FixedTimesteps(u32),
}
