//! Tests for the two-stage transform propagation
//! (`TransformChangeDetection` pollution pass + `TransformPropagation`).

use zlim_core::borrow::Ref;
use zlim_core::entity::EntityId;
use zlim_core::job::{JobId, JobLabel};
use zlim_core::job_fn;
use zlim_core::query::Query;
use zlim_core::schedule::{AnonymousSchedule, Schedule};
use zlim_core::system::Local;
use zlim_core::tick::{DetectChanges, Tick};
use zlim_core::world::World;
use zlim_transform::{GlobalTransform, Transform, TransformChangeDetection};
use zlim_transform::{TransformChangeRoot, TransformPropagateStrategy, TransformPropagation};

fn init_config(world: &mut World) {
    world.init_resource::<TransformPropagateStrategy>();
    world.init_resource::<TransformChangeRoot>();
}

/// One persistent schedule (like an `App` frame), run once per frame.
fn make_schedule() -> Schedule {
    let mut schedule = Schedule::new(AnonymousSchedule);
    schedule.insert::<TransformChangeDetection>(());
    schedule.insert::<TransformPropagation>(());
    schedule.insert_order(&[
        JobId::isolated(TransformChangeDetection::name()),
        JobId::isolated(TransformPropagation::name()),
    ]);
    schedule
}

#[test]
fn propagates_to_descendants() {
    let mut world = World::alloc();
    init_config(&mut world);

    let mut root = world.spawn(Transform::from_xyz(1.0, 0.0, 0.0), None);
    root.with_child(Transform::from_xyz(0.0, 2.0, 0.0)).unwrap();
    let root_id = root.id();
    let child_id = root.children().unwrap()[0];
    let _ = root;

    let mut child = world.entity_owned(child_id);
    child
        .with_child(Transform::from_xyz(0.0, 0.0, 3.0))
        .unwrap();
    let grandchild_id = child.children().unwrap()[0];
    let _ = child;

    make_schedule().run(&mut world);

    assert_eq!(
        world.entity(root_id).get::<GlobalTransform>().unwrap(),
        &GlobalTransform::from(Transform::from_xyz(1.0, 0.0, 0.0)),
    );

    assert_eq!(
        world.entity(child_id).get::<GlobalTransform>().unwrap(),
        &GlobalTransform::from(Transform::from_xyz(1.0, 2.0, 0.0)),
    );

    assert_eq!(
        world
            .entity(grandchild_id)
            .get::<GlobalTransform>()
            .unwrap(),
        &GlobalTransform::from(Transform::from_xyz(1.0, 2.0, 3.0)),
    );
}

#[test]
fn local_transform_change_propagates() {
    let mut world = World::alloc();
    init_config(&mut world);

    let mut root = world.spawn(Transform::from_xyz(1.0, 0.0, 0.0), None);
    root.with_child(Transform::from_xyz(0.0, 2.0, 0.0)).unwrap();

    let root_id = root.id();
    let child_id = root.children().unwrap()[0];
    let _ = root;

    let mut schedule = make_schedule();
    schedule.run(&mut world);

    assert_eq!(
        world.entity(child_id).get::<GlobalTransform>().unwrap(),
        &GlobalTransform::from(Transform::from_xyz(1.0, 2.0, 0.0)),
    );

    // Move the root; the child's global transform must follow.
    world
        .entity_mut(root_id)
        .get_mut::<Transform>()
        .unwrap()
        .translation = zlim_math::Vec3::new(5.0, 0.0, 0.0);

    schedule.run(&mut world);

    assert_eq!(
        world.entity(child_id).get::<GlobalTransform>().unwrap(),
        &GlobalTransform::from(Transform::from_xyz(5.0, 2.0, 0.0)),
    );
}

