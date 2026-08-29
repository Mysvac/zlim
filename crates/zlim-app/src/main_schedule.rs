use zlim_core::borrow::ResMut;
use zlim_core::derive::Resource;
use zlim_core::job_fn;
use zlim_core::schedule::InternedScheduleLabel;
use zlim_core::schedule::Schedule;
use zlim_core::schedule::ScheduleLabel;
use zlim_core::schedule::ScheduleStage;
use zlim_core::schedule::SingleThreadedExecutor;
use zlim_core::system::Local;
use zlim_core::world::World;
use zlim_reflect::TypePath;

use super::{App, Plugin};

// -----------------------------------------------------------------------------
// Main

/// The app's main schedule; runs every other schedule in order each frame.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Main;

// ---------------------------------------------------------------------
// Main Loop - Start Up

/// Runs once before [`Startup`].
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct PreStartup;

/// Runs once after the app is built; the main setup stage.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Startup;

/// Runs once after [`Startup`].
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct PostStartup;

// ---------------------------------------------------------------------
// Main Loop - Update

/// Runs before everything else, every frame.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct First;

/// Runs before the main [`Update`] logic.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct PreUpdate;

/// Steps [`FixedMain`] with the accumulated fixed time, between
/// [`PreUpdate`] and [`Update`].
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct RunFixedMainLoop;

/// The main per-frame logic stage.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Update;

/// Spawns scene entities after the main [`Update`] logic.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct SpawnScene;

/// Runs after the main [`Update`] logic.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct PostUpdate;

/// Runs last each frame; final cleanup and deferred work.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Last;

// ---------------------------------------------------------------------
// RunFixedMainLoop

/// The fixed-timestep schedule, stepped once per accumulated timestep by
/// [`RunFixedMainLoop`].
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct FixedMain;

/// Runs first inside [`FixedMain`].
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct FixedFirst;

/// Runs before [`FixedUpdate`].
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct FixedPreUpdate;

/// The main fixed-timestep logic stage (physics, networking, ...).
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct FixedUpdate;

/// Runs after [`FixedUpdate`].
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct FixedPostUpdate;

/// Runs last inside [`FixedMain`].
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct FixedLast;

// ---------------------------------------------------------------------
// Stages

/// Stages of the [`RunFixedMainLoop`] schedule, ordered so the fixed loop
/// runs strictly between the surrounding frame logic.
#[derive(TypePath, ScheduleStage, Debug, Hash, PartialEq, Eq, Clone)]
pub enum FixedMainLoopStage {
    BeforeFixedMainLoop,
    FixedMainLoop,
    AfterFixedMainLoop,
}

// ---------------------------------------------------------------------
// ScheduleOrder

/// The ordered list of schedules driven by [`RunMainJob`]: per-frame
/// `labels` plus the one-shot `startup_labels`.
#[derive(TypePath, Resource, Debug)]
pub struct MainScheduleOrder {
    pub labels: Vec<InternedScheduleLabel>,
    pub startup_labels: Vec<InternedScheduleLabel>,
}

/// The ordered list of fixed schedules driven by [`RunFixedMainJob`] each
/// fixed step.
#[derive(TypePath, Resource, Debug)]
pub struct FixedMainScheduleOrder {
    pub labels: Vec<InternedScheduleLabel>,
}

impl Default for MainScheduleOrder {
    fn default() -> Self {
        Self {
            labels: vec![
                First.intern(),
                PreUpdate.intern(),
                RunFixedMainLoop.intern(),
                Update.intern(),
                SpawnScene.intern(),
                PostUpdate.intern(),
                Last.intern(),
            ],
            startup_labels: vec![PreStartup.intern(), Startup.intern(), PostStartup.intern()],
        }
    }
}

impl Default for FixedMainScheduleOrder {
    fn default() -> Self {
        Self {
            labels: vec![
                FixedFirst.intern(),
                FixedPreUpdate.intern(),
                FixedUpdate.intern(),
                FixedPostUpdate.intern(),
                FixedLast.intern(),
            ],
        }
    }
}

impl MainScheduleOrder {
    /// Adds the given `schedule` after the `after` schedule in the main list of schedules.
    pub fn insert_after(&mut self, after: impl ScheduleLabel, schedule: impl ScheduleLabel) {
        let index = self
            .labels
            .iter()
            .position(|current| (**current).eq(&after))
            .unwrap_or_else(|| panic!("Expected {after:?} to exist"));
        self.labels.insert(index + 1, schedule.intern());
    }

    /// Adds the given `schedule` before the `before` schedule in the main list of schedules.
    pub fn insert_before(&mut self, before: impl ScheduleLabel, schedule: impl ScheduleLabel) {
        let index = self
            .labels
            .iter()
            .position(|current| (**current).eq(&before))
            .unwrap_or_else(|| panic!("Expected {before:?} to exist"));
        self.labels.insert(index, schedule.intern());
    }

