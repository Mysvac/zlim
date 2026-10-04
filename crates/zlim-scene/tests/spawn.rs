//! Integration tests for the queued scene API: the `SpawnScene` schedule, `SceneQueue`, and the
//! `queue_*` entry points that fill it.

use zlim_app::App;
use zlim_asset::plugin::AssetPlugin;
use zlim_core::component::Component;
use zlim_core::entity::EntityId;
use zlim_core::world::World;

use zlim_scene::{CommandsSceneExt, EntityCommandsExt, ScenePlugin, WorldSceneExt, scn, scn_list};

// -----------------------------------------------------------------------------
// Types

/// A component that is `Clone + Default`, so it is its own template.
#[derive(Component, Clone, Default, Debug, PartialEq)]
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

/// A scene queued for spawning is given an empty entity at once, so there is something to point at
/// while its components are still loading; the scene itself arrives with the `SpawnScene` run.
#[test]
fn a_queued_scene_is_built_when_the_spawn_scene_schedule_runs() {
    let mut app = scene_app();

    let before = app.main_world_mut().entities().count_spawned();

    app.main_world_mut()
        .queue_spawn_scene(scn! { Scale(1.5) }, None)
        .expect("the scene is queued");

    assert_eq!(
        app.main_world_mut().entities().count_spawned(),
        before + 1,
        "the entity is made as soon as the scene is queued"
    );

    let entity = app
        .main_world_mut()
        .entities()
        .root_entities()
        .last()
        .expect("the entity exists");
    assert_eq!(
        scale(app.main_world_mut(), entity),
        None,
        "and it is empty until the scene is ready"
    );

    app.update();

    assert_eq!(
        scale(app.main_world_mut(), entity),
        Some(Scale(1.5)),
        "the scene was applied to the entity that was already there"
    );
    assert_eq!(
        app.main_world_mut().entities().count_spawned(),
        before + 1,
        "the scene did not make a second entity"
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

    let holder = app.main_world_mut().spawn_empty(None).id();
    app.main_world_mut()
        .queue_spawn_scene_list(
            scn_list! {
                Scale(1.0)
                --
                Scale(2.0)
            },
            Some(holder),
        )
        .expect("the list is queued");

    assert!(
        app.main_world_mut()
            .entity_owned(holder)
            .children()
            .is_ok_and(<[EntityId]>::is_empty),
        "nothing is spawned until the job runs"
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

/// A request that names an entity which is gone by the time it is answered is reported and dropped,
/// rather than bringing the run down: the queueing call could not have known, since it resolves
/// nothing.
#[test]
fn a_request_naming_a_missing_entity_is_dropped() {
    let mut app = scene_app();

    let target = app.main_world_mut().spawn_empty(None).id();
    let parent = app.main_world_mut().spawn_empty(None).id();

    app.main_world_mut()
        .queue_apply_scene(scn! { Scale(2.5) }, target)
        .expect("the scene is queued");
    app.main_world_mut()
        .queue_spawn_scene_list(scn_list! { Scale(1.0) }, Some(parent))
        .expect("the list is queued");

    app.main_world_mut()
        .despawn(target)
        .expect("the entity is live");
    app.main_world_mut()
        .despawn(parent)
        .expect("the entity is live");

    // Neither request can be built where it asked, and neither is left behind.
    app.update();

    let world = app.main_world_mut();
    assert!(world.get_entity_ref(target).is_err());
    assert!(world.get_entity_ref(parent).is_err());
    assert!(
        world
            .get_resource::<zlim_scene::SceneQueue>()
            .is_some_and(zlim_scene::SceneQueue::is_empty),
        "a request nothing can answer is dropped rather than retried for ever"
    );
}

// -----------------------------------------------------------------------------
// Commands
//
// A command is deferred twice: the command queue runs at the end of the schedule, and the scene then
// waits in `SceneQueue` for its assets. These tests drive the command queue by hand, since what they
// are about is the command, not the frame.

/// A scene queued as a command spawns its entity when the command queue runs, and the entity it
/// hands back is the one that will exist.
#[test]
fn a_scene_command_spawns_when_the_queue_runs() {
    let mut app = scene_app();

    let entity = {
        let world = app.main_world_mut();
        let mut commands = world.commands();
        let entity = commands.spawn_scene(scn! { Scale(3.5) }, None);
        let id = entity.id();
        assert!(
            world.get_entity_ref(id).is_err(),
            "the command has not run yet, so the entity is not there"
        );
        id
    };

    app.main_world_mut().flush();
    app.update();

    assert_eq!(scale(app.main_world_mut(), entity), Some(Scale(3.5)));
}

/// `apply_scene` is the same deferral for an entity that already exists, and it chains: the value it
/// returns is the same `EntityCommands`, so further commands can be hung on it.
#[test]
fn a_scene_command_applies_and_chains() {
    let mut app = scene_app();

    let target = app.main_world_mut().spawn_empty(None).id();

    {
        let world = app.main_world_mut();
        let mut commands = world.commands();
        commands
            .with_entity(target)
            .apply_scene(scn! { Scale(4.5) })
            .apply_scene(scn! { Scale(5.5) });
    }

    app.main_world_mut().flush();
    app.update();

    assert_eq!(
        scale(app.main_world_mut(), target),
        Some(Scale(5.5)),
        "both scenes were queued, and the one written last wins"
    );
}

/// A scene list queued as a command spawns its entities under the parent, and hands nothing back
/// because a list describes more than one entity.
#[test]
fn a_scene_list_command_spawns_under_its_parent() {
    let mut app = scene_app();

    let parent = app.main_world_mut().spawn_empty(None).id();

    {
        let world = app.main_world_mut();
        let mut commands = world.commands();
        commands.spawn_scene_list(
            scn_list! {
                Scale(1.0)
                --
                Scale(2.0)
            },
            Some(parent),
        );
    }

    app.main_world_mut().flush();
    app.update();

    let world = app.main_world_mut();
    let children = world
        .entity_owned(parent)
        .children()
        .expect("the parent is live")
        .to_vec();

    assert_eq!(children.len(), 2);
    assert_eq!(scale(world, children[0]), Some(Scale(1.0)));
    assert_eq!(scale(world, children[1]), Some(Scale(2.0)));
}
