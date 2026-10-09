//! Reading a scene document back into a world.
//!
//! The document format lives in `zlim-core`, which knows how to *read* a scene but nothing about the
//! resolved form; `zlim-scene` knows the resolved form but nothing about the format. These tests are
//! where the two meet: a world is described, serialized, read back, arranged into
//! [`ResolvedScene`]s, and applied to a second world — the whole path a scene asset takes.

use serde::de::DeserializeSeed;

use zlim_core::component::Component;
use zlim_core::entity::EntityId;
use zlim_core::scene::{BorrowedSceneSerial, DynamicEntity, DynamicScene};
use zlim_core::scene::{SceneEntityMapper, SceneVisitor};
use zlim_core::world::World;
use zlim_reflect::TypeDB;
use zlim_reflect::derive::{Reflect, TypePath};
use zlim_reflect::serde::EMPTY_CONTEXT;
use zlim_scene::ResolvedScene;

// -----------------------------------------------------------------------------
// Types

/// A component that a scene serializes, and whose fields are plain data.
#[derive(Default, Component, Clone, Debug, PartialEq, Reflect, TypePath)]
#[component(reflect, serialize)]
struct Label(String);

// -----------------------------------------------------------------------------
// Helpers

/// Serializes `roots` and everything under them into a RON document.
fn document_of(world: &World, roots: &[EntityId]) -> String {
    let mut builder = world.borrowed_scene_builder().skip_missing();
    for &root in roots {
        builder = builder.with_recursive(root);
    }
    let scene = builder.finish().expect("the entities exist");
    let driver = BorrowedSceneSerial {
        context: EMPTY_CONTEXT,
        scene: &scene,
    };
    ron::ser::to_string_pretty(&driver, ron::ser::PrettyConfig::default())
        .expect("a scene serializes")
}

/// Reads a document back into a [`DynamicScene`].
fn read_document(text: &str) -> DynamicScene {
    let mut mapper = SceneEntityMapper::new();
    let visitor = SceneVisitor {
        mapper: &mut mapper,
        context: EMPTY_CONTEXT,
    };
    let mut deserializer =
        ron::de::Deserializer::from_str(text).expect("the document is valid RON");
    visitor
        .deserialize(&mut deserializer)
        .expect("the document reads back")
}

/// Returns the labels of `roots` and everything under them, in traversal order.
fn labels_from(world: &mut World, roots: &[EntityId]) -> Vec<String> {
    let mut labels = Vec::new();
    let mut stack: Vec<EntityId> = roots.to_vec();
    while let Some(id) = stack.pop() {
        let entity = world.entity(id);
        if let Some(label) = entity.get::<Label>() {
            labels.push(label.0.clone());
        }
        stack.extend_from_slice(entity.children());
    }
    labels.sort();
    labels
}

// -----------------------------------------------------------------------------
// Tests

/// A parent and its child round-trip into a world with the same two labels.
#[test]
fn a_scene_round_trips_through_a_document() {
    TypeDB::collect();

    let mut source = World::alloc();
    let parent = source.spawn_empty(None).id();
    source
        .entity_owned(parent)
        .insert(Label("parent".into()))
        .expect("the component is inserted");
    let child = source.spawn_empty(Some(parent)).id();
    source
        .entity_owned(child)
        .insert(Label("child".into()))
        .expect("the component is inserted");

    let document = document_of(&source, &[parent]);
    let dynamic = read_document(&document);
    assert_eq!(dynamic.entities.len(), 2, "the child comes with its parent");

    let scenes = ResolvedScene::from_dynamic(dynamic).expect("the document describes a tree");
    assert_eq!(scenes.len(), 1, "the document describes one root");
    assert_eq!(scenes[0].children().len(), 1, "the root carries its child");

    let mut target = World::alloc();
    let roots = ResolvedScene::spawn_batch(&scenes, &mut target, None).expect("the scene applies");

    assert_eq!(
        labels_from(&mut target, &roots),
        vec!["child".to_string(), "parent".to_string()],
    );
}

