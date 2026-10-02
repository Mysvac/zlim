//! Integration tests for `scn!` and `scn_list!`.

use zlim_app::App;
use zlim_asset::assets::Assets;
use zlim_asset::plugin::AssetPlugin;
use zlim_asset::server::AssetServer;
use zlim_core::derive::{Component, FromTemplate};
use zlim_core::entity::EntityId;
use zlim_core::error::ZlimResult;
use zlim_core::world::World;

use zlim_scene::{ResolveContext, ResolvedScene, Scene, ScenePatch};
use zlim_scene::{ScenePlugin, WorldSceneExt, scn, scn_list};

// -----------------------------------------------------------------------------
// Types

/// A component that is `Clone + Default`, so it is its own template.
#[derive(Component, Clone, Default, Debug, PartialEq)]
struct Scale(f32);

/// A component with two fields, so that a scene can edit one of them and leave the other alone.
#[derive(Component, Clone, Default, Debug, PartialEq)]
struct Health {
    current: u32,
    max: u32,
}

/// A component that points at another entity, described through a derived template.
#[derive(Component, Clone, FromTemplate, Debug, PartialEq)]
struct Link {
    to: EntityId,
}

/// A component with a constructor of its own, for the `Type::function(args)` form of an entry.
#[derive(Component, Clone, Default, Debug, PartialEq)]
struct Pair {
    left: u32,
    right: u32,
}

impl Pair {
    /// A `Pair` with both values set — what `Type::function(args)` replaces the template with.
    fn both(value: u32) -> Self {
        Self {
            left: value,
            right: value,
        }
    }
}

/// A scene written in code, which `@` includes as it is.
#[derive(Clone, Copy)]
struct Inline(f32);

