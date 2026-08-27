//! Integration tests for `Schedule` job/group insertion and removal.

// Implementing `System` requires an `unsafe fn run_raw`, which is a
// deliberate part of the trait contract and fine to use in this test.

use core::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use zlim_core::derive::ScheduleStage as DeriveScheduleStage;
use zlim_core::job::job;
use zlim_core::job::job_fn;
use zlim_core::job::job_group;
use zlim_core::job::{JobDB, JobGroup, JobId};
use zlim_core::schedule::{AnonymousSchedule, ExecutorKind};
use zlim_core::schedule::{MultiThreadedExecutor, SingleThreadedExecutor};
use zlim_core::schedule::{Schedule, ScheduleStage};
use zlim_core::system::{AccessTable, System, SystemError, SystemFlags, SystemId};
use zlim_core::tick::Tick;
use zlim_core::world::{DeferredWorld, World, WorldCell};
use zlim_reflect::derive::TypePath;

// -----------------------------------------------------------------------------
// Job & group definitions

#[job_fn(type = JobA, name = "test_job_a")]
fn job_a() {}

#[job_fn(type = JobB, name = "test_job_b")]
fn job_b() {}

#[job_fn(type = JobC, name = "test_job_c")]
fn job_c() {}

#[job_fn(type = GenericJob<T: Default>, name = "test_generic_job")]
fn generic_job<T: Default>() {}

job_group! {
    type: TestGroup,
    name: "test_group",
    jobs: [JobA, JobB],
    order: [[JobA, JobB]],
}

// -----------------------------------------------------------------------------
// Stage definitions

#[derive(TypePath, DeriveScheduleStage, Clone, Copy)]
enum GameStage {
    Update,
    Render,
}

// Plain marker jobs used by the stage insertion tests; the order-recording
// jobs live inside the test that observes execution order, so parallel tests
// never share mutable state.
#[job_fn(type = StageJobA, name = "stage_job_a")]
fn stage_job_a() {}

#[job_fn(type = StageJobB, name = "stage_job_b")]
fn stage_job_b() {}

// -----------------------------------------------------------------------------
// Standalone job insertion

#[test]
fn insert_and_remove_by_name() {
    JobDB::collect();

    let mut schedule = Schedule::new(AnonymousSchedule);

    assert!(schedule.insert_by_name("test_job_a", ()));
    assert!(!schedule.insert_by_name("test_job_a", ()));

    let id = JobId::new("test_job_a", "#anonymous");
    assert!(schedule.contains_job(id));
    assert_eq!(schedule.jobs().len(), 1);

    assert!(schedule.remove_by_name("test_job_a"));
    assert!(!schedule.remove_by_name("test_job_a"));
    assert!(!schedule.contains_job(id));
    assert_eq!(schedule.jobs().len(), 0);
}

#[test]
fn insert_label_falls_back_to_constructing_itself() {
    JobDB::collect();

    let mut schedule = Schedule::new(AnonymousSchedule);

    // `GenericJob` is not auto-registered, so `insert_by_name` fails and the
    // label constructs its own database.
    assert!(schedule.insert::<GenericJob<u32>>(()));

    let id = JobId::new("test_generic_job<u32>", "#anonymous");
    assert!(schedule.contains_job(id));
}

#[test]
fn insert_missing_name_fails() {
    JobDB::collect();

    let mut schedule = Schedule::new(AnonymousSchedule);

    assert!(!schedule.insert_by_name("not_registered_job", ()));
    assert_eq!(schedule.jobs().len(), 0);
}

// -----------------------------------------------------------------------------
// Group insertion

#[test]
fn insert_and_remove_group() {
    JobDB::collect();
    JobGroup::collect();

    let mut schedule = Schedule::new(AnonymousSchedule);

    assert!(schedule.insert_group_by_name("test_group", ()));
    assert!(!schedule.insert_group_by_name("test_group", ()));

    assert!(schedule.contains_group("test_group"));
    assert!(schedule.groups().any(|name| name == "test_group"));

    // The group inserts its jobs (plus the begin/end markers) with the
    // group name.
    assert!(schedule.contains_job(JobId::new("test_job_a", "test_group")));
    assert!(schedule.contains_job(JobId::new("test_job_b", "test_group")));
    assert!(schedule.contains_job(JobId::new("zlim_core::GroupBegin", "test_group")));
    assert!(schedule.contains_job(JobId::new("zlim_core::GroupEnd", "test_group")));

    assert!(schedule.remove_group_by_name("test_group"));
    assert!(!schedule.remove_group_by_name("test_group"));

    assert!(!schedule.contains_group("test_group"));
    assert!(!schedule.contains_job(JobId::new("test_job_a", "test_group")));
    assert!(!schedule.contains_job(JobId::new("test_job_b", "test_group")));
    assert!(!schedule.contains_job(JobId::new("zlim_core::GroupBegin", "test_group")));
}

