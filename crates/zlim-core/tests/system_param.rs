//! Integration tests for `#[derive(SystemParam)]`.

use core::marker::PhantomData;

use zlim_core::borrow::{NonSend, Res, ResMut};
use zlim_core::command::Commands;
use zlim_core::derive::{Resource, SystemParam};
use zlim_core::resource::Resource as ResourceTrait;
use zlim_core::system::System;
use zlim_core::system::SystemFlags;
use zlim_core::system::SystemParam as SystemParamTrait;
use zlim_core::system::SystemTick;
use zlim_core::system::{AccessTable, ExclusiveMarker, IntoSystem, Local, NonSendMarker};
use zlim_core::world::{NonSendWorld, World};
use zlim_reflect::TypePath;

// -----------------------------------------------------------------------------
// Resources

#[derive(TypePath, Resource, Debug, PartialEq)]
struct Score(u32);

#[derive(TypePath, Resource, Debug, PartialEq)]
struct Delta(i32);

// -----------------------------------------------------------------------------
// Named struct param

#[derive(SystemParam)]
struct ScoreParam<'w, 's> {
    score: Res<'w, Score>,
    delta: ResMut<'w, Delta>,
    local: Local<'s, u32>,
}

#[test]
fn score_param_default_flags() {
    const { assert!(!<ScoreParam as SystemParamTrait>::DEFERRED) };
    const { assert!(!<ScoreParam as SystemParamTrait>::NON_SEND) };
    const { assert!(!<ScoreParam as SystemParamTrait>::EXCLUSIVE) };
}

#[test]
fn item_type_matches_struct_type() {
    // Compile-time proof that the derived `Item<'w, 's>` type is the struct
    // itself with its lifetimes rebound to `'w` / `'s`.
    fn same_type<'w, 's>(
        item: <ScoreParam as SystemParamTrait>::Item<'w, 's>,
    ) -> ScoreParam<'w, 's> {
        item
    }

    let _ = same_type;
}

fn score_system(mut param: ScoreParam) -> u32 {
    *param.local += 1;
    param.delta.0 += *param.local as i32;
    param.score.0 + *param.local
}

#[test]
fn score_param_runs_end_to_end() {
    let mut world = World::alloc();
    world.insert_resource(Score(10));
    world.insert_resource(Delta(0));

    // `invoke` caches the instance, so `Local` state persists between
    // runs.
    assert_eq!(world.invoke(score_system, ()).unwrap(), 11);
    assert_eq!(world.resource::<Delta>().0, 1);

    assert_eq!(world.invoke(score_system, ()).unwrap(), 12);
    assert_eq!(world.resource::<Delta>().0, 3);
}

#[test]
fn score_param_system_has_no_flags() {
    let system = IntoSystem::into_system(score_system);
    assert!(system.flags().is_empty());
}

// -----------------------------------------------------------------------------
// Deferred param (Commands)

#[derive(SystemParam)]
struct CommandsParam<'w, 's> {
    commands: Commands<'w, 's>,
}

#[test]
fn commands_param_flags() {
    const { assert!(<CommandsParam as SystemParamTrait>::DEFERRED) };
    const { assert!(!<CommandsParam as SystemParamTrait>::NON_SEND) };
    const { assert!(!<CommandsParam as SystemParamTrait>::EXCLUSIVE) };
}

fn commands_system(mut param: CommandsParam) {
    param.commands.insert_resource(Score(99));
}

#[test]
fn commands_param_applies_deferred_commands() {
    let mut world = World::alloc();

    let mut system = IntoSystem::into_system(commands_system);
    system.initialize(&world);
    assert!(system.flags().contains(SystemFlags::DEFERRED));

    world.invoke_once(commands_system, ()).unwrap();
    assert_eq!(world.resource::<Score>().0, 99);
}

// -----------------------------------------------------------------------------
// World access params

#[derive(SystemParam)]
struct WorldRefParam<'w, 's> {
    world: &'w World,
    _marker: PhantomData<(&'w (), &'s ())>,
}

fn world_ref_system(param: WorldRefParam) -> usize {
    param.world.entity_count()
}

#[test]
fn world_ref_param_sees_world_state() {
    let mut world = World::alloc();
    world.spawn((), None);
    world.spawn((), None);

    assert_eq!(world.invoke_once(world_ref_system, ()).unwrap(), 2);
}

#[derive(SystemParam)]
struct WorldMutParam<'w, 's> {
    world: &'w mut World,
    _marker: PhantomData<(&'w (), &'s ())>,
}

#[test]
fn world_mut_param_flags() {
    const { assert!(!<WorldMutParam as SystemParamTrait>::DEFERRED) };
    // `&mut World` is exclusive but `Send` — it may run on any thread.
    const { assert!(!<WorldMutParam as SystemParamTrait>::NON_SEND) };
    const { assert!(<WorldMutParam as SystemParamTrait>::EXCLUSIVE) };
}

