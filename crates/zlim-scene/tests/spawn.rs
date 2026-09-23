//! Integration tests for the queued scene API: the `SpawnScene` schedule, `ScenePatchInstance` and
//! `SceneListPatchInstance`.

use zlim_app::App;
use zlim_asset::plugin::AssetPlugin;
use zlim_core::component::Component;
use zlim_core::entity::EntityId;
use zlim_core::world::World;
use zlim_path::TypePath;

use zlim_scene::{SceneListPatchInstance, ScenePatchInstance, ScenePlugin};
use zlim_scene::{WorldSceneQueueExt, scn, scn_list};

// -----------------------------------------------------------------------------
// Types

/// A component that is `Clone + Default`, so it is its own template.
#[derive(TypePath, Component, Clone, Default, Debug, PartialEq)]
struct Scale(f32);

/// Builds an app with the asset system and the scene plugin, which is what a queued scene needs.
///
/// Watching is turned off: the tests never load from the file system, and a watcher would keep a
/// thread alive for the whole test binary.
fn scene_app() -> App {
    let mut app = App::new();
    app.add_plugins(AssetPlugin {
        watch_for_changes_override: Some(false),
        ..AssetPlugin::default()
    });
    app.add_plugins(ScenePlugin);
    app.build();
    app
}

/// Returns the [`Scale`] of `entity`, if it has one.
fn scale(world: &mut World, entity: EntityId) -> Option<Scale> {
    world.entity_owned(entity).get::<Scale>().cloned()
}

// -----------------------------------------------------------------------------
// Tests

/// A scene queued for an entity is built by the `SpawnScene` schedule, not by the call that queued
/// it: the entity exists first, so it can be pointed at.
#[test]
fn a_queued_scene_is_built_when_the_spawn_scene_schedule_runs() {
    let mut app = scene_app();

    let entity = app
        .main_world_mut()
        .queue_spawn_scene(scn! { Scale(1.5) }, None)
        .expect("the scene is queued");

    assert!(
        scale(app.main_world_mut(), entity).is_none(),
        "the scene is only described so far"
    );
    assert!(
        app.main_world_mut()
            .entity_owned(entity)
            .get::<ScenePatchInstance>()
            .is_some(),
        "the request is what the schedule looks for"
    );

    app.update();

    assert_eq!(scale(app.main_world_mut(), entity), Some(Scale(1.5)));
    assert!(
        app.main_world_mut()
            .entity_owned(entity)
            .get::<ScenePatchInstance>()
            .is_none(),
        "a request is removed once it has been answered"
    );
}

/// A queued scene is applied *to* the entity it was queued for, rather than spawned as a new one.
#[test]
fn a_queued_scene_is_applied_to_the_entity_it_names() {
    let mut app = scene_app();

    let target = app.main_world_mut().spawn_empty(None).id();
    app.main_world_mut()
        .queue_apply_scene(scn! { Scale(2.5) }, target)
        .expect("the scene is queued");

    assert!(scale(app.main_world_mut(), target).is_none());
    app.update();

    assert_eq!(scale(app.main_world_mut(), target), Some(Scale(2.5)));
}

/// A queued scene list spawns one entity per scene of the list, under the entity it was queued for.
#[test]
fn a_queued_scene_list_spawns_under_its_entity() {
    let mut app = scene_app();

    let holder = app
        .main_world_mut()
        .queue_spawn_scene_list(
            scn_list! {
                Scale(1.0)
                --
                Scale(2.0)
            },
            None,
        )
        .expect("the list is queued");

    assert!(
        app.main_world_mut()
            .entity_owned(holder)
            .get::<SceneListPatchInstance>()
            .is_some()
    );

    app.update();

    let world = app.main_world_mut();
    let children = world
        .entity_owned(holder)
        .children()
        .expect("the entity is live")
        .to_vec();

    assert_eq!(children.len(), 2);
    assert_eq!(scale(world, children[0]), Some(Scale(1.0)));
    assert_eq!(scale(world, children[1]), Some(Scale(2.0)));
    assert!(
        world
            .entity_owned(holder)
            .get::<SceneListPatchInstance>()
            .is_none(),
        "a request is removed once it has been answered"
    );
}

/// A queued scene is applied by the first run that finds it ready, and the request is only taken out
/// once it has been: a later frame still builds it.
#[test]
fn a_queued_scene_is_built_by_a_later_frame() {
    let mut app = scene_app();

    let entity = app
        .main_world_mut()
        .queue_spawn_scene(scn! { Scale(3.5) }, None)
        .expect("the scene is queued");

    // The scene is applied on the first `SpawnScene` run, which is part of the first frame; earlier
    // frames only matter for a scene whose assets are still loading.
    app.update();
    assert!(scale(app.main_world_mut(), entity).is_some());

    app.update();
    assert_eq!(
        scale(app.main_world_mut(), entity),
        Some(Scale(3.5)),
        "a scene is applied once, not once per frame"
    );
}

/// Queueing a scene in a world without the scene assets is an error rather than a silent no-op.
#[test]
fn queueing_a_scene_without_the_scene_plugin_is_an_error() {
    let mut world = World::alloc();

    assert!(world.queue_spawn_scene(scn! { Scale(1.0) }, None).is_err());
    assert!(
        world
            .queue_spawn_scene_list(scn_list! { Scale(1.0) }, None)
            .is_err()
    );
}

/// The entity a scene is queued for has to exist: queueing onto a stale id is an error, rather than
/// a request nobody will ever see.
#[test]
fn queueing_a_scene_for_a_missing_entity_is_an_error() {
    let mut app = scene_app();

    let entity = app.main_world_mut().spawn_empty(None).id();
    app.main_world_mut()
        .despawn(entity)
        .expect("the entity is live");

    assert!(
        app.main_world_mut()
            .queue_apply_scene(scn! { Scale(1.0) }, entity)
            .is_err()
    );
}