#[test]
fn insert_group_label() {
    JobDB::collect();
    JobGroup::collect();

    let mut schedule = Schedule::new(AnonymousSchedule);

    assert!(schedule.insert_group::<TestGroup>(()));
    assert!(!schedule.insert_group::<TestGroup>(()));
    assert!(schedule.contains_group("test_group"));

    assert!(schedule.remove_group::<TestGroup>());
    assert!(!schedule.contains_group("test_group"));
}

#[test]
fn remove_missing_group_fails() {
    JobDB::collect();
    JobGroup::collect();

    let mut schedule = Schedule::new(AnonymousSchedule);

    assert!(!schedule.remove_group_by_name("not_registered_group"));
}

// -----------------------------------------------------------------------------
// Execution

#[test]
fn schedule_runs_jobs() {
    JobDB::collect();

    let mut schedule = Schedule::new(AnonymousSchedule);
    assert!(schedule.insert_by_name("test_job_a", ()));
    assert!(schedule.insert_by_name("test_job_b", ()));

    let mut world = World::alloc();
    schedule.run(&mut world);
}

#[test]
fn schedule_runs_group() {
    JobDB::collect();
    JobGroup::collect();

    let mut schedule = Schedule::new(AnonymousSchedule);
    assert!(schedule.insert_group_by_name("test_group", ()));

    let mut world = World::alloc();
    schedule.run(&mut world);
}

// -----------------------------------------------------------------------------
// Rebuild behavior

/// Counts how often it is initialized / access-registered.
struct CountingJob {
    inits: &'static AtomicU32,
    registers: &'static AtomicU32,
}

impl System for CountingJob {
    type Input = ();
    type Output = ();

    fn id(&self) -> SystemId {
        SystemId::of::<Self>()
    }

    fn flags(&self) -> SystemFlags {
        SystemFlags::empty()
    }

    fn last_run(&self) -> Tick {
        Tick::new(0)
    }

    fn clamp_ticks(&mut self, _: Tick) {}

    fn set_last_run(&mut self, _: Tick) {}

    fn initialize(&mut self, _: &World) {
        self.inits.fetch_add(1, Ordering::Relaxed);
    }

    fn register_access(&self, _: &mut AccessTable, _: bool) {
        self.registers.fetch_add(1, Ordering::Relaxed);
    }

    #[expect(unsafe_code, reason = "required by the `System` trait contract")]
    unsafe fn run_raw(&mut self, _: (), _: WorldCell<'_>) -> Result<Self::Output, SystemError> {
        Ok(())
    }

    fn queue_deferred(&mut self, _: DeferredWorld) {}

    fn apply_deferred(&mut self, _: &mut World) {}
}

#[test]
fn rebuild_recycles_jobs_without_reinitializing() {
    static INITS: AtomicU32 = AtomicU32::new(0);
    static REGISTERS: AtomicU32 = AtomicU32::new(0);
    INITS.store(0, Ordering::Relaxed);
    REGISTERS.store(0, Ordering::Relaxed);

    JobDB::collect();

    job! {
        type: CountingJobLabel,
        name: "test_counting_job",
        system: CountingJob { inits: &INITS, registers: &REGISTERS },
    }

    let mut schedule = Schedule::new(AnonymousSchedule);
    assert!(schedule.insert::<CountingJobLabel>(()));

    let mut world = World::alloc();
    schedule.run(&mut world);

    assert_eq!(INITS.load(Ordering::Relaxed), 1);
    assert_eq!(REGISTERS.load(Ordering::Relaxed), 1);

    // Inserting another job forces a rebuild; the recycled job must keep
    // its access table and skip re-initialization.
    assert!(schedule.insert_by_name("test_job_a", ()));
    schedule.run(&mut world);

    assert_eq!(INITS.load(Ordering::Relaxed), 1);
    assert_eq!(REGISTERS.load(Ordering::Relaxed), 1);
}

// -----------------------------------------------------------------------------
// Executor selection

#[test]
fn executor_kind_controls_apply_deferred() {
    JobDB::collect();
    JobGroup::collect();

    // Single-threaded executor does not insert apply-deferred helpers.
    let mut single =
        Schedule::with_executor(AnonymousSchedule, Box::new(SingleThreadedExecutor::new()));
    assert!(single.insert_group_by_name("test_group", ()));
    let single_job_count = single.jobs().len();

    // Multi-threaded executor may insert apply-deferred helpers for
    // deferred jobs; our test jobs are not deferred, so the count matches.
    let mut multi =
        Schedule::with_executor(AnonymousSchedule, Box::new(MultiThreadedExecutor::new()));
    assert_eq!(multi.executor_kind(), ExecutorKind::MultiThreaded);
    assert!(multi.insert_group_by_name("test_group", ()));
    assert_eq!(multi.jobs().len(), single_job_count);
}