#[test]
fn parented_root_recomputes_own_global() {
    let mut world = World::alloc();
    init_config(&mut world);

    let mut a = world.spawn(Transform::from_xyz(1.0, 0.0, 0.0), None);
    a.with_child(Transform::from_xyz(0.0, 2.0, 0.0)).unwrap();
    let a_id = a.id();
    let b_id = a.children().unwrap()[0];
    let _ = a;

    let mut b = world.entity_owned(b_id);
    b.with_child(Transform::from_xyz(0.0, 0.0, 3.0)).unwrap();
    let c_id = b.children().unwrap()[0];
    let _ = b;

    let mut schedule = make_schedule();

    // Frame 1: everything is new, so every global is computed.
    schedule.run(&mut world);
    assert_eq!(
        world.entity(b_id).get::<GlobalTransform>().unwrap(),
        &GlobalTransform::from(Transform::from_xyz(1.0, 2.0, 0.0)),
    );

    // Frame 2: only B's local transform changes.  B becomes a "subtree root"
    // whose parent (A) is unchanged — the parented-root path.
    world
        .entity_owned(b_id)
        .get_mut::<Transform>()
        .unwrap()
        .translation = zlim_math::Vec3::new(0.0, 3.0, 0.0);
    schedule.run(&mut world);

    assert_eq!(
        world.entity(b_id).get::<GlobalTransform>().unwrap(),
        &GlobalTransform::from(Transform::from_xyz(1.0, 3.0, 0.0)),
        "B's global transform was not recomputed",
    );
    assert_eq!(
        world.entity(a_id).get::<GlobalTransform>().unwrap(),
        &GlobalTransform::from(Transform::from_xyz(1.0, 0.0, 0.0)),
        "A's global transform was corrupted",
    );
    assert_eq!(
        world.entity(c_id).get::<GlobalTransform>().unwrap(),
        &GlobalTransform::from(Transform::from_xyz(1.0, 3.0, 3.0)),
    );
}

#[test]
fn multiple_roots_under_same_unchanged_parent() {
    let mut world = World::alloc();
    init_config(&mut world);

    let mut a = world.spawn(Transform::IDENTITY, None);
    let a_id = a.id();
    a.with_child(Transform::from_xyz(1.0, 0.0, 0.0)).unwrap();
    let b_id = a.children().unwrap()[0];
    a.with_child(Transform::from_xyz(2.0, 0.0, 0.0)).unwrap();
    let c_id = a.children().unwrap()[1];
    let _ = a;

    let mut schedule = make_schedule();
    schedule.run(&mut world);
    assert_eq!(
        world.entity(b_id).get::<GlobalTransform>().unwrap(),
        &GlobalTransform::from(Transform::from_xyz(1.0, 0.0, 0.0)),
    );

    // Change both children: two subtree roots under the same unchanged parent.
    world
        .entity_owned(b_id)
        .get_mut::<Transform>()
        .unwrap()
        .translation = zlim_math::Vec3::new(10.0, 0.0, 0.0);
    world
        .entity_owned(c_id)
        .get_mut::<Transform>()
        .unwrap()
        .translation = zlim_math::Vec3::new(20.0, 0.0, 0.0);
    schedule.run(&mut world);

    assert_eq!(
        world.entity(b_id).get::<GlobalTransform>().unwrap(),
        &GlobalTransform::from(Transform::from_xyz(10.0, 0.0, 0.0)),
        "B's global transform was not recomputed",
    );
    assert_eq!(
        world.entity(c_id).get::<GlobalTransform>().unwrap(),
        &GlobalTransform::from(Transform::from_xyz(20.0, 0.0, 0.0)),
        "C's global transform was not recomputed",
    );
    assert_eq!(
        world.entity(a_id).get::<GlobalTransform>().unwrap(),
        &GlobalTransform::from(Transform::IDENTITY),
        "A's global transform was corrupted",
    );
}

/// Records the per-entity change ticks of [`GlobalTransform`] after
/// propagation, and fails if they changed since the previous frame.
///
/// This pins the tick contract the propagation relies on: the writes it made
/// last frame must not be seen as changes this frame, otherwise it would
/// re-write (and re-stamp) the whole tree every frame and the recorded ticks
/// would keep advancing.
#[job_fn(type = AssertStableGlobalTicks, auto_register = false)]
fn assert_stable_global_ticks(
    query: Query<(EntityId, Ref<GlobalTransform>)>,
    mut prev: Local<Option<Vec<(EntityId, Tick)>>>,
) {
    let cur: Vec<(EntityId, Tick)> = query
        .iter()
        .map(|(id, global)| (id, global.changed_tick()))
        .collect();

    if let Some(prev) = prev.as_ref() {
        assert_eq!(
            prev, &cur,
            "propagation must not re-write `GlobalTransform` in a frame after \
            its own writes: the previous frame's writes must not be reported \
            as changes by the current frame",
        );
    }

    *prev = Some(cur);
}