fn world_mut_system(param: WorldMutParam) {
    param.world.clear_trackers();
}

#[test]
fn world_mut_param_runs() {
    let mut world = World::alloc();

    let mut system = IntoSystem::into_system(world_mut_system);
    system.initialize(&world);
    assert!(system.flags().contains(SystemFlags::EXCLUSIVE));

    world.invoke_once(world_mut_system, ()).unwrap();
}

// -----------------------------------------------------------------------------
// NonSendWorld access params

#[derive(TypePath, Resource)]
struct NonSendCounter(core::cell::Cell<u32>);

#[derive(SystemParam)]
struct NonSendWorldRefParam<'w, 's> {
    world: &'w NonSendWorld,
    _marker: PhantomData<(&'w (), &'s ())>,
}

#[test]
fn non_send_world_ref_param_flags() {
    const { assert!(!<NonSendWorldRefParam as SystemParamTrait>::DEFERRED) };
    const { assert!(<NonSendWorldRefParam as SystemParamTrait>::NON_SEND) };
    const { assert!(!<NonSendWorldRefParam as SystemParamTrait>::EXCLUSIVE) };
}

fn non_send_world_ref_system(param: NonSendWorldRefParam) -> u32 {
    param
        .world
        .get_non_send::<NonSendCounter>()
        .unwrap()
        .0
        .get()
}

#[test]
fn non_send_world_ref_param_runs() {
    let mut world = World::alloc();
    world.with_non_send_mut(|w| {
        w.insert_non_send(NonSendCounter(core::cell::Cell::new(9)));
    });

    let mut system = IntoSystem::into_system(non_send_world_ref_system);
    system.initialize(&world);
    assert!(system.flags().contains(SystemFlags::NON_SEND));
    assert!(!system.flags().contains(SystemFlags::EXCLUSIVE));

    assert_eq!(world.invoke_once(non_send_world_ref_system, ()).unwrap(), 9);
}

#[derive(SystemParam)]
struct NonSendWorldMutParam<'w, 's> {
    world: &'w mut NonSendWorld,
    _marker: PhantomData<(&'w (), &'s ())>,
}

#[test]
fn non_send_world_mut_param_flags() {
    const { assert!(!<NonSendWorldMutParam as SystemParamTrait>::DEFERRED) };
    const { assert!(<NonSendWorldMutParam as SystemParamTrait>::NON_SEND) };
    const { assert!(<NonSendWorldMutParam as SystemParamTrait>::EXCLUSIVE) };
}

fn non_send_world_mut_system(param: NonSendWorldMutParam) {
    param
        .world
        .insert_non_send(NonSendCounter(core::cell::Cell::new(3)));
    param
        .world
        .get_non_send_mut::<NonSendCounter>()
        .unwrap()
        .0
        .set(7);
}

#[test]
fn non_send_world_mut_param_runs() {
    let mut world = World::alloc();

    let mut system = IntoSystem::into_system(non_send_world_mut_system);
    system.initialize(&world);
    assert!(system.flags().contains(SystemFlags::NON_SEND));

    world.invoke_once(non_send_world_mut_system, ()).unwrap();
    world.with_non_send(|w| {
        assert_eq!(w.get_non_send::<NonSendCounter>().unwrap().0.get(), 7);
    });
}

// -----------------------------------------------------------------------------
// Tick param

#[derive(SystemParam)]
struct TickParam<'w, 's> {
    ticks: SystemTick,
    _marker: PhantomData<(&'w (), &'s ())>,
}

fn tick_system(param: TickParam) -> (u32, u32) {
    (param.ticks.last_run.get(), param.ticks.this_run.get())
}

#[test]
fn tick_param_tracks_runs() {
    let mut world = World::alloc();

    // The cached instance keeps its `last_run` baseline, so the second run
    // observes the window advanced by the first.
    assert_eq!(world.invoke(tick_system, ()).unwrap(), (0, 1));
    assert_eq!(world.invoke(tick_system, ()).unwrap(), (1, 2));
}

// -----------------------------------------------------------------------------
// Optional param

#[derive(SystemParam)]
struct OptionParam<'w, 's> {
    score: Option<Res<'w, Score>>,
    _marker: PhantomData<&'s ()>,
}

fn option_system(param: OptionParam) -> Option<u32> {
    param.score.map(|score| score.0)
}

#[test]
fn option_param_handles_missing_resource() {
    let mut world = World::alloc();

    assert_eq!(world.invoke_once(option_system, ()).unwrap(), None);

    world.insert_resource(Score(5));
    assert_eq!(world.invoke_once(option_system, ()).unwrap(), Some(5));
}

// -----------------------------------------------------------------------------
// Tuple struct param

#[derive(SystemParam)]
struct TupleParam<'w, 's>(Res<'w, Score>, Local<'s, u32>);