/// The child is spawned *under* the root, so the hierarchy survives the round trip.
#[test]
fn the_hierarchy_survives_the_round_trip() {
    TypeDB::collect();

    let mut source = World::alloc();
    let parent = source.spawn_empty(None).id();
    let child = source.spawn_empty(Some(parent)).id();

    let document = document_of(&source, &[parent]);
    let dynamic = read_document(&document);
    let scenes = ResolvedScene::from_dynamic(dynamic).expect("the document describes a tree");

    let mut target = World::alloc();
    let roots = ResolvedScene::spawn_batch(&scenes, &mut target, None).expect("the scene applies");
    assert_eq!(roots.len(), 1);

    let children: Vec<EntityId> = target.entity(roots[0]).children().to_vec();
    assert_eq!(children.len(), 1, "the root has exactly one child");

    // The child the document declared is the one the source had: same index, so the same slot.
    assert_eq!(
        target.entity(children[0]).parent(),
        Some(roots[0]),
        "the child points back at the root",
    );
    assert!(target.contains_entity(children[0]));
    let _ = child;
}

/// An entity with no parent is a root of the document, and one document may describe several.
#[test]
fn several_roots_stay_separate_roots() {
    TypeDB::collect();

    let mut source = World::alloc();
    let first = source.spawn_empty(None).id();
    let second = source.spawn_empty(None).id();
    source
        .entity_owned(first)
        .insert(Label("first".into()))
        .expect("the component is inserted");
    source
        .entity_owned(second)
        .insert(Label("second".into()))
        .expect("the component is inserted");

    let document = document_of(&source, &[first, second]);
    let dynamic = read_document(&document);
    assert_eq!(dynamic.entities.len(), 2);

    let scenes = ResolvedScene::from_dynamic(dynamic).expect("the document describes a tree");
    assert_eq!(scenes.len(), 2, "each root keeps its own scene");
    assert!(scenes.iter().all(|scene| scene.children().is_empty()));

    let mut target = World::alloc();
    let roots = ResolvedScene::spawn_batch(&scenes, &mut target, None).expect("the scene applies");
    assert_eq!(
        labels_from(&mut target, &roots),
        vec!["first".to_string(), "second".to_string()],
    );
}

// -----------------------------------------------------------------------------
// Malformed documents
//
// A document can describe edges that no tree has. The serializer never writes one, so these scenes
// are built by hand — which is also how a hand-edited document arrives.

/// An entity with the given id and parent, and no components.
fn node(id: u32, parent: Option<u32>) -> DynamicEntity {
    let generation = core::num::NonZeroU32::MIN;
    DynamicEntity {
        id: EntityId::new(id, generation),
        parent: parent.map(|parent| EntityId::new(parent, generation)),
        components: Default::default(),
    }
}

/// A cycle has no root, so there is no tree to build.
#[test]
fn a_parent_cycle_is_an_error() {
    // 1 <- 2 <- 3 <- 1: every entity has a parent, and no root leads into the cycle.
    let dynamic = DynamicScene {
        entities: vec![node(1, Some(3)), node(2, Some(1)), node(3, Some(2))],
    };

    let error = ResolvedScene::from_dynamic(dynamic).expect_err("a cycle is not a tree");
    let text = error.to_string();
    assert!(text.contains("cycle"), "unexpected message: {text}");
}

/// An entity that names itself is a root, not a cycle of one.
#[test]
fn an_entity_that_names_itself_is_a_root() {
    let dynamic = DynamicScene {
        entities: vec![node(1, Some(1))],
    };

    let scenes = ResolvedScene::from_dynamic(dynamic).expect("a self parent is a root");
    assert_eq!(scenes.len(), 1);
    assert!(scenes[0].children().is_empty());
}

/// The same id declared twice leaves one entity with two parents.
#[test]
fn a_duplicated_entity_id_is_an_error() {
    // Two entities claim id 1 and both hang off entity 2, so 2's child slot is taken twice.
    let dynamic = DynamicScene {
        entities: vec![node(1, Some(2)), node(1, Some(2)), node(2, None)],
    };

    let error = ResolvedScene::from_dynamic(dynamic).expect_err("one id, one parent");
    let text = error.to_string();
    assert!(text.contains("twice"), "unexpected message: {text}");
}
