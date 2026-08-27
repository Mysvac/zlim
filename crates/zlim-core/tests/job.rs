//! Integration tests for the `job_fn` attribute macro and the
//! `job!` macro.

use zlim_core::system::{In, IntoSystem};
use zlim_core::world::World;
use zlim_core::{job, job_fn, job_group};

use job::{IntoJob, JobDB, JobGroup, JobGroupLabel, JobLabel};
use zlim_reflect::TypePath;

// -----------------------------------------------------------------------------
// Attribute macro — non-generic

#[job_fn(type = SimpleSystem, name = "attr_simple")]
fn simple_system() {}

#[test]
fn job_attr_non_generic_name() {
    assert_eq!(SimpleSystem::name(), "attr_simple");
}

#[test]
fn job_attr_non_generic_database() {
    let db = SimpleSystem::database();
    assert_eq!(db.name, "attr_simple");

    // The ctor wraps the function into a runnable job.
    let mut sys = (db.ctor)("group");
    assert_eq!(sys.id().name(), "attr_simple");
    assert_eq!(sys.id().group(), "group");

    let world = World::alloc();
    sys.initialize(&world);
}

#[test]
fn job_attr_non_generic_registered() {
    JobDB::collect();
    assert!(JobDB::get("attr_simple").is_some());
}

// -----------------------------------------------------------------------------
// Attribute macro — generic

#[job_fn(type = GenericSystem<T: Default>, name = "attr_generic")]
fn generic_system<T: Default>() {}

#[test]
fn job_attr_generic_name() {
    assert_eq!(GenericSystem::<u32>::name(), "attr_generic<u32>");
}

#[test]
fn job_attr_generic_database() {
    let db = GenericSystem::<u32>::database();
    assert_eq!(db.name, "attr_generic<u32>");

    let mut sys = (db.ctor)("group");
    let world = World::alloc();
    sys.initialize(&world);
}

#[test]
fn job_attr_generic_not_auto_registered() {
    JobDB::collect();
    assert!(JobDB::get("attr_generic<u32>").is_none());

    JobDB::register(GenericSystem::<u32>::database());
    assert!(JobDB::get("attr_generic<u32>").is_some());
}

// -----------------------------------------------------------------------------
// job! — non-generic

fn pipe1() -> u8 {
    1
}

fn pipe2(_input: In<u8>) {}

job! {
    type: PipeSystem,
    name: "pipe_simple",
    system: pipe1.pipe(pipe2),
}

#[test]
fn job_non_generic() {
    assert_eq!(PipeSystem::name(), "pipe_simple");

    let db = PipeSystem::database();
    assert_eq!(db.name, "pipe_simple");

    let mut sys = (db.ctor)("group");
    let world = World::alloc();
    sys.initialize(&world);

    JobDB::collect();
    assert!(JobDB::get("pipe_simple").is_some());
}

// -----------------------------------------------------------------------------
// job! — generic

fn gen1<T: Default>() -> T {
    T::default()
}

fn gen2<T>(_input: In<T>) {}

job! {
    type: GenericPipe<T: Default + 'static>,
    name: "pipe_generic",
    system: gen1::<T>.pipe(gen2::<T>),
}

#[test]
fn job_generic() {
    assert_eq!(GenericPipe::<u32>::name(), "pipe_generic<u32>");

    let db = GenericPipe::<u32>::database();
    assert_eq!(db.name, "pipe_generic<u32>");

    let mut sys = (db.ctor)("group");
    let world = World::alloc();
    sys.initialize(&world);
}

// -----------------------------------------------------------------------------
// name omitted — defaults to the marker's TypePath

#[job_fn(type = NoNameJob)]
fn no_name_job() {}

#[job_fn(type = NoNameGeneric<T: Default>)]
fn no_name_generic<T: Default>() {}

job! {
    type: NoNamePipeJob,
    system: pipe1.pipe(pipe2),
}