// -----------------------------------------------------------------------------
// Node generation — removal must survive a rebuild

#[test]
fn removed_job_stays_removed_after_rebuild() {
    JobDB::collect();

    let mut schedule = Schedule::new(AnonymousSchedule);
    assert!(schedule.insert_by_name("test_job_a", ()));
    assert!(schedule.insert_by_name("test_job_b", ()));

    let mut world = World::alloc();
    schedule.run(&mut world); // compile both jobs

    // Remove `a` while its job object still lives in the compiled schedule.
    assert!(schedule.remove_by_name("test_job_a"));

    // The rebuild recycles the old schedule; the removed job must not be
    // resurrected.
    schedule.run(&mut world);
    assert_eq!(schedule.jobs().len(), 1);
    assert!(!schedule.contains_job(JobId::new("test_job_a", "#anonymous")));
    assert!(schedule.contains_job(JobId::new("test_job_b", "#anonymous")));
}

#[test]
fn removed_job_slot_reuse_does_not_corrupt() {
    JobDB::collect();

    let mut schedule = Schedule::new(AnonymousSchedule);
    assert!(schedule.insert_by_name("test_job_a", ()));
    assert!(schedule.insert_by_name("test_job_b", ()));

    let mut world = World::alloc();
    schedule.run(&mut world);

    // Remove `a`, then insert `c`, which reuses `a`'s freed slot with a
    // bumped generation tag.
    assert!(schedule.remove_by_name("test_job_a"));
    assert!(schedule.insert_by_name("test_job_c", ()));

    // The rebuild must not overwrite `c`'s buffer slot with the recycled
    // (stale) `a` job.
    schedule.run(&mut world);

    assert_eq!(schedule.jobs().len(), 2);
    assert!(!schedule.contains_job(JobId::new("test_job_a", "#anonymous")));
    assert!(schedule.contains_job(JobId::new("test_job_b", "#anonymous")));
    assert!(schedule.contains_job(JobId::new("test_job_c", "#anonymous")));
}

// -----------------------------------------------------------------------------
// ScheduleStage

#[test]
fn insert_into_named_stage_creates_markers() {
    JobDB::collect();

    let mut schedule = Schedule::new(AnonymousSchedule);
    let stage = GameStage::Update;

    assert!(schedule.insert::<StageJobA>(stage));

    // The stage is auto-created with begin/end marker jobs, and the job
    // keeps its own (anonymous) group value.
    assert!(schedule.contains_job(stage.stage_begin()));
    assert!(schedule.contains_job(stage.stage_end()));
    assert!(schedule.contains_job(JobId::new("stage_job_a", "#anonymous")));
    assert_eq!(schedule.jobs().len(), 3);
}

#[test]
fn insert_into_anonymous_stage_creates_no_markers() {
    JobDB::collect();

    let mut schedule = Schedule::new(AnonymousSchedule);

    assert!(schedule.insert::<StageJobA>(()));

    // The anonymous stage produces no begin/end jobs and no stage entry.
    assert!(!schedule.contains_job(().stage_begin()));
    assert!(!schedule.contains_job(().stage_end()));
    assert_eq!(schedule.jobs().len(), 1);
}

#[test]
fn insert_group_into_stage_creates_markers() {
    JobDB::collect();
    JobGroup::collect();

    let mut schedule = Schedule::new(AnonymousSchedule);
    let stage = GameStage::Render;

    assert!(schedule.insert_group::<TestGroup>(stage));

    // The group keeps its own name; the stage markers exist.
    assert!(schedule.contains_group("test_group"));
    assert!(schedule.contains_job(stage.stage_begin()));
    assert!(schedule.contains_job(stage.stage_end()));
    assert!(schedule.contains_job(JobId::new("zlim_core::GroupBegin", "test_group")));
    assert!(schedule.contains_job(JobId::new("test_job_a", "test_group")));

    // Executes without cycle errors.
    let mut world = World::alloc();
    schedule.run(&mut world);
}

