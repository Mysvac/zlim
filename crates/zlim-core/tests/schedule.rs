//! Integration tests for `Schedule` job/group insertion and removal.

// Implementing `System` requires an `unsafe fn run_raw`, which is a
// deliberate part of the trait contract and fine to use in this test.
#![allow(unsafe_code, reason = "required by the `System` trait contract")]

use core::sync::atomic::{AtomicU32, Ordering};

use zlim_core::job::job;
use zlim_core::job::job_fn;
use zlim_core::job::job_group;
use zlim_core::job::{JobDB, JobGroup, JobId};
use zlim_core::schedule::{
    AnonymousSchedule, ExecutorKind, MultiThreadedExecutor, Schedule, SingleThreadedExecutor,
};
use zlim_core::system::{AccessTable, System, SystemError, SystemFlags, SystemId};
use zlim_core::tick::Tick;
use zlim_core::world::{DeferredWorld, World, WorldCell};

// -----------------------------------------------------------------------------
// Job & group definitions

#[job_fn(type = JobA, name = "test_job_a")]
fn job_a() {}

#[job_fn(type = JobB, name = "test_job_b")]
fn job_b() {}

#[job_fn(type = GenericJob<T: Default>, name = "test_generic_job")]
fn generic_job<T: Default>() {}

job_group! {
    type: TestGroup,
    name: "test_group",
    jobs: [JobA, JobB],
    order: [[JobA, JobB]],
}

// -----------------------------------------------------------------------------
// Standalone job insertion

#[test]
fn insert_and_remove_by_name() {
    JobDB::collect();

    let mut schedule = Schedule::new(AnonymousSchedule);

    assert!(schedule.insert_by_name("test_job_a"));
    assert!(!schedule.insert_by_name("test_job_a"));

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
    assert!(schedule.insert::<GenericJob<u32>>());

    let id = JobId::new("test_generic_job<u32>", "#anonymous");
    assert!(schedule.contains_job(id));
}

#[test]
fn insert_missing_name_fails() {
    JobDB::collect();

    let mut schedule = Schedule::new(AnonymousSchedule);

    assert!(!schedule.insert_by_name("not_registered_job"));
    assert_eq!(schedule.jobs().len(), 0);
}

// -----------------------------------------------------------------------------
// Group insertion

#[test]
fn insert_and_remove_group() {
    JobDB::collect();
    JobGroup::collect();

    let mut schedule = Schedule::new(AnonymousSchedule);

    assert!(schedule.insert_group_by_name("test_group"));
    assert!(!schedule.insert_group_by_name("test_group"));

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

    assert!(schedule.insert_group::<TestGroup>());
    assert!(!schedule.insert_group::<TestGroup>());
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
    assert!(schedule.insert_by_name("test_job_a"));
    assert!(schedule.insert_by_name("test_job_b"));

    let mut world = World::alloc();
    schedule.run(&mut world);
}

#[test]
fn schedule_runs_group() {
    JobDB::collect();
    JobGroup::collect();

    let mut schedule = Schedule::new(AnonymousSchedule);
    assert!(schedule.insert_group_by_name("test_group"));

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

    unsafe fn run_raw(&mut self, _: (), _: WorldCell<'_>) -> Result<Self::Output, SystemError> {
        Ok(())
    }

    fn queue_deferred(&mut self, _: DeferredWorld) {}

    fn apply_deferred(&mut self, _: &mut World) {}
}

static INITS: AtomicU32 = AtomicU32::new(0);
static REGISTERS: AtomicU32 = AtomicU32::new(0);

job! {
    type: CountingJobLabel,
    name: "test_counting_job",
    system: CountingJob { inits: &INITS, registers: &REGISTERS },
}

#[test]
fn rebuild_recycles_jobs_without_reinitializing() {
    JobDB::collect();

    INITS.store(0, Ordering::Relaxed);
    REGISTERS.store(0, Ordering::Relaxed);

    let mut schedule = Schedule::new(AnonymousSchedule);
    assert!(schedule.insert::<CountingJobLabel>());

    let mut world = World::alloc();
    schedule.run(&mut world);

    assert_eq!(INITS.load(Ordering::Relaxed), 1);
    assert_eq!(REGISTERS.load(Ordering::Relaxed), 1);

    // Inserting another job forces a rebuild; the recycled job must keep
    // its access table and skip re-initialization.
    assert!(schedule.insert_by_name("test_job_a"));
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
    assert!(single.insert_group_by_name("test_group"));
    let single_job_count = single.jobs().len();

    // Multi-threaded executor may insert apply-deferred helpers for
    // deferred jobs; our test jobs are not deferred, so the count matches.
    let mut multi =
        Schedule::with_executor(AnonymousSchedule, Box::new(MultiThreadedExecutor::new()));
    assert_eq!(multi.executor_kind(), ExecutorKind::MultiThreaded);
    assert!(multi.insert_group_by_name("test_group"));
    assert_eq!(multi.jobs().len(), single_job_count);
}