#[test]
fn job_name_defaults_to_type_path() {
    assert_eq!(NoNameJob::name(), NoNameJob::type_path());
    assert_eq!(
        NoNameGeneric::<u32>::name(),
        <NoNameGeneric::<u32>>::type_path()
    );
    assert_eq!(NoNamePipeJob::name(), NoNamePipeJob::type_path());
}

#[test]
fn job_name_default_database_and_registration() {
    let db = NoNameJob::database();
    assert_eq!(db.name, NoNameJob::name());

    JobDB::collect();
    assert!(JobDB::get(NoNameJob::name()).is_some());
}

// -----------------------------------------------------------------------------
// strict parameter

#[job_fn(type = StrictJob, name = "strict_job", strict = true)]
fn strict_job() {}

#[job_fn(type = LaxJob, name = "lax_job", strict = false)]
fn lax_job() {}

job! {
    type: StrictPipeJob,
    name: "strict_pipe",
    system: pipe1.pipe(pipe2),
    strict: true,
}

job! {
    type: LaxPipeJob,
    name: "lax_pipe",
    system: pipe1.pipe(pipe2),
    strict: false,
}

#[test]
fn job_strict_parameter() {
    assert_eq!(StrictJob::name(), "strict_job");
    assert_eq!(LaxJob::name(), "lax_job");
    assert_eq!(StrictPipeJob::name(), "strict_pipe");
    assert_eq!(LaxPipeJob::name(), "lax_pipe");

    let dbs = [
        StrictJob::database(),
        LaxJob::database(),
        StrictPipeJob::database(),
        LaxPipeJob::database(),
    ];

    for db in dbs {
        let mut sys = (db.ctor)("group");
        let world = World::alloc();
        sys.initialize(&world);
    }

    JobDB::collect();
    assert!(JobDB::get("strict_job").is_some());
    assert!(JobDB::get("lax_job").is_some());
    assert!(JobDB::get("strict_pipe").is_some());
    assert!(JobDB::get("lax_pipe").is_some());
}

#[test]
fn into_job_strict_const_generic() {
    // The `STRICT` const generic directly selects the wrapper.
    let mut strict = IntoJob::into_job::<true>(simple_system, "direct_strict", "group");
    let mut lax = IntoJob::into_job::<false>(simple_system, "direct_lax", "group");

    let world = World::alloc();
    strict.initialize(&world);
    lax.initialize(&world);

    assert_eq!(strict.id().name(), "direct_strict");
    assert_eq!(lax.id().name(), "direct_lax");
}

// -----------------------------------------------------------------------------
// run_if

fn condition_a() -> bool {
    true
}

fn condition_b() -> bool {
    false
}

#[job_fn(type = RunIfJob, name = "run_if_job", run_if = condition_a)]
fn run_if_job() {}

job! {
    type: RunIfPipeJob,
    name: "run_if_pipe",
    system: pipe1.pipe(pipe2),
    run_if: [condition_a, condition_b],
}

#[test]
fn job_run_if_single_and_list() {
    // A single `run_if` expression becomes a one-element slice; the
    // condition constructor takes the job's group name explicitly.
    let db = RunIfJob::database();
    assert_eq!(db.run_if.len(), 1);
    let cond = (db.run_if[0])("group");
    assert_eq!(cond.id().name(), "run_if_job#run_if<0>#condition_a");
    assert_eq!(cond.id().group(), "group");

    // A list keeps the order
    let db = RunIfPipeJob::database();
    assert_eq!(db.run_if.len(), 2);
    assert_eq!(
        (db.run_if[0])("group").id().name(),
        "run_if_pipe#run_if<0>#condition_a"
    );
    assert_eq!(
        (db.run_if[1])("group").id().name(),
        "run_if_pipe#run_if<1>#condition_b"
    );
}

#[test]
fn job_run_if_empty() {
    // Without `run_if` the slice is empty (and stays `'static`).
    let db = SimpleSystem::database();
    assert!(db.run_if.is_empty());
}

// -----------------------------------------------------------------------------
// job_group!

#[job_fn(type = GroupLabelA, name = "group_label_a")]
fn group_label_a() {}

