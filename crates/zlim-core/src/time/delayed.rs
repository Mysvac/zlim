//! Delayed commands: queue commands that are applied after a time delay.

use core::cmp::Reverse;
use core::fmt::{Debug, Formatter};
use core::time::Duration;

use zlim_core_derive::job_fn;
use zlim_reflect::derive::TypePath;
use zlim_utils::debug::DebugLocation;
use zlim_utils::hash::HashMap;

use crate::borrow::ResMut;
use crate::command::{CommandQueue, Commands};
use crate::derive::Resource;
use crate::world::World;

use super::Time;

// -----------------------------------------------------------------------------
// DelayedCommandQueue

/// A delayed command queue that should be submitted at `submit_at`.
#[derive(TypePath, Debug)]
pub struct DelayedCommandQueue {
    /// Absolute time (in `Time::elapsed`) at which the queue is due.
    pub submit_at: Duration,
    /// The command queue to apply once due.
    pub queue: CommandQueue, // reduce type size
}

// -----------------------------------------------------------------------------
// DelayedCommands

/// A wrapper over [`Commands`] that stores delayed [`CommandQueue`] values.
///
/// Queues are deduplicated by delay duration.  On drop, each queue is
/// converted into a [`DelayedCommandQueue`] and stored in [`DelayedCommandQueues`].
pub struct DelayedCommands<'w, 's> {
    queues: HashMap<Duration, CommandQueue>,
    commands: Commands<'w, 's>,
}

impl<'w, 's> DelayedCommands<'w, 's> {
    /// Returns a [`Commands`] writer whose queued commands will be delayed by `duration`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use zlim_core::command::Commands;
    ///
    /// fn queue(mut commands: Commands) {
    ///     let mut delayed = commands.delayed();
    ///     delayed.duration(Duration::from_millis(500)).spawn_empty(None);
    /// }
    /// ```
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    #[must_use = "The returned Commands must be used to submit commands with this delay."]
    pub fn duration(&mut self, duration: Duration) -> Commands<'w, '_> {
        let caller = DebugLocation::caller();
        let default = CommandQueue::with_caller(caller); // const function
        let queue = self.queues.entry(duration).or_insert(default);
        self.commands.rebound_to(queue)
    }

    /// Returns a [`Commands`] writer whose queued commands will be delayed by `secs` seconds.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::command::Commands;
    ///
    /// fn queue(mut commands: Commands) {
    ///     commands.delayed().secs(1.0).spawn_empty(None);
    /// }
    /// ```
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    #[must_use = "The returned Commands must be used to submit commands with this delay."]
    pub fn secs(&mut self, secs: f32) -> Commands<'w, '_> {
        let caller = DebugLocation::caller();
        let default = CommandQueue::with_caller(caller); // const function
        let duration = Duration::from_secs_f32(secs);
        let queue = self.queues.entry(duration).or_insert(default);
        self.commands.rebound_to(queue)
    }
}

impl DelayedCommands<'_, '_> {
    /// Submits all commands currently buffered in the local queue.
    ///
    /// This is typically called automatically when the buffer is dropped,
    /// so manual invocation is usually unnecessary.
    pub fn submit(&mut self) {
        self.submit_inner();
    }

    fn submit_inner(&mut self) {
        if self.queues.is_empty() {
            return;
        }

        let queues: Vec<DelayedCommandQueue> = self
            .queues
            .drain()
            .map(|(submit_at, queue)| DelayedCommandQueue { submit_at, queue })
            .collect();

        self.commands.queue(move |world: &mut World| {
            let elapsed: Duration = world.resource_mut_or_init::<Time>().elapsed();

            let delayed_queues = world
                .resource_mut_or_init::<DelayedCommandQueues>()
                .into_inner();

            delayed_queues.sorted = false;
            delayed_queues.queues.reserve(queues.len());

            for mut delayed in queues {
                delayed.submit_at += elapsed;
                delayed_queues.queues.push(delayed);
            }
        });
    }
}

impl Drop for DelayedCommands<'_, '_> {
    fn drop(&mut self) {
        self.submit_inner();
    }
}

impl Debug for DelayedCommands<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.pad("DelayedCommands { .. }")
    }
}

