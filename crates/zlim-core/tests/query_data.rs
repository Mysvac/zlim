//! Integration tests for the `#[derive(QueryData)]` macro.

use zlim_core::borrow::Mut;
use zlim_core::component::Component;
use zlim_core::derive::QueryData;
use zlim_core::query::{Query, Single};
use zlim_core::system::{IntoSystem, System};
use zlim_core::world::World;
use zlim_reflect::TypePath;

use serde::{Deserialize, Serialize};

#[derive(TypePath, Component, Clone, Serialize, Deserialize)]
struct Score(u32);

#[derive(TypePath, Component, Clone, Serialize, Deserialize)]
struct Name(String);

#[derive(TypePath, Component, Clone, Serialize, Deserialize)]
struct Tag;

// -----------------------------------------------------------------------------
// Derived query data types

/// Read-only derived data — `#[query_data(readonly)]`.
#[derive(QueryData)]
#[query_data(readonly)]
#[query_slice(type = ReadPlayerSlice)]
struct ReadPlayer<'w> {
    name: &'w Name,
    score: &'w Score,
}

/// Mutable derived data — a companion `PlayerReadOnly` struct is generated.
#[derive(QueryData)]
#[query_slice(type = PlayerSlice)]
struct Player<'w> {
    score: Mut<'w, Score>,
    name: &'w Name,
}

/// Tuple struct.
#[derive(QueryData)]
#[query_data(readonly)]
struct PlayerPair<'w>(&'w Name, &'w Score);

/// Unit struct — matches every entity.
#[derive(QueryData)]
struct UnitData;

/// Generic derived data.
#[derive(QueryData)]
#[query_data(readonly)]
#[query_slice(type = GenericSlice)]
struct Generic<'w, T: Component> {
    value: &'w T,
}

/// Generic derived data with a mutable field.
#[derive(QueryData)]
#[query_slice(type = GenericMutSlice)]
struct GenericMut<'w, T: Component> {
    value: Mut<'w, T>,
}

// -----------------------------------------------------------------------------
// Readonly derive

fn sum_readonly(query: Query<ReadPlayer>) -> u32 {
    // `ReadPlayer` is `ReadOnlyQueryData`, so `Query` is `Copy` — use it twice.
    let copy = query;
    let via_copy: u32 = copy.iter().map(|p| p.score.0).sum();
    let via_query: u32 = query.iter().map(|p| p.score.0).sum();
    via_copy + via_query
}

#[test]
fn derived_readonly_iterates() {
    let mut world = World::alloc();
    world.spawn((Score(1), Name("a".into()), Tag), None);
    world.spawn((Score(2), Name("b".into())), None);

    let mut system = IntoSystem::into_system(sum_readonly);
    system.initialize(&world);
    assert_eq!(system.run((), &mut world).unwrap(), 6);
}

// -----------------------------------------------------------------------------
// Mutable derive + companion ReadOnly struct

fn modify_players(mut query: Query<Player>) -> (u32, u32) {
    let mut total = 0;
    let mut name_chars = 0;
    for player in query.iter_mut() {
        let score = player.score.into_inner();
        score.0 += 10;
        total += score.0;
        name_chars += player.name.0.len() as u32;
    }
    (total, name_chars)
}

fn read_through_companion(query: Query<Player>) -> (u32, u32) {
    // `PlayerReadOnly` is the generated companion: `score` becomes `Ref`.
    let mut via_ro = 0;
    let mut names = 0;
    for player in query.as_readonly().iter() {
        via_ro += player.score.into_inner().0;
        names += player.name.0.len() as u32;
    }
    (via_ro, names)
}

#[test]
fn derived_mut_modifies_and_readonly_view() {
    let mut world = World::alloc();
    world.spawn((Score(1), Name("a".into())), None);
    world.spawn((Score(2), Name("b".into())), None);

    let mut system = IntoSystem::into_system(modify_players);
    system.initialize(&world);
    assert_eq!(system.run((), &mut world).unwrap(), (23, 2));

    let mut system = IntoSystem::into_system(read_through_companion);
    system.initialize(&world);
    assert_eq!(system.run((), &mut world).unwrap(), (23, 2));
}

// -----------------------------------------------------------------------------
// Tuple & unit derives