job_group! {
    type: ExampleGroup,
    name: "example_group",
    jobs: [GroupLabelA, "label_b"],
    condition: GroupLabelA,
    order: [["label_b", GroupLabelA]],
    weak_order: [[GroupLabelA, GroupLabelA]],
    relaxed_order: [["label_b", GroupLabelA]],
}

#[test]
fn job_group_non_generic() {
    assert_eq!(ExampleGroup::name(), "example_group");

    let group = ExampleGroup::group();
    assert_eq!(group.name, "example_group");

    // `jobs` is prefixed with the group begin/end markers.
    assert_eq!(group.jobs.len(), 4);
    assert_eq!(group.jobs[0].name(), "zlim_core::GroupBegin");
    assert_eq!(group.jobs[1].name(), "zlim_core::GroupEnd");
    assert_eq!(group.jobs[2].name(), "group_label_a");
    assert_eq!(group.jobs[3].name(), "label_b");

    // `condition` indexes the group's `jobs` array (user list shifted by
    // +2 for the begin/end markers).
    assert_eq!(group.condition, Some(2));

    // order: the user chain `label_b -> group_label_a` shifted by +2,
    // plus `GroupBegin -> all`, the strict `GroupBegin -> GroupEnd` pair,
    // and the condition edge `group_label_a -> GroupBegin` (the condition
    // job must run before the group begins).
    assert_eq!(group.order, &[(0, 1), (0, 2), (0, 3), (2, 0), (3, 2)]);

    // weak_order: only the user chain shifted by +2.
    assert_eq!(group.weak_order, &[(2, 2)]);

    // relaxed_order: the user chain shifted by +2, plus `all -> GroupEnd`
    // (non-blocking hints).
    assert_eq!(group.relaxed_order, &[(2, 1), (3, 1), (3, 2)]);
}

#[test]
fn job_group_registers() {
    ExampleGroup::register();
    assert!(JobGroup::get("example_group").is_some());
}

#[test]
fn job_group_auto_registered() {
    JobGroup::collect();
    assert!(JobGroup::get("example_group").is_some());
}

job_group! {
    type: GenericGroup<T: Default>,
    name: "generic_group",
    jobs: [GroupLabelA],
}

#[test]
fn job_group_generic() {
    assert_eq!(GenericGroup::<u32>::name(), "generic_group<u32>");

    let group = GenericGroup::<u32>::group();
    assert_eq!(group.jobs.len(), 3);
    assert_eq!(group.jobs[0].name(), "zlim_core::GroupBegin");
    assert_eq!(group.jobs[1].name(), "zlim_core::GroupEnd");
    assert_eq!(group.jobs[2].name(), "group_label_a");
    assert_eq!(group.condition, None);
    assert_eq!(group.order, &[(0, 1), (0, 2)]);
    assert_eq!(group.weak_order, &[]);
    assert_eq!(group.relaxed_order, &[(2, 1)]);
}

#[test]
fn job_group_generic_not_auto_registered() {
    JobGroup::collect();
    assert!(JobGroup::get("generic_group<u32>").is_none());
}

// -----------------------------------------------------------------------------
// register() registers the group's job labels

#[job_fn(type = GenericGroupJob<T: Default>, name = "generic_group_job")]
fn generic_group_job<T: Default>() {}

job_group! {
    type: RegisterGroup<T: Default>,
    name: "register_group",
    jobs: [GenericGroupJob<T>],
    condition: GenericGroupJob<T>,
}

#[test]
fn job_group_register_registers_job_labels() {
    JobDB::collect();
    JobGroup::collect();

    // Generic job markers are not auto-registered at startup.
    assert!(JobDB::get("generic_group_job<u32>").is_none());

    // `register` registers the type-based jobs and condition before the
    // group itself; string slots would be skipped.
    RegisterGroup::<u32>::register();

    assert!(JobDB::get("generic_group_job<u32>").is_some());
    assert!(JobGroup::get("register_group<u32>").is_some());
}