impl Scene for Inline {
    fn resolve(self, _context: &mut ResolveContext, scene: &mut ResolvedScene) -> ZlimResult<()> {
        scene.get_or_insert_template::<Scale>().0 = self.0;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Helpers

/// Returns the [`Scale`] of `entity`, if it has one.
fn scale(world: &mut World, entity: EntityId) -> Option<Scale> {
    world.entity_owned(entity).get::<Scale>().cloned()
}

/// Builds an app with the asset system and the scene plugin, which is what a cached scene needs.
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

// -----------------------------------------------------------------------------
// Tests

/// Components and children are described by the macro, and applying the scene writes them.
#[test]
fn scn_describes_components_and_children() {
    let mut world = World::alloc();

    let root = world
        .spawn_scene(
            scn! {
                #Root
                Scale(1.5)
                Children [
                    Scale(2.5)
                    --
                    Scale(3.5)
                ]
            },
            None,
        )
        .expect("the scene spawns");

    assert_eq!(
        world.entity_owned(root).get::<Scale>().cloned(),
        Some(Scale(1.5))
    );

    let children = world
        .entity_owned(root)
        .children()
        .expect("the root is live")
        .to_vec();

    assert_eq!(children.len(), 2);
    assert_eq!(
        world.entity_owned(children[0]).get::<Scale>().cloned(),
        Some(Scale(2.5))
    );
    assert_eq!(
        world.entity_owned(children[1]).get::<Scale>().cloned(),
        Some(Scale(3.5))
    );
}

/// A `#Name` used as a value resolves to the entity that declared it.
#[test]
fn scn_names_resolve_to_their_entities() {
    let mut world = World::alloc();

    let roots = world
        .spawn_scene_list(
            scn_list! {
                #Left
                Scale(1.0)
                --
                Link { to: #Left }
                Scale(2.0)
            },
            None,
        )
        .expect("the list spawns");

    assert_eq!(roots.len(), 2);
    assert_eq!(
        world
            .entity_owned(roots[1])
            .get::<Link>()
            .map(|link| link.to),
        Some(roots[0])
    );
}

/// `Parent(...)` is applied once every entity of the list exists, so one root can point at another.
#[test]
fn scn_parents_one_root_under_another() {
    let mut world = World::alloc();

    let roots = world
        .spawn_scene_list(
            scn_list! {
                #Root
                Scale(1.0)
                --
                Scale(2.0)
                Parent(#Root)
            },
            None,
        )
        .expect("the list spawns");

    assert_eq!(
        world.entity_owned(roots[1]).parent().expect("live"),
        Some(roots[0])
    );
    assert_eq!(
        world
            .entity_owned(roots[0])
            .children()
            .expect("live")
            .to_vec(),
        vec![roots[1]]
    );
}

/// The roots of a list are all spawned and named before any of them is written, so one may point at
/// a name that a *later* root declares.
#[test]
fn scn_names_reach_across_roots_in_either_order() {
    let mut world = World::alloc();

    let roots = world
        .spawn_scene_list(
            scn_list! {
                Link { to: #Right }
                --
                #Right
                Scale(1.0)
            },
            None,
        )
        .expect("the list spawns");

    assert_eq!(
        world
            .entity_owned(roots[0])
            .get::<Link>()
            .map(|link| link.to),
        Some(roots[1])
    );
}

/// A name nobody declares is an error, rather than an entity invented on the spot.
#[test]
fn scn_an_undeclared_name_is_an_error() {
    let mut world = World::alloc();

    let result = world.spawn_scene(scn! { Link { to: #Nowhere } }, None);

    assert!(result.is_err(), "nothing in the scene declares `#Nowhere`");
}

/// A cached scene is named by path, and only the asset server can turn a path into a handle: a world
/// without one reports that instead of resolving something else.
#[test]
fn scn_a_cached_scene_needs_an_asset_server() {
    let mut world = World::alloc();

    assert!(
        world
            .spawn_scene(scn! { : "base.scene"  Scale(2.0) }, None)
            .is_err()
    );
}

/// `@ expression` includes a scene where it is written, so the entries around it apply in order.
#[test]
fn scn_includes_an_inline_scene_where_it_is_written() {
    let mut world = World::alloc();

    // The patch is written after the scene it edits, so the patch wins.
    let patched = world
        .spawn_scene(scn! { @ Inline(1.0)  Scale(2.0) }, None)
        .expect("the scene spawns");
    assert_eq!(scale(&mut world, patched), Some(Scale(2.0)));

    // The scene is written after the patch, so the scene wins.
    let overridden = world
        .spawn_scene(scn! { Scale(2.0)  @ Inline(1.0) }, None)
        .expect("the scene spawns");
    assert_eq!(scale(&mut world, overridden), Some(Scale(1.0)));
}

/// A scene is the tuple of the runs it is written in, and a tuple holds twelve parts: this is the
/// longest scene that still fits. Alternating a scene with every statement is what costs parts, and
/// the entry written last is the one that survives.
#[test]
fn scn_a_scene_of_twelve_runs_still_fits() {
    let mut world = World::alloc();

    let root = world
        .spawn_scene(
            scn! {
                @ Inline(0.5)
                #Root
                @ Inline(1.0)
                Scale(2.0)
                @ Inline(3.0)
                Scale(4.0)
                @ Inline(5.0)
                Scale(6.0)
                @ Inline(7.0)
                Scale(8.0)
                @ Inline(9.0)
                Scale(10.0)
            },
            None,
        )
        .expect("the scene spawns");

    assert_eq!(scale(&mut world, root), Some(Scale(10.0)));
}

/// A scene list is the tuple of its entities, and a tuple holds twelve of them.
#[test]
fn scn_a_list_of_twelve_entities_still_fits() {
    let mut world = World::alloc();

    let roots = world
        .spawn_scene_list(
            scn_list! {
                Scale(0.0) -- Scale(1.0) -- Scale(2.0) -- Scale(3.0)
                -- Scale(4.0) -- Scale(5.0) -- Scale(6.0) -- Scale(7.0)
                -- Scale(8.0) -- Scale(9.0) -- Scale(10.0) -- Scale(11.0)
            },
            None,
        )
        .expect("the list spawns");

    assert_eq!(roots.len(), 12);
    assert_eq!(scale(&mut world, roots[11]), Some(Scale(11.0)));
}

/// `Type::function(args)` replaces the canonical template with what the call produces, where
/// `Type { … }` edits the one that is already there.
#[test]
fn scn_a_constructor_entry_replaces_the_template() {
    let mut world = World::alloc();

    let root = world
        .spawn_scene(
            scn! {
                Pair { left: 1, right: 2 }
                Pair::both(9)
            },
            None,
        )
        .expect("the scene spawns");

    assert_eq!(
        world.entity_owned(root).get::<Pair>().cloned(),
        Some(Pair { left: 9, right: 9 }),
        "the replacement is what the canonical slot holds"
    );
}

/// `: expression` names a cached scene asset by path, and the templates it contributes can be edited
/// in place: what the entry does not mention is taken over from the cached scene.
#[test]
fn scn_builds_on_a_cached_scene_by_path() {
    let mut app = scene_app();

    let world = app.main_world_mut();
    let server = world
        .get_resource::<AssetServer>()
        .expect("the asset plugin inserts a server")
        .clone();

    // Nothing loads a scene asset yet — there is no loader for one — so the handle is asked for by
    // path and the resolved patch is then published under it by hand.
    let handle = server.load::<ScenePatch>("base.scene");

    let mut base = ScenePatch::new(scn! { Health { current: 10, max: 20 } });

    {
        let mut patches = world
            .get_resource_mut::<Assets<ScenePatch>>()
            .expect("the scene plugin registers the patches");
        base.resolve(Some(&server), &mut patches)
            .expect("the base resolves");
        patches
            .insert(handle.id(), base)
            .expect("the base is published");
    }

    // The cached scene contributes `max: 20`; the patch edits `current` alone, and `max` survives —
    // which is the copy-on-write: the template was cloned out of the cached scene, not created.
    let root = world
        .spawn_scene(scn! { : "base.scene"  Health { current: 5 } }, None)
        .expect("the scene spawns");

    let health = world.entity_owned(root).get::<Health>().cloned();
    assert_eq!(
        health,
        Some(Health {
            current: 5,
            max: 20
        })
    );
}