    /// Adds the given `schedule` after the `after` schedule in the list of startup schedules.
    pub fn insert_startup_after(
        &mut self,
        after: impl ScheduleLabel,
        schedule: impl ScheduleLabel,
    ) {
        let index = self
            .startup_labels
            .iter()
            .position(|current| (**current).eq(&after))
            .unwrap_or_else(|| panic!("Expected {after:?} to exist"));
        self.startup_labels.insert(index + 1, schedule.intern());
    }

    /// Adds the given `schedule` before the `before` schedule in the list of startup schedules.
    pub fn insert_startup_before(
        &mut self,
        before: impl ScheduleLabel,
        schedule: impl ScheduleLabel,
    ) {
        let index = self
            .startup_labels
            .iter()
            .position(|current| (**current).eq(&before))
            .unwrap_or_else(|| panic!("Expected {before:?} to exist"));
        self.startup_labels.insert(index, schedule.intern());
    }
}

impl FixedMainScheduleOrder {
    /// Adds the given `schedule` after the `after` schedule
    pub fn insert_after(&mut self, after: impl ScheduleLabel, schedule: impl ScheduleLabel) {
        let index = self
            .labels
            .iter()
            .position(|current| (**current).eq(&after))
            .unwrap_or_else(|| panic!("Expected {after:?} to exist"));
        self.labels.insert(index + 1, schedule.intern());
    }

    /// Adds the given `schedule` before the `before` schedule
    pub fn insert_before(&mut self, before: impl ScheduleLabel, schedule: impl ScheduleLabel) {
        let index = self
            .labels
            .iter()
            .position(|current| (**current).eq(&before))
            .unwrap_or_else(|| panic!("Expected {before:?} to exist"));
        self.labels.insert(index, schedule.intern());
    }
}

// ---------------------------------------------------------------------
// run

/// A job in the [`Main`] schedule that executes inner schedules
/// according to [`MainScheduleOrder`].
///
/// The `Startup` schedule is only run when this job is triggered
/// for the first time.
#[job_fn(type = RunMainJob, name = "zlim_app::jobs::RunMainJob")]
fn run_main(world: &mut World, mut non_startup: Local<bool>) {
    #[cold]
    #[inline(never)]
    fn run_startup(world: &mut World) {
        world.resource_scope(|world, order: ResMut<MainScheduleOrder>| {
            for &label in &order.startup_labels {
                // The startup schedule is one-shot; it is executed once and can be discarded.
                if let Some(mut schedule) = world.remove_schedule(label) {
                    schedule.run(world);
                }
            }
        });
        zlim_task::cfg::multi_thread! {
            World::update_schedules(world);
        }
    }

    if !*non_startup {
        run_startup(world);
        *non_startup = true;
    }

    world.resource_scope(|world, order: ResMut<MainScheduleOrder>| {
        for &label in &order.labels {
            world.try_run_schedule(label);
        }
    });
}

/// A job in the [`FixedMain`] schedule that executes inner schedules
/// according to [`FixedMainScheduleOrder`].
#[job_fn(type = RunFixedMainJob, name = "zlim_app::jobs::RunFixedMainJob")]
fn run_fixed_main(world: &mut World) {
    world.resource_scope(|world, order: ResMut<FixedMainScheduleOrder>| {
        for &label in &order.labels {
            world.try_run_schedule(label);
        }
    });
}

/// A job in the [`RunFixedMainLoop`] schedule that runs [`FixedMain`]
/// multiple times (or not at all) based on time information.
///
/// A maximum iteration limit is set to prevent starving the main loop.
#[job_fn(type = RunFixedMainLoopJob, name = "zlim_app::jobs::RunFixedMainLoopJob")]
fn run_fixed_main_loop(world: &mut World) {
    const MAX_LOOP: usize = 50;

    let mut count: usize = 0;

    while count < MAX_LOOP && World::step_fixed(world) {
        count += 1; // ↓ temporarily switch to fixed time
        world.apply_time(&world.fixed_time().unwrap().as_generic());
        world.try_schedule_scope(FixedMain, |world, schedule| schedule.run(world));
    }

    if count > 0 {
        world.apply_time(&world.virtual_time().unwrap().as_generic());
    }

    if count >= MAX_LOOP {
        ::core::hint::cold_path();
        zlim_log::warn!(
            "FixedMain loop exceeded maximum iterations.\n\
            The fixed timestep may be too small, or FixedMain systems are too slow, \
            causing backlog accumulation. Consider increasing the fixed timestep \
            or optimizing FixedMain systems.",
        );
    }
}

// ---------------------------------------------------------------------
// MainSchedulePlugin

