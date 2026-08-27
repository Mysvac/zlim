//! Integration tests for `#[derive(Bundle)]`.

use zlim_core::bundle::Bundle as BundleTrait;
use zlim_core::bundle::DataBundle;
use zlim_core::component::{ComponentCollector, ComponentWriter};
use zlim_core::derive::{Bundle, Component, Resource};
use zlim_core::entity::EntityId;
use zlim_core::ops::EntityOwned;
use zlim_core::world::World;
use zlim_ptr::OwningPtr;
use zlim_reflect::TypePath;

// -----------------------------------------------------------------------------
// Components

#[derive(TypePath, Component, Clone, Debug, PartialEq)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(TypePath, Component, Clone, Debug, PartialEq)]
struct Velocity {
    dx: f32,
    dy: f32,
}

#[derive(TypePath, Component, Clone, Debug, PartialEq)]
struct Health(u32);

// -----------------------------------------------------------------------------
// Named struct bundles

#[derive(Bundle)]
struct MovableBundle {
    position: Position,
    velocity: Velocity,
}

#[test]
fn named_bundle_consts() {
    // Plain component fields need no effect, so the OR of their flags is
    // `false`.
    const { assert!(!<MovableBundle as BundleTrait>::NEED_APPLY_EFFECT) };
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
    assert_eq!(entity.get::<Position>(), Some(&Position { x: 1.0, y: 2.0 }));
    assert_eq!(
        entity.get::<Velocity>(),
        Some(&Velocity { dx: 3.0, dy: 4.0 })
    );
    assert_eq!(entity.get::<Health>(), None);
}

#[test]
fn named_bundle_spawns_at_given_entity() {
    let mut world = World::alloc();
    let id = EntityId::from_bits(0x0000_0001_0000_0001).unwrap();

    let entity = world.spawn_at(
        MovableBundle {
            position: Position { x: 5.0, y: 6.0 },
            velocity: Velocity { dx: 7.0, dy: 8.0 },
        },
        id,
        None,
    );

    assert_eq!(entity.id(), id);
    assert!(entity.contains::<Position>());
    assert!(entity.contains::<Velocity>());
}

// -----------------------------------------------------------------------------
// Data bundles

#[derive(Bundle)]
#[bundle(data)]
struct DataOnlyBundle {
    position: Position,
    health: Health,
}

#[test]
fn data_bundle_consts() {
    const { assert!(!<DataOnlyBundle as BundleTrait>::NEED_APPLY_EFFECT) };

    fn assert_data_bundle<T: DataBundle>() {}
    assert_data_bundle::<DataOnlyBundle>();
}

#[test]
fn data_bundle_spawns() {
    let mut world = World::alloc();

    let entity = world.spawn(
        DataOnlyBundle {
            position: Position { x: 1.0, y: 2.0 },
            health: Health(100),
        },
        None,
    );

    assert_eq!(entity.get::<Position>(), Some(&Position { x: 1.0, y: 2.0 }));
    assert_eq!(entity.get::<Health>(), Some(&Health(100)));
}

#[test]
fn data_bundle_batch_spawns() {
    let mut world = World::alloc();

    let entities: Vec<EntityId> = world
        .spawn_batch::<DataOnlyBundle, _>(
            [
                DataOnlyBundle {
                    position: Position { x: 0.0, y: 0.0 },
                    health: Health(1),
                },
                DataOnlyBundle {
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

// -----------------------------------------------------------------------------
// Tuple struct bundles

#[derive(Bundle)]
#[bundle(data)]
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
fn unit_bundle_consts() {
    const { assert!(!<UnitBundle as BundleTrait>::NEED_APPLY_EFFECT) };
}

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
#[bundle(data)]
struct NestedBundle {
    tuple: TupleBundle,
    health: Health,
}

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

#[derive(Bundle)]
#[bundle(data)]
struct GenericDataBundle<T> {
    value: T,
}

#[test]
fn generic_bundle_consts() {
    // Plain component fields keep the OR `false`...
    const { assert!(!<GenericBundle<Health> as BundleTrait>::NEED_APPLY_EFFECT) };
    // ...while an effectful field makes it `true`.
    const { assert!(<GenericBundle<Effect> as BundleTrait>::NEED_APPLY_EFFECT) };
    const { assert!(!<GenericDataBundle<Health> as BundleTrait>::NEED_APPLY_EFFECT) };
}

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

#[test]
fn generic_data_bundle_impls_data_bundle() {
    fn assert_data_bundle<T: DataBundle>() {}
    assert_data_bundle::<GenericDataBundle<Health>>();
    assert_data_bundle::<GenericDataBundle<Position>>();
}

// -----------------------------------------------------------------------------
// Bundles with post-spawn effects

/// A test-only bundle whose `apply_effect` pushes its id into the world's
/// [`EffectLog`] resource — making post-spawn side effects observable.
struct Effect(u32);

#[expect(unsafe_code, reason = "test-only bundle implementation")]
unsafe impl BundleTrait for Effect {
    const NEED_APPLY_EFFECT: bool = true;

    fn collect_explicit(_: &mut ComponentCollector) {}

    fn collect_required(_: &mut ComponentCollector) {}

    unsafe fn write_explicit(_: OwningPtr<'_>, _: &mut ComponentWriter) {}

    unsafe fn write_required(_writer: &mut ComponentWriter) {}

    unsafe fn apply_effect(data: OwningPtr<'_>, entity: &mut EntityOwned<'_>) {
        // SAFETY: `data` points to a live, aligned `Effect` instance — this is
        // guaranteed by the `Bundle::apply_effect` contract.
        let id = unsafe { data.as_ref::<Self>() }.0;
        entity.resource_mut::<EffectLog>().0.push(id);
    }
}

#[derive(TypePath, Resource, Debug, PartialEq)]
struct EffectLog(Vec<u32>);

#[derive(Bundle)]
struct EffectBundle {
    first: Effect,
    second: Effect,
}

#[derive(Bundle)]
struct NestedEffectBundle {
    inner: EffectBundle,
    tail: Effect,
}

#[test]
fn effect_bundle_needs_apply_effect() {
    const { assert!(<EffectBundle as BundleTrait>::NEED_APPLY_EFFECT) };
}

#[test]
fn effect_bundle_applies_effects_in_field_order() {
    let mut world = World::alloc();
    world.insert_resource(EffectLog(Vec::new()));

    let entity = world.spawn(
        EffectBundle {
            first: Effect(1),
            second: Effect(2),
        },
        None,
    );

    assert!(entity.is_spawned());
    assert_eq!(world.resource::<EffectLog>().0, vec![1, 2]);
}

#[test]
fn nested_effect_bundle_forwards_effects() {
    let mut world = World::alloc();
    world.insert_resource(EffectLog(Vec::new()));

    world.spawn(
        NestedEffectBundle {
            inner: EffectBundle {
                first: Effect(1),
                second: Effect(2),
            },
            tail: Effect(3),
        },
        None,
    );

    assert_eq!(world.resource::<EffectLog>().0, vec![1, 2, 3]);
}

// -----------------------------------------------------------------------------
// Inserting derived bundles into existing entities

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