#[test]
fn previous_frame_propagation_writes_are_not_detected() {
    let mut world = World::alloc();
    init_config(&mut world);

    let mut root = world.spawn(Transform::from_xyz(1.0, 0.0, 0.0), None);
    root.with_child(Transform::from_xyz(0.0, 2.0, 0.0)).unwrap();
    let child_id = root.children().unwrap()[0];
    let _ = root;

    let mut child = world.entity_owned(child_id);
    child
        .with_child(Transform::from_xyz(0.0, 0.0, 3.0))
        .unwrap();
    let _ = child;

    let mut schedule = make_schedule();
    schedule.insert::<AssertStableGlobalTicks>(());
    schedule.insert_order(&[
        JobId::isolated(TransformChangeDetection::name()),
        JobId::isolated(TransformPropagation::name()),
        JobId::isolated(AssertStableGlobalTicks::name()),
    ]);

    // Frames 1-3: after the initial propagation, nothing is dirty, so the
    // recorded change ticks must stay identical frame over frame.
    schedule.run(&mut world);
    schedule.run(&mut world);
    schedule.run(&mut world);
}

#[test]
fn reparent_syncs_and_propagates() {
    let mut world = World::alloc();
    init_config(&mut world);

    let mut root = world.spawn(Transform::from_xyz(1.0, 0.0, 0.0), None);
    root.with_child(Transform::from_xyz(0.0, 2.0, 0.0)).unwrap();

    let root_id = root.id();
    let child_id = root.children().unwrap()[0];
    let _ = root;

    let mut schedule = make_schedule();

    schedule.run(&mut world);
    assert_eq!(
        world.entity(child_id).get::<GlobalTransform>().unwrap(),
        &GlobalTransform::from(Transform::from_xyz(1.0, 2.0, 0.0)),
    );

    // ReparentSignal the child under a new root.
    let new_parent = world.spawn(Transform::from_xyz(10.0, 0.0, 0.0), None);
    let new_parent_id = new_parent.id();
    let _ = new_parent;

    world
        .entity_owned(child_id)
        .reparent(Some(new_parent_id))
        .unwrap();

    schedule.run(&mut world);

    // The global transform must now be relative to the new parent.
    assert_eq!(
        world.entity(child_id).get::<GlobalTransform>().unwrap(),
        &GlobalTransform::from(Transform::from_xyz(10.0, 2.0, 0.0)),
    );
    // The old root's global transform is unaffected.
    assert_eq!(
        world.entity(root_id).get::<GlobalTransform>().unwrap(),
        &GlobalTransform::from(Transform::from_xyz(1.0, 0.0, 0.0)),
    );
}

#[test]
fn app_plugin_propagates() {
    use zlim_app::App;
    use zlim_transform::TransformPlugin;

    let mut app = App::new();
    app.add_plugins(TransformPlugin::default());
    app.build(); // executes the plugins (build → apply → cleanup)

    // Build the hierarchy: root -> child.
    let world = app.main_world_mut();
    let mut root = world.spawn(Transform::from_xyz(1.0, 0.0, 0.0), None);
    root.with_child(Transform::from_xyz(0.0, 2.0, 0.0)).unwrap();
    let child_id = root.children().unwrap()[0];
    let _ = root;

    // Run one frame: `TransformChangeDetection` → `TransformPropagation`
    // run inside `PostUpdate`.
    app.update();

    let world = app.main_world_mut();
    assert_eq!(
        world.entity(child_id).get::<GlobalTransform>().unwrap(),
        &GlobalTransform::from(Transform::from_xyz(1.0, 2.0, 0.0)),
    );
}