#[test]
fn stage_begin_runs_before_and_end_after_jobs() {
    static ORDER: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    ORDER.lock().unwrap().clear();

    JobDB::collect();

    job! {
        type: OrderBeforeBegin,
        system: || ORDER.lock().unwrap().push("before_begin"),
    }
    job! {
        type: OrderAfterEnd,
        system: || ORDER.lock().unwrap().push("after_end"),
    }
    job! {
        type: OrderJobA,
        system: || ORDER.lock().unwrap().push("a"),
    }
    job! {
        type: OrderJobB,
        system: || ORDER.lock().unwrap().push("b"),
    }

    let mut schedule =
        Schedule::with_executor(AnonymousSchedule, Box::new(SingleThreadedExecutor::new()));
    let stage = GameStage::Update;

    assert!(schedule.insert::<OrderJobA>(stage));
    assert!(schedule.insert::<OrderJobB>(stage));

    // Pin the stage markers between two observable jobs: the begin marker
    // must run before any stage job, the end marker after every stage job.
    assert!(schedule.insert::<OrderBeforeBegin>(()));
    assert!(schedule.insert::<OrderAfterEnd>(()));
    schedule.insert_order(&[
        JobId::isolated("schedule::OrderBeforeBegin"),
        stage.stage_begin(),
    ]);
    schedule.insert_order(&[
        stage.stage_end(),
        JobId::isolated("schedule::OrderAfterEnd"),
    ]);

    let mut world = World::alloc();
    schedule.run(&mut world);

    let order = ORDER.lock().unwrap();
    assert_eq!(order.first(), Some(&"before_begin"));
    assert_eq!(order.last(), Some(&"after_end"));
    assert!(order.contains(&"a"));
    assert!(order.contains(&"b"));
}

// -----------------------------------------------------------------------------
// run_if conditions
//
// Each test keeps its counter/order state in a function-local `static`,
// reset at the start, so parallel tests never share mutable state.

#[test]
fn run_if_conditions_gate_jobs() {
    static RAN: AtomicU32 = AtomicU32::new(0);
    RAN.store(0, Ordering::Relaxed);

    JobDB::collect();

    job! {
        type: LocalGatedJob,
        system: || {
            RAN.fetch_add(1, Ordering::Relaxed);
        },
        run_if: || false,
    }
    job! {
        type: LocalGatedJobTrue,
        system: || {
            RAN.fetch_add(1, Ordering::Relaxed);
        },
        run_if: || true,
    }

    let mut schedule = Schedule::new(AnonymousSchedule);
    assert!(schedule.insert::<LocalGatedJob>(()));
    assert!(schedule.insert::<LocalGatedJobTrue>(()));

    // Each job brings its run_if condition along as a separate node.
    assert_eq!(schedule.jobs().len(), 4);

    let mut world = World::alloc();
    schedule.run(&mut world);

    // `|| false` gates its job (skipped); `|| true` lets it run.
    assert_eq!(RAN.load(Ordering::Relaxed), 1);
}

#[test]
fn run_if_condition_runs_after_stage_begin() {
    static STAGE_ORDER: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    STAGE_ORDER.lock().unwrap().clear();

    JobDB::collect();

    job! {
        type: PinBegin,
        system: || STAGE_ORDER.lock().unwrap().push("before_begin"),
    }
    job! {
        type: PinEnd,
        system: || STAGE_ORDER.lock().unwrap().push("after_end"),
    }
    job! {
        type: LocalStageGatedJob,
        system: || STAGE_ORDER.lock().unwrap().push("gated"),
        run_if: || {
            STAGE_ORDER.lock().unwrap().push("cond");
            true
        },
    }

    let mut schedule =
        Schedule::with_executor(AnonymousSchedule, Box::new(SingleThreadedExecutor::new()));
    let stage = GameStage::Update;

    assert!(schedule.insert::<PinBegin>(()));
    assert!(schedule.insert::<LocalStageGatedJob>(stage));
    assert!(schedule.insert::<PinEnd>(()));
    schedule.insert_order(&[JobId::isolated("schedule::PinBegin"), stage.stage_begin()]);
    schedule.insert_order(&[stage.stage_end(), JobId::isolated("schedule::PinEnd")]);

    let mut world = World::alloc();
    schedule.run(&mut world);

    // begin marker (skipped) → condition → gated job → end marker (skipped).
    let order = STAGE_ORDER.lock().unwrap();
    assert_eq!(&*order, &["before_begin", "cond", "gated", "after_end"]);
}

#[test]
fn run_if_in_group_gates_job() {
    static GROUP_RAN: AtomicU32 = AtomicU32::new(0);
    GROUP_RAN.store(0, Ordering::Relaxed);

    JobDB::collect();
    JobGroup::collect();

    job! {
        type: LocalGroupGatedJob,
        system: || {
            GROUP_RAN.fetch_add(1, Ordering::Relaxed);
        },
        run_if: || false,
    }
    job_group! {
        type: LocalRunIfGroup,
        name: "local_run_if_group",
        jobs: [LocalGroupGatedJob, JobA],
    }

    let mut schedule = Schedule::new(AnonymousSchedule);
    assert!(schedule.insert_group::<LocalRunIfGroup>(()));

    let mut world = World::alloc();
    schedule.run(&mut world);

    // The group runs without cycles; the gated job (false condition) is
    // skipped while `JobA` still runs.
    assert_eq!(GROUP_RAN.load(Ordering::Relaxed), 0);
}
