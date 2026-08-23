//! Integration tests for the `Query` module and its `SystemParam`
//! implementation.

use zlim_core::component::Component;
use zlim_core::query::{Query, Single, With};
use zlim_core::system::{IntoSystem, System};
use zlim_core::world::World;
use zlim_reflect::TypePath;

use serde::{Deserialize, Serialize};

#[derive(TypePath, Component, Clone, Serialize, Deserialize)]
struct Pos {
    x: f32,
    y: f32,
}

#[derive(TypePath, Component, Clone, Serialize, Deserialize)]
struct Vel {
    x: f32,
    y: f32,
}

fn sum_x(query: Query<&Pos>) -> f32 {
    query.iter().map(|pos| pos.x).sum()
}

fn sum_x_with_filter(query: Query<&Pos, With<Vel>>) -> f32 {
    query.iter().map(|pos| pos.x).sum()
}

fn count_mut(mut query: Query<&mut Pos>) -> usize {
    let mut count = 0;
    for pos in query.iter_mut() {
        pos.into_inner().x += 100.0;
        count += 1;
    }
    count
}

fn single_vel(query: Single<&Vel>) -> f32 {
    query.x
}

#[test]
fn query_iterates_components() {
    let mut world = World::alloc();
    world.spawn((Pos { x: 1.0, y: 0.0 }, Vel { x: 0.0, y: 0.0 }), None);
    world.spawn((Pos { x: 2.0, y: 0.0 },), None);

    let mut system = IntoSystem::into_system(sum_x);
    system.initialize(&world);

    assert_eq!(system.run((), &mut world).unwrap(), 3.0);
}

#[test]
fn query_applies_filters() {
    let mut world = World::alloc();
    world.spawn((Pos { x: 1.0, y: 0.0 }, Vel { x: 0.0, y: 0.0 }), None);
    world.spawn((Pos { x: 2.0, y: 0.0 },), None);

    let mut system = IntoSystem::into_system(sum_x_with_filter);
    system.initialize(&world);

    // Only the entity with `Vel` matches.
    assert_eq!(system.run((), &mut world).unwrap(), 1.0);
}

#[test]
fn query_iter_mut_modifies() {
    let mut world = World::alloc();
    let a = world.spawn((Pos { x: 1.0, y: 0.0 },), None).id();

    let mut system = IntoSystem::into_system(count_mut);
    system.initialize(&world);

    assert_eq!(system.run((), &mut world).unwrap(), 1);
    assert_eq!(world.entity(a).get::<Pos>().unwrap().x, 101.0);
}

#[test]
fn query_get_and_contains() {
    let mut world = World::alloc();
    let a = world.spawn((Pos { x: 1.0, y: 0.0 },), None).id();
    let b = world.spawn((Pos { x: 2.0, y: 0.0 },), None).id();

    let mut system = IntoSystem::into_system(move |query: Query<&Pos>| -> (f32, bool, bool) {
        let x = query.get(a).unwrap().x;
        let has_a = query.contains(a);
        let has_b = query.contains(b);
        (x, has_a, has_b)
    });
    system.initialize(&world);

    assert_eq!(system.run((), &mut world).unwrap(), (1.0, true, true));
}

#[test]
fn query_single_param() {
    let mut world = World::alloc();
    world.spawn((Pos { x: 5.0, y: 0.0 }, Vel { x: 1.0, y: 0.0 }), None);

    let mut system = IntoSystem::into_system(single_vel);
    system.initialize(&world);
    assert_eq!(system.run((), &mut world).unwrap(), 1.0);

    // Multiple matches fail parameter construction.
    world.spawn((Pos { x: 6.0, y: 0.0 }, Vel { x: 2.0, y: 0.0 }), None);
    let mut system = IntoSystem::into_system(single_vel);
    system.initialize(&world);
    assert!(system.run((), &mut world).is_err());

    // Zero matches fail too.
    let mut world = World::alloc();
    let mut system = IntoSystem::into_system(single_vel);
    system.initialize(&world);
    assert!(system.run((), &mut world).is_err());
}

// -----------------------------------------------------------------------------