/// Initializes the app's main schedules and their housekeeping jobs.
///
/// Installed by [`App::new`] / [`App::default`].  It creates three
/// single-threaded schedules and their driving jobs:
///
/// - **`Main`** — runs [`RunMainJob`], which executes the startup schedules
///   (`PreStartup` → `Startup` → `PostStartup`) once on the first frame and
///   then the per-frame schedules (`First` → `PreUpdate` → `RunFixedMainLoop`
///   → `Update` → `SpawnScene` → `PostUpdate` → `Last`) in the order of the
///   [`MainScheduleOrder`] resource.
///
/// - **`RunFixedMainLoop`** — runs [`RunFixedMainLoopJob`], which consumes
///   the accumulated fixed time with [`World::step_fixed`] and runs [`FixedMain`]
///   once per step (capped at 50 steps per frame so the main loop is never starved).
///
/// - **`FixedMain`** — runs [`RunFixedMainJob`], executing the fixed
///   schedules (`FixedFirst` → `FixedPreUpdate` → `FixedUpdate` →
///   `FixedPostUpdate` → `FixedLast`) per [`FixedMainScheduleOrder`].
///
/// It also wires engine housekeeping into the frame:
///
/// - `Last` runs [`OptimizeDelayedCommands`] to apply queued commands at
///   the end of each frame;
///
/// - `FixedPostUpdate` runs [`UpdateMessagesSignal`], so the double-buffered
///   message queues rotate exactly once per fixed step;
///
/// - message rotation is switched to signal-driven mode
///   ([`World::enable_update_messages_signal`]) because [`FixedMain`] steps
///   multiple times per frame.
///
/// Empty schedules (e.g. `Update` with no jobs) are skipped entirely; jobs
/// are added to them later by other plugins.
///
/// [`App::new`]: crate::App::new
/// [`OptimizeDelayedCommands`]: zlim_core::time::OptimizeDelayedCommands
/// [`UpdateMessagesSignal`]: zlim_core::message::UpdateMessagesSignal
#[derive(Debug, Default)]
pub struct MainSchedulePlugin;

impl Plugin for MainSchedulePlugin {
    fn apply(&self, app: &mut App) {
        let executor = Box::new(SingleThreadedExecutor::new());
        let main_schedule = Schedule::with_executor(Main, executor);

        let executor = Box::new(SingleThreadedExecutor::new());
        let fixed_main_schedule = Schedule::with_executor(FixedMain, executor);

        let executor = Box::new(SingleThreadedExecutor::new());
        let run_fixed_main_schedule = Schedule::with_executor(RunFixedMainLoop, executor);

        let world = app.main_world_mut();

        // With FixedMain present, message update must be manually triggered.
        // We send signals in the `FixedPostUpdate` schedule. See the end of this function.
        World::enable_update_messages_signal(world);

        world.insert_schedule(main_schedule);
        world.insert_schedule(fixed_main_schedule);
        world.insert_schedule(run_fixed_main_schedule);

        world.init_resource::<MainScheduleOrder>();
        world.init_resource::<FixedMainScheduleOrder>();

        // Not initializing, thus skipping empty schedules.
        // let _ = world.schedule_entry(PreStartup);
        // let _ = world.schedule_entry(Startup);
        // let _ = world.schedule_entry(PostStartup);

        // let _ = world.schedule_entry(First);
        // let _ = world.schedule_entry(PreUpdate);
        // let _ = world.schedule_entry(RunFixedMainLoop);
        // let _ = world.schedule_entry(Update);
        // let _ = world.schedule_entry(SpawnScene);
        // let _ = world.schedule_entry(PostStartup);
        // let _ = world.schedule_entry(Last);

        // let _ = world.schedule_entry(FixedFirst);
        // let _ = world.schedule_entry(FixedPreUpdate);
        // let _ = world.schedule_entry(FixedUpdate);
        // let _ = world.schedule_entry(FixedPostUpdate);
        // let _ = world.schedule_entry(FixedLast);

        let main = world.schedule_entry(Main);
        main.insert::<RunMainJob>(());

        let fixed_main = world.schedule_entry(FixedMain);
        fixed_main.insert::<RunFixedMainJob>(());

        let fixed_main_loop = world.schedule_entry(RunFixedMainLoop);
        fixed_main_loop.insert::<RunFixedMainLoopJob>(());

        let last = world.schedule_entry(Last);
        last.insert::<zlim_core::time::OptimizeDelayedCommands>(());

        let run_fixed_main = world.schedule_entry(RunFixedMainLoop);
        run_fixed_main.insert_stage(FixedMainLoopStage::BeforeFixedMainLoop);
        run_fixed_main.insert_stage(FixedMainLoopStage::FixedMainLoop);
        run_fixed_main.insert_stage(FixedMainLoopStage::AfterFixedMainLoop);
        run_fixed_main.insert_order(&[
            // should we use weak_order instead ?
            // FixedMainLoopStage::BeforeFixedMainLoop.stage_begin(), // optional
            FixedMainLoopStage::BeforeFixedMainLoop.stage_end(),
            FixedMainLoopStage::FixedMainLoop.stage_begin(),
            FixedMainLoopStage::FixedMainLoop.stage_end(),
            FixedMainLoopStage::AfterFixedMainLoop.stage_begin(),
            // FixedMainLoopStage::AfterFixedMainLoop.stage_end(),    // optional
        ]);

        let fixed_post = world.schedule_entry(FixedPostUpdate);
        fixed_post.insert::<zlim_core::message::UpdateMessagesSignal>(());
    }
}

// ---------------------------------------------------------------------