fn tuple_system(param: TupleParam) -> u32 {
    param.0.0 + *param.1
}

#[test]
fn tuple_param_runs() {
    let mut world = World::alloc();
    world.insert_resource(Score(41));

    assert_eq!(world.invoke_once(tuple_system, ()).unwrap(), 41);
}

// -----------------------------------------------------------------------------
// Unit struct param

#[derive(SystemParam)]
struct UnitParam<'w, 's>(PhantomData<(&'w (), &'s ())>);

fn unit_system(_: UnitParam) -> u8 {
    7
}

#[test]
fn unit_param_runs() {
    let mut world = World::alloc();

    assert_eq!(world.invoke_once(unit_system, ()).unwrap(), 7);
}

// -----------------------------------------------------------------------------
// Generic param

#[derive(SystemParam)]
struct GenericParam<'w, 's, T: ResourceTrait + Sync> {
    value: Res<'w, T>,
    _marker: PhantomData<&'s ()>,
}

#[test]
fn generic_param_item_type_matches() {
    fn same_type<'w, 's>(
        item: <GenericParam<'w, 's, Score> as SystemParamTrait>::Item<'w, 's>,
    ) -> GenericParam<'w, 's, Score> {
        item
    }

    let _ = same_type;
}

fn generic_system(param: GenericParam<'_, '_, Score>) -> u32 {
    param.value.0
}

#[test]
fn generic_param_runs() {
    let mut world = World::alloc();
    world.insert_resource(Score(77));

    assert_eq!(world.invoke_once(generic_system, ()).unwrap(), 77);
}

// -----------------------------------------------------------------------------
// Flag propagation

#[derive(SystemParam)]
struct MarkerParam<'w, 's> {
    _non_send: NonSendMarker,
    _exclusive: ExclusiveMarker,
    _marker: PhantomData<(&'w (), &'s ())>,
}

#[test]
fn marker_param_propagates_flags() {
    const { assert!(!<MarkerParam as SystemParamTrait>::DEFERRED) };
    const { assert!(<MarkerParam as SystemParamTrait>::NON_SEND) };
    const { assert!(<MarkerParam as SystemParamTrait>::EXCLUSIVE) };
}

#[derive(SystemParam)]
struct NonSendParam<'w, 's> {
    _value: NonSend<'w, Score>,
    _marker: PhantomData<&'s ()>,
}

#[test]
fn non_send_param_propagates_flags() {
    const { assert!(!<NonSendParam as SystemParamTrait>::DEFERRED) };
    const { assert!(<NonSendParam as SystemParamTrait>::NON_SEND) };
    const { assert!(!<NonSendParam as SystemParamTrait>::EXCLUSIVE) };
}

// -----------------------------------------------------------------------------
// Access registration

#[derive(SystemParam)]
struct ReadScoreParam<'w, 's> {
    _score: Res<'w, Score>,
    _marker: PhantomData<&'s ()>,
}

#[derive(SystemParam)]
struct WriteScoreParam<'w, 's> {
    _score: ResMut<'w, Score>,
    _marker: PhantomData<&'s ()>,
}

#[derive(SystemParam)]
struct WriteDeltaParam<'w, 's> {
    _delta: ResMut<'w, Delta>,
    _marker: PhantomData<&'s ()>,
}

fn access_of<P: SystemParamTrait>(world: &World) -> AccessTable {
    let state = <P as SystemParamTrait>::init_state(world);
    let mut table = AccessTable::new();
    let ok = <P as SystemParamTrait>::register_access(&state, &mut table, true);
    assert!(ok);
    table
}

#[test]
fn disjoint_params_are_parallelizable() {
    let mut world = World::alloc();
    world.insert_resource(Score(0));
    world.insert_resource(Delta(0));

    let write_score = access_of::<WriteScoreParam>(&world);
    let write_delta = access_of::<WriteDeltaParam>(&world);

    assert!(write_score.parallelizable(&write_delta));
}

#[test]
fn conflicting_params_are_not_parallelizable() {
    let mut world = World::alloc();
    world.insert_resource(Score(0));

    let write_score = access_of::<WriteScoreParam>(&world);
    let read_score = access_of::<ReadScoreParam>(&world);

    assert!(!write_score.parallelizable(&read_score));
}

#[derive(SystemParam)]
struct DoubleMutParam<'w, 's> {
    _a: ResMut<'w, Score>,
    _b: ResMut<'w, Score>,
    _marker: PhantomData<&'s ()>,
}

#[test]
fn field_conflict_is_reported_but_merged() {
    let mut world = World::alloc();
    world.insert_resource(Score(0));

    let state = <DoubleMutParam as SystemParamTrait>::init_state(&world);
    let mut table = AccessTable::new();

    assert!(!<DoubleMutParam as SystemParamTrait>::register_access(
        &state, &mut table, true,
    ));
}