impl<'w, 's> Commands<'w, 's> {
    /// Returns a helper that can queue commands for delayed execution.
    ///
    /// The returned [`DelayedCommands`] wraps this [`Commands`] writer; call
    /// [`DelayedCommands::secs`] or [`DelayedCommands::duration`] to obtain a
    /// writer whose commands are deferred.  Queues are submitted when the
    /// helper (and the temporary writers) are dropped.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use std::time::Duration;
    /// # use zlim_core::command::Commands;
    /// # use zlim_core::time::TimeUpdateStrategy;
    /// # use zlim_core::world::World;
    /// #
    /// fn queue(mut commands: Commands) {
    ///     // Spawn an entity one second from now.
    ///     commands.delayed().secs(1.0).spawn_empty(None);
    /// }
    ///
    /// let mut world = World::alloc();
    ///
    /// let strategy = TimeUpdateStrategy::ManualDuration(Duration::from_millis(250));
    /// world.set_time_strategy(strategy);
    ///
    /// world.invoke_once(queue, ()).unwrap();
    /// assert_eq!(world.entity_count(), 0); // nothing spawned yet
    ///
    /// // Advance the clocks past the delay; `refresh_metadata` drives the
    /// // delayed queues each frame.  (Steps stay within the virtual clock's
    /// // default 250ms max-delta.)
    /// World::refresh_metadata(&mut world); // baseline
    /// World::refresh_metadata(&mut world); // 250ms
    /// World::refresh_metadata(&mut world); // 500ms
    /// World::refresh_metadata(&mut world); // 750ms
    /// World::refresh_metadata(&mut world); // 1000ms — the 1s delay is due
    ///
    /// // `refresh_metadata` only appends the due queue to the world's
    /// // command queue; flush it to actually spawn the entity.
    /// world.flush();
    /// assert_eq!(world.entity_count(), 1);
    /// ```
    pub fn delayed(&mut self) -> DelayedCommands<'w, '_> {
        DelayedCommands {
            commands: self.reborrow(),
            queues: HashMap::default(),
        }
    }
}

// -----------------------------------------------------------------------------
// DelayedCommandQueues

/// Resource that stores delayed command queues.
///
/// [`World::refresh_metadata`] drives it every frame: due queues are
/// appended to the world's [`Commands`] — they are **not** executed there.
/// The caller (e.g. the frame loop) must flush the world's command queue
/// (see [`World::flush`]) at an appropriate time to actually apply them.
/// The resource is auto-initialized on the first delayed submission; manual
/// setups may insert it explicitly before using [`Commands::delayed`].
///
/// [`World::refresh_metadata`]: crate::world::World::refresh_metadata
/// [`World::flush`]: crate::world::World::flush
#[derive(TypePath, Resource)]
pub struct DelayedCommandQueues {
    queues: Vec<DelayedCommandQueue>,
    sorted: bool,
}

impl Default for DelayedCommandQueues {
    fn default() -> Self {
        Self {
            queues: Vec::new(),
            sorted: true,
        }
    }
}

impl Debug for DelayedCommandQueues {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("DelayedCommandQueues")
            .field(&self.queues.as_slice())
            .finish()
    }
}

impl Drop for DelayedCommandQueues {
    fn drop(&mut self) {
        for queue in self.queues.iter_mut() {
            queue.queue.silence_on_unapplied();
        }
    }
}

// -----------------------------------------------------------------------------
// DelayedCommandQueues

pub(crate) fn queue_delayed_commands(world: &mut World) {
    let cell = world.cell();
    let world = unsafe { cell.data_mut() };

    let Some(queues) = world.get_resource_mut::<DelayedCommandQueues>() else {
        return;
    };
    if queues.queues.is_empty() {
        return;
    }

    #[cfg(feature = "trace")]
    let _span = zlim_log::info_span!("apply delayed commands").entered();

    ::core::hint::cold_path();
    let queues = queues.into_inner();

    // Do not directly use the values recorded in the change detection.
    if !queues.sorted {
        queues.queues.sort_by_key(|x| Reverse(x.submit_at));
        queues.sorted = true;
    }

    let world = unsafe { cell.data_mut() };

    let elapsed = world.resource::<Time>().elapsed();

    let mut commands = world.commands();

    let queues: &mut Vec<DelayedCommandQueue> = &mut queues.queues;
    loop {
        let Some(mut last) = queues.pop() else {
            break;
        };

        if last.submit_at > elapsed {
            queues.push(last);
            break;
        }

        commands.append(&mut last.queue);
    }

    // NOTE: the appended commands are NOT executed here.  The caller is
    // expected to flush the world's command queue at an appropriate time
    // (e.g. `world.flush()`) to actually apply them.
    // world.flush();
}

/// Optimizes the delayed command queue order for faster processing.
///
/// This step is optional; however, sorting delayed commands can
/// greatly enhance the speed of future enqueue operations.
///
/// If you use `MainSchedulePlugin` (the default main world driver in `App::default`),
/// this job will run in the `Last` stage. See `zlim_app` crate for details.
///
/// Otherwise, you may need to add it manually.
#[job_fn(type = OptimizeDelayedCommands, name = "zlim_core::time::OptimizeDelayedCommands")]
fn optimize_delayed_commands(mut queues: ResMut<DelayedCommandQueues>) {
    if queues.sorted {
        return;
    }

    if queues.queues.is_empty() {
        queues.sorted = true;
        return;
    }

    let queues = queues.into_inner();
    queues.queues.sort_by_key(|x| Reverse(x.submit_at));
    queues.sorted = true;
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use core::cmp::Reverse;
    use core::time::Duration;

    #[test]
    fn sorted_time() {
        let mut times: Vec<Duration> = vec![
            Duration::from_secs(1),
            Duration::from_secs(2),
            Duration::from_secs(3),
            Duration::from_secs(4),
        ];

        times.sort_by_key(|x| *x);
        assert_eq!(times[0], Duration::from_secs(1));

        times.sort_by_key(|x| Reverse(*x));
        assert_eq!(times[0], Duration::from_secs(4));
        assert_eq!(times[3], Duration::from_secs(1));
    }
}
