//! Integration tests for query filters: composition (`And` / `Or`),
//! tick-based filters (`Changed` / `Added`), and slice iteration over
//! archetype filters.

use zlim_core::component::Component;
use zlim_core::entity::EntityId;
use zlim_core::query::{Added, And, Changed, Or, Query, QueryState, With, Without};
use zlim_core::world::World;
use zlim_reflect::TypePath;

#[derive(TypePath, Component, Clone)]
struct A;

#[derive(TypePath, Component, Clone)]
struct B;

#[derive(TypePath, Component, Clone)]
struct C;

#[derive(TypePath, Component, Clone)]
struct Health(u32);

// -----------------------------------------------------------------------------
// And / Or composition

#[test]
fn and_combines_archetype_filters() {
    let mut world = World::alloc();
    world.spawn((A, B), None);
    world.spawn((A, B, C), None);
    world.spawn((A,), None);

    // `And<(With<A>, With<B>)>`: needs both components.
    assert_eq!(
        world.query::<(), And<(With<A>, With<B>)>>().iter().count(),
        2,
    );
    // `And<(With<A>, Without<C>)>`: has `A`, lacks `C`.
    assert_eq!(
        world
            .query::<(), And<(With<A>, Without<C>)>>()
            .iter()
            .count(),
        2,
    );
}

#[test]
fn or_combines_archetype_filters() {
    let mut world = World::alloc();
    world.spawn((Health(10), A), None);
    world.spawn((Health(20), B), None);
    world.spawn(Health(30), None);

    // `Or<(With<A>, With<B>)>`: matches either component.
    let total: u32 = world
        .query::<&Health, Or<(With<A>, With<B>)>>()
        .iter()
        .map(|h| h.0)
        .sum();
    assert_eq!(total, 30);
}

// -----------------------------------------------------------------------------
// Tick-based filters

#[test]
fn changed_filter_matches_only_modified() {
    let mut world = World::alloc();
    world.spawn((A, Health(10)), None);
    world.spawn((A, Health(20)), None);

    // Move the change-detection baseline past the spawn ticks so freshly
    // spawned components are no longer reported as changed.
    world.clear_trackers();

    // Modify the first entity's `Health` outside any system window.
    {
        let mut query = world.query_mut::<&mut Health, ()>();
        let first = query.iter_mut().next().unwrap();
        first.into_inner().0 += 1;
    }

    let changed = world
        .invoke_once(
            |query: Query<EntityId, Changed<Health>>| query.iter().count(),
            (),
        )
        .unwrap();
    assert_eq!(changed, 1);
}

#[test]
fn added_filter_matches_only_new_spawns() {
    let mut world = World::alloc();
    world.spawn((A, Health(10)), None);

    // Baseline past the first spawn, then spawn a new entity inside the
    // upcoming window.
    world.clear_trackers();
    world.spawn((A, Health(20)), None);

    let added = world
        .invoke_once(
            |query: Query<EntityId, Added<Health>>| query.iter().count(),
            (),
        )
        .unwrap();
    assert_eq!(added, 1);
}

// -----------------------------------------------------------------------------
// Composed tick filters (entity-level filtering path)

#[test]
fn and_with_tick_filter_filters_per_entity() {
    let mut world = World::alloc();
    world.spawn((A, Health(10)), None);
    world.spawn((A, Health(20)), None);

    world.clear_trackers();

    // Modify only the first entity's `Health`.
    {
        let mut query = world.query_mut::<&mut Health, With<A>>();
        let first = query.iter_mut().next().unwrap();
        first.into_inner().0 += 1;
    }

    // `And<(With<A>, Changed<Health>)>` requires entity-level filtering:
    // only the modified entity matches, despite both having `A`.
    let total: u32 = world
        .invoke_once(
            |query: Query<&Health, And<(With<A>, Changed<Health>)>>| {
                query.iter().map(|h| h.0).sum()
            },
            (),
        )
        .unwrap();
    assert_eq!(total, 11);
}

// -----------------------------------------------------------------------------
// Slice iteration over archetype filters

#[test]
fn iter_slice_accepts_and_archetype_filter() {
    let mut world = World::alloc();
    world.spawn((Health(10), A, B), None);
    world.spawn((Health(20), A), None);
    world.spawn((Health(30), B), None);

    // `And<(With<A>, With<B>)>` is a pure archetype filter, so the whole
    // component column can be fetched as one slice per table.
    let total: u32 = world
        .invoke_once(
            |query: Query<&Health, And<(With<A>, With<B>)>>| {
                query
                    .iter_slice()
                    .flat_map(|healths| healths.iter())
                    .map(|h| h.0)
                    .sum()
            },
            (),
        )
        .unwrap();
    assert_eq!(total, 10);
}

#[test]
fn iter_slice_mut_accepts_or_archetype_filter() {
    let mut world = World::alloc();
    world.spawn((Health(10), A), None);
    world.spawn((Health(20), B), None);
    world.spawn(Health(30), None);

    world
        .invoke_once(
            |mut query: Query<&mut Health, Or<(With<A>, With<B>)>>| {
                for mut healths in query.iter_slice_mut() {
                    for health in healths.iter_mut() {
                        health.0 += 100;
                    }
                }
            },
            (),
        )
        .unwrap();

    let total: u32 = world.query::<&Health, ()>().iter().map(|h| h.0).sum();
    assert_eq!(total, 10 + 100 + 20 + 100 + 30);
}

// -----------------------------------------------------------------------------
// QueryState incremental updates

#[test]
fn query_state_tracks_new_tables() {
    let mut world = World::alloc();
    world.spawn((A,), None);

    let mut state = QueryState::<&A>::build(&world);
    assert!(!state.should_update(&world));

    // Registering a new archetype adds a table, invalidating the state.
    world.spawn((A, B), None);
    assert!(state.should_update(&world));

    state.update(&world);
    assert!(!state.should_update(&world));
}
