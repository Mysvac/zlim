//! Integration tests for the `Schedules` collection and its `World`
//! integration.

use zlim_core::borrow::ResMut;
use zlim_core::derive::Resource;
use zlim_core::job::{JobDB, job_fn};
use zlim_core::schedule::{Schedule, ScheduleLabel, Schedules};
use zlim_core::world::World;
use zlim_reflect::TypePath;

// -----------------------------------------------------------------------------
// Labels & jobs

#[derive(ScheduleLabel, Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Update;

#[derive(ScheduleLabel, Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Render;

#[derive(TypePath, Resource, Debug, PartialEq)]
struct Counter(u32);

#[job_fn(type = IncCounter, name = "sched_inc_counter")]
fn inc_counter(mut counter: ResMut<Counter>) {
    counter.0 += 1;
}

// -----------------------------------------------------------------------------
// Insert & lookup

#[test]
fn insert_get_remove() {
    let mut schedules = Schedules::default();

    assert!(schedules.is_empty());
    assert_eq!(schedules.len(), 0);

    assert!(schedules.insert(Schedule::new(Update)).is_none());
    assert_eq!(schedules.len(), 1);
    assert!(!schedules.is_empty());
    assert!(schedules.contains(Update));
    assert_eq!(schedules.get(Update).unwrap().label(), Update.intern());

    // Inserting a schedule with the same label replaces the old one.
    let replaced = schedules.insert(Schedule::new(Update)).unwrap();
    assert_eq!(replaced.label(), Update.intern());
    assert_eq!(schedules.len(), 1);

    let removed = schedules.remove(Update).unwrap();
    assert_eq!(removed.label(), Update.intern());
    assert!(!schedules.contains(Update));
    assert!(schedules.remove(Update).is_none());
}

#[test]
fn get_mut_allows_in_place_building() {
    JobDB::collect();

    let mut schedules = Schedules::default();
    schedules.insert(Schedule::new(Update));

    let schedule = schedules.get_mut(Update).unwrap();
    assert!(schedule.insert_by_name("sched_inc_counter", ()));

    let schedule = schedules.get_mut(Update).unwrap();
    assert_eq!(schedule.jobs().len(), 1);
}

// -----------------------------------------------------------------------------
// Entry & add_schedule

#[test]
fn entry_and_add_schedule() {
    let mut schedules = Schedules::default();

    // `entry` creates the schedule on demand when the label is absent.
    assert!(!schedules.contains(Render));

    schedules.entry(Render);
    assert!(schedules.contains(Render));
    assert_eq!(schedules.len(), 1);

    let schedule = schedules.entry(Render);
    assert_eq!(schedule.label(), Render.intern());
    assert_eq!(schedules.len(), 1);

    // `add_schedule` replaces and returns a mutable reference.
    let schedule = schedules.add_schedule(Schedule::new(Update));
    assert_eq!(schedule.label(), Update.intern());
    assert_eq!(schedules.len(), 2);
}

// -----------------------------------------------------------------------------
// Deref to the underlying map

#[test]
fn derefs_to_map() {
    let mut schedules = Schedules::default();
    schedules.insert(Schedule::new(Update));
    schedules.insert(Schedule::new(Render));

    let interned = Update.intern();
    assert!(schedules.contains_key(&interned));
    assert_eq!(schedules.values().count(), 2);
    assert_eq!(schedules.keys().count(), 2);
    assert_eq!(schedules.iter().count(), 2);
    assert_eq!(schedules.iter_mut().count(), 2);

    schedules.clear();
    assert!(schedules.is_empty());
}

// -----------------------------------------------------------------------------
// Execution

#[test]
fn world_run_schedule_executes_jobs() {
    JobDB::collect();

    let mut schedule = Schedule::new(Update);
    assert!(schedule.insert_by_name("sched_inc_counter", ()));

    let mut world = World::alloc();
    world.insert_resource(Counter(0));
    world.schedules_mut().insert(schedule);

    world.run_schedule(Update);
    world.run_schedule(Update);

    assert_eq!(world.resource::<Counter>().0, 2);
}

// -----------------------------------------------------------------------------
// World integration

#[test]
fn world_owns_schedules() {
    JobDB::collect();

    let mut world = World::alloc();
    world.insert_resource(Counter(0));

    assert!(world.schedules().is_empty());

    {
        let mut schedule = Schedule::new(Update);
        assert!(schedule.insert_by_name("sched_inc_counter", ()));
        world.schedules_mut().insert(schedule);
    }

    assert_eq!(world.schedules().len(), 1);
    assert!(world.schedules().contains(Update));
    assert!(world.schedules().get(Update).is_some());

    world.run_schedule(Update);
    world.run_schedule(Update);

    assert_eq!(world.resource::<Counter>().0, 2);

    assert!(world.schedules_mut().remove(Update).is_some());
    assert!(world.schedules().is_empty());
}

#[test]
fn world_run_schedule_creates_missing_schedule() {
    let mut world = World::alloc();

    // `run_schedule` on an unknown label creates an empty schedule instead
    // of panicking.
    assert!(!world.schedules().contains(Update));
    world.run_schedule(Update);
    assert!(world.schedules().contains(Update));
}

// -----------------------------------------------------------------------------