#[test]
fn derived_tuple_and_unit() {
    let mut world = World::alloc();
    world.spawn((Score(1), Name("a".into())), None);
    world.spawn((Score(2), Name("b".into())), None);
    world.spawn((Score(3), Name("c".into())), None);

    let s = |query: Query<PlayerPair>, units: Query<UnitData>| {
        let pair_sum: u32 = query.iter().map(|p| p.0.0.len() as u32 + p.1.0).sum();
        (pair_sum, query.iter().count() as u32, units.iter().count())
    };

    let mut system = IntoSystem::into_system(s);
    system.initialize(&world);
    // name lengths 1+1+1 plus scores 1+2+3.
    assert_eq!(system.run((), &mut world).unwrap(), (9, 3, 3));
}

// -----------------------------------------------------------------------------
// Generic derive

fn sum_generic(query: Query<Generic<Score>>) -> u32 {
    query.iter().map(|g| g.value.0).sum()
}

fn bump_generic(mut query: Query<GenericMut<Score>>) -> u32 {
    let mut total = 0;
    for g in query.iter_mut() {
        let score = g.value.into_inner();
        score.0 += 5;
        total += score.0;
    }
    total
}

#[test]
fn derived_generic() {
    let mut world = World::alloc();
    world.spawn((Score(1),), None);
    world.spawn((Score(2),), None);

    let mut system = IntoSystem::into_system(sum_generic);
    system.initialize(&world);
    assert_eq!(system.run((), &mut world).unwrap(), 3);

    let mut system = IntoSystem::into_system(bump_generic);
    system.initialize(&world);
    assert_eq!(system.run((), &mut world).unwrap(), 13);
}

// -----------------------------------------------------------------------------
// Single with derived data

#[test]
fn derived_single() {
    let mut world = World::alloc();
    world.spawn((Score(7), Name("solo".into())), None);

    let s = |query: Single<ReadPlayer>| (query.score.0, query.name.0.len() as u32);

    let mut system = IntoSystem::into_system(s);
    system.initialize(&world);
    assert_eq!(system.run((), &mut world).unwrap(), (7, 4));
}

// -----------------------------------------------------------------------------
// QuerySlice derive (`#[query_data(query_slice(type = Name))]`)

fn sum_slice(query: Query<Generic<Score>>) -> u32 {
    // `GenericSlice<'w, Score>` is the generated companion:
    // `value: &'w [Score]`.
    query
        .iter_slice()
        .map(|g| g.value.iter().map(|s| s.0).sum::<u32>())
        .sum()
}

fn sum_slice_readonly(query: Query<ReadPlayer>) -> u32 {
    // `ReadPlayerSlice<'w>` is generated: `score: &'w [Score]`.
    query
        .iter_slice()
        .map(|p| p.score.iter().map(|s| s.0).sum::<u32>())
        .sum()
}

fn bump_slice_mut(mut query: Query<Player>) -> u32 {
    // `PlayerSlice<'w>` is generated with `score: SliceMut<'w, Score>`.
    let mut total = 0;
    for mut player in query.iter_slice_mut() {
        for score in player.score.iter_mut() {
            score.0 += 100;
        }
        total += player.score.iter().map(|s| s.0).sum::<u32>();
        total += player.name.len() as u32;
    }
    total
}

fn read_slice_through_readonly(query: Query<Player>) -> (u32, u32) {
    // The readonly slice companion `PlayerSliceReadOnly<'w>` has
    // `score: SliceRef<'w, Score>`.
    let mut scores = 0;
    let mut names = 0;
    for player in query.iter_slice() {
        scores += player.score.iter().map(|s| s.0).sum::<u32>();
        names += player.name.len() as u32;
    }
    (scores, names)
}

#[test]
fn derived_query_slice() {
    let mut world = World::alloc();
    world.spawn((Score(1), Name("a".into())), None);
    world.spawn((Score(2), Name("b".into())), None);
    world.spawn((Score(3), Name("c".into())), None);

    // readonly, generic
    let mut system = IntoSystem::into_system(sum_slice);
    system.initialize(&world);
    assert_eq!(system.run((), &mut world).unwrap(), 6);

    // readonly, concrete
    let mut system = IntoSystem::into_system(sum_slice_readonly);
    system.initialize(&world);
    assert_eq!(system.run((), &mut world).unwrap(), 6);

    // mutable slice + readonly view of it
    let mut system = IntoSystem::into_system(bump_slice_mut);
    system.initialize(&world);
    // scores 1+2+3 + 300, plus name lengths 1+1+1.
    assert_eq!(system.run((), &mut world).unwrap(), 309);

    let mut system = IntoSystem::into_system(read_slice_through_readonly);
    system.initialize(&world);
    assert_eq!(system.run((), &mut world).unwrap(), (306, 3));
}

// -----------------------------------------------------------------------------
