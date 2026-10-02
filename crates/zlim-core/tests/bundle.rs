//! Integration tests for `#[derive(Bundle)]`.

use zlim_core::derive::{Bundle, Component};
use zlim_core::entity::EntityId;
use zlim_core::world::World;

// -----------------------------------------------------------------------------
// Components

#[derive(Component, Clone, Debug, PartialEq)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Component, Clone, Debug, PartialEq)]
struct Velocity {
    dx: f32,
    dy: f32,
}

#[derive(Component, Clone, Debug, PartialEq)]
struct Health(u32);

// -----------------------------------------------------------------------------
// Named struct bundles

#[derive(Bundle)]
struct MovableBundle {
    position: Position,
    velocity: Velocity,
}

#[test]
fn named_bundle_spawns_all_fields() {
    let mut world = World::alloc();

    let entity = world.spawn(
        MovableBundle {
            position: Position { x: 1.0, y: 2.0 },
            velocity: Velocity { dx: 3.0, dy: 4.0 },
        },
        None,
    );

    assert!(entity.contains::<Position>());
    assert!(entity.contains::<Velocity>());
    assert!(!entity.contains::<Health>());
    assert_eq!(entity.get::<Health>(), None);
    assert_eq!(entity.get::<Position>(), Some(&Position { x: 1.0, y: 2.0 }));
    assert_eq!(
        entity.get::<Velocity>(),
        Some(&Velocity { dx: 3.0, dy: 4.0 })
    );
}

#[test]
fn named_bundle_spawns_at_given_entity() {
    let mut world = World::alloc();
    let id = EntityId::from_bits(0x0000_0001_0000_0001).unwrap();

    let bundle = MovableBundle {
        position: Position { x: 5.0, y: 6.0 },
        velocity: Velocity { dx: 7.0, dy: 8.0 },
    };
    let entity = world.spawn_at(bundle, id, None);

    assert_eq!(entity.id(), id);
    assert!(entity.contains::<Position>());
    assert!(entity.contains::<Velocity>());
}

// -----------------------------------------------------------------------------
// Bundles without sub-bundles

#[derive(Bundle)]
struct HealthBundle {
    position: Position,
    health: Health,
}

#[test]
fn bundle_spawns() {
    let mut world = World::alloc();

    let entity = world.spawn(
        HealthBundle {
            position: Position { x: 1.0, y: 2.0 },
            health: Health(100),
        },
        None,
    );

    assert_eq!(entity.get::<Position>(), Some(&Position { x: 1.0, y: 2.0 }));
    assert_eq!(entity.get::<Health>(), Some(&Health(100)));
}

#[test]
fn bundle_batch_spawns() {
    let mut world = World::alloc();

    let entities: Vec<EntityId> = world
        .spawn_batch::<HealthBundle, _>(
            [
                HealthBundle {
                    position: Position { x: 0.0, y: 0.0 },
                    health: Health(1),
                },
                HealthBundle {
                    position: Position { x: 1.0, y: 1.0 },
                    health: Health(2),
                },
            ],
            None,
        )
        .collect();

    assert_eq!(entities.len(), 2);
    assert_eq!(world.entity_count(), 2);
}

/// A derived bundle names its components, so the removal APIs take it as they
/// take any other bundle.
#[test]
fn derived_bundle_removes_its_components() {
    let mut world = World::alloc();
    let mut entity = world.spawn(
        HealthBundle {
            position: Position { x: 1.0, y: 2.0 },
            health: Health(100),
        },
        None,
    );

    entity.remove::<HealthBundle>().unwrap();

    assert!(!entity.contains::<Position>());
    assert!(!entity.contains::<Health>());
}

// -----------------------------------------------------------------------------
// Tuple struct bundles

#[derive(Bundle)]
struct TupleBundle(Position, Velocity);

#[test]
fn tuple_bundle_spawns() {
    let mut world = World::alloc();

    let entity = world.spawn(
        TupleBundle(Position { x: 1.0, y: 2.0 }, Velocity { dx: 3.0, dy: 4.0 }),
        None,
    );

    assert_eq!(entity.get::<Position>(), Some(&Position { x: 1.0, y: 2.0 }));
    assert_eq!(
        entity.get::<Velocity>(),
        Some(&Velocity { dx: 3.0, dy: 4.0 })
    );
}

// -----------------------------------------------------------------------------
// Unit struct bundles

#[derive(Bundle)]
struct UnitBundle;

#[test]
fn unit_bundle_spawns_empty_entity() {
    let mut world = World::alloc();

    let entity = world.spawn(UnitBundle, None);

    assert!(entity.is_spawned());
    assert!(!entity.contains::<Position>());
    assert_eq!(world.entity_count(), 1);
}

// -----------------------------------------------------------------------------
// Nested bundles

#[derive(Bundle)]
struct NestedBundle {
    tuple: TupleBundle,
    health: Health,
}

/// A field that is itself a bundle is flattened into the outer one, so the
/// entity receives the inner bundle's components directly, with no trace of the
/// nesting left behind.
#[test]
fn nested_bundle_flattens_fields() {
    let mut world = World::alloc();

    let entity = world.spawn(
        NestedBundle {
            tuple: TupleBundle(Position { x: 1.0, y: 2.0 }, Velocity { dx: 3.0, dy: 4.0 }),
            health: Health(100),
        },
        None,
    );

    assert!(entity.contains::<Position>());
    assert!(entity.contains::<Velocity>());
    assert_eq!(entity.get::<Health>(), Some(&Health(100)));
}

// -----------------------------------------------------------------------------
// Generic bundles

#[derive(Bundle)]
struct GenericBundle<T> {
    value: T,
}

/// The type parameter is filled with a tuple of components, so the derived
/// implementation has to forward to the tuple's own bundle implementation
/// rather than treating the value as a single component.
#[test]
fn generic_bundle_spawns() {
    let mut world = World::alloc();

    let entity = world.spawn(
        GenericBundle {
            value: (Position { x: 1.0, y: 2.0 }, Velocity { dx: 3.0, dy: 4.0 }),
        },
        None,
    );

    assert!(entity.contains::<Position>());
    assert!(entity.contains::<Velocity>());
    assert_eq!(entity.get::<Health>(), None);
}

// -----------------------------------------------------------------------------
// Inserting derived bundles into existing entities

/// Inserting a bundle into a live entity has to overwrite the components the
/// entity already carries and add the ones it lacks, rather than failing or
/// leaving duplicates behind.
#[test]
fn insert_derived_bundle_overwrites_components() {
    let mut world = World::alloc();
    let mut entity = world.spawn(Position { x: 0.0, y: 0.0 }, None);

    entity
        .insert(MovableBundle {
            position: Position { x: 9.0, y: 9.0 },
            velocity: Velocity { dx: -1.0, dy: 1.0 },
        })
        .unwrap();

    assert_eq!(entity.get::<Position>(), Some(&Position { x: 9.0, y: 9.0 }));
    assert_eq!(
        entity.get::<Velocity>(),
        Some(&Velocity { dx: -1.0, dy: 1.0 })
    );
}
