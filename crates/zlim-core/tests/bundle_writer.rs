//! Integration tests for the dynamic insertion path: `BundleScratch` and the
//! `BundleWriter` it hands out.

use zlim_core::bundle::{Bundle, BundleScratch};
use zlim_core::derive::Component;
use zlim_core::entity::EntityError;
use zlim_core::tick::DetectChanges;
use zlim_core::world::World;
use zlim_path::TypePath;

// -----------------------------------------------------------------------------
// Components

#[derive(TypePath, Component, Clone, Debug, PartialEq)]
struct Hp(u32);

#[derive(TypePath, Component, Clone, Debug, PartialEq)]
struct Armor(u32);

/// The innermost required component. Its `Default` is deliberately not what
/// zeroed memory looks like, so a column the writer never filled in is easy to
/// tell apart from one that was default-initialised.
#[derive(TypePath, Component, Clone, Debug, PartialEq)]
struct Inner(u32);

impl Default for Inner {
    fn default() -> Self {
        Inner(0x1111_1111)
    }
}

/// Requires [`Inner`] directly, and is itself required by [`TwoLevels`].
#[derive(TypePath, Component, Clone, Debug, PartialEq)]
#[require(Inner)]
struct Outer(u32);

impl Default for Outer {
    fn default() -> Self {
        Outer(0x2222_2222)
    }
}

/// Requires only [`Outer`]: [`Inner`] is reachable through the chain.
#[derive(TypePath, Component, Clone, Debug, PartialEq)]
#[require(Outer)]
struct TwoLevels(u32);

// -----------------------------------------------------------------------------
// Writing pushed components

/// Pushed components reach the entity in a single write: the one the entity
/// already has is overwritten, the one it lacks is added, and the scratch space
/// is empty and reusable afterwards.
#[test]
fn writer_inserts_pushed_components() {
    let mut world = World::alloc();
    let mut entity = world.spawn(Hp(100), None);

    let mut scratch = BundleScratch::default();

    let mut writer = scratch.writer();
    writer.push(Hp(50), None);
    writer.push(Armor(25), None);
    writer.write(&mut entity).unwrap();

    assert_eq!(entity.get::<Hp>(), Some(&Hp(50)));
    assert_eq!(entity.get::<Armor>(), Some(&Armor(25)));
    assert!(scratch.is_empty());

    // The same scratch space serves the next write.
    let mut writer = scratch.writer();
    writer.push(Armor(10), None);
    writer.write(&mut entity).unwrap();

    assert_eq!(entity.get::<Hp>(), Some(&Hp(50)));
    assert_eq!(entity.get::<Armor>(), Some(&Armor(10)));
    assert!(scratch.is_empty());
}

/// Component types can be looked up in the target world's registry instead of
/// the process-global one; the pushed component has to end up in the same place
/// either way.
#[test]
fn writer_accepts_a_world_registry() {
    let mut world = World::alloc();
    let mut entity = world.spawn((), None);

    let mut scratch = BundleScratch::default();
    let mut writer = scratch.writer();
    writer.push(Hp(5), Some(entity.world().components()));
    writer.write(&mut entity).unwrap();

    assert_eq!(entity.get::<Hp>(), Some(&Hp(5)));
}

/// Pushing the same component type twice is allowed, and the last value is the
/// one that survives: the earlier value is replaced like any other overwritten
/// component.
#[test]
fn writer_keeps_the_last_push_of_a_component() {
    let mut world = World::alloc();
    let mut entity = world.spawn((), None);

    let mut scratch = BundleScratch::default();
    let mut writer = scratch.writer();
    writer.push(Hp(1), None);
    writer.push(Hp(2), None);
    writer.write(&mut entity).unwrap();

    assert_eq!(entity.get::<Hp>(), Some(&Hp(2)));
}

// -----------------------------------------------------------------------------
// Required components

/// A pushed component's required components are collected into the same bundle,
/// which makes them new columns of the target table. Nothing else initialises
/// those columns, so the writer has to fill them in from their `Default` — and
/// stamp them with the current tick like any other added component.
#[test]
fn writer_initialises_required_components() {
    let mut world = World::alloc();
    // Move away from tick zero, so a column left untouched cannot pass this
    // test by reading as zero by accident.
    world.clear_trackers();

    let mut entity = world.spawn((), None);

    let mut scratch = BundleScratch::default();
    let mut writer = scratch.writer();
    writer.push(Outer(7), None);
    writer.write(&mut entity).unwrap();

    assert!(entity.contains::<Inner>());
    assert_eq!(entity.get::<Outer>(), Some(&Outer(7)));
    assert_eq!(entity.get::<Inner>(), Some(&Inner::default()));

    let inner = entity.get_ref::<Inner>().expect("Inner was just added");
    assert_eq!(inner.changed_tick(), entity.world().this_run());
    assert!(inner.is_changed());
}

/// Required components are followed recursively, so a component that is only
/// reachable through another required component is initialised as well.
#[test]
fn writer_initialises_transitive_required_components() {
    let mut world = World::alloc();
    let mut entity = world.spawn((), None);

    let mut scratch = BundleScratch::default();
    let mut writer = scratch.writer();
    writer.push(TwoLevels(7), None);
    writer.write(&mut entity).unwrap();

    assert_eq!(entity.get::<TwoLevels>(), Some(&TwoLevels(7)));
    assert_eq!(entity.get::<Outer>(), Some(&Outer::default()));
    assert_eq!(entity.get::<Inner>(), Some(&Inner::default()));
}

/// Required components are only supplied when the entity does not have them
/// yet: an existing value is not reset to its `Default`.
#[test]
fn writer_keeps_existing_required_components() {
    let mut world = World::alloc();
    let mut entity = world.spawn(Inner(42), None);

    let mut scratch = BundleScratch::default();
    let mut writer = scratch.writer();
    writer.push(Outer(7), None);
    writer.write(&mut entity).unwrap();

    assert_eq!(entity.get::<Outer>(), Some(&Outer(7)));
    assert_eq!(entity.get::<Inner>(), Some(&Inner(42)));
}

// -----------------------------------------------------------------------------
// Pushing bundles

/// A derived bundle is walked with `push_to`: every field it carries reaches the
/// writer, in declaration order, and the required components of those fields are
/// initialised by the write.
#[test]
fn writer_pushes_a_derived_bundle() {
    #[derive(Bundle)]
    struct Gear {
        hp: Hp,
        levels: TwoLevels,
    }

    let mut world = World::alloc();
    let mut entity = world.spawn((), None);
    let mut scratch = BundleScratch::default();

    let mut writer = scratch.writer();
    writer.push(
        Gear {
            hp: Hp(7),
            levels: TwoLevels(1),
        },
        Some(entity.world().components()),
    );
    writer.write(&mut entity).unwrap();

    assert_eq!(entity.get::<Hp>(), Some(&Hp(7)));
    assert_eq!(entity.get::<TwoLevels>(), Some(&TwoLevels(1)));
    assert_eq!(entity.get::<Outer>(), Some(&Outer::default()));
    assert_eq!(entity.get::<Inner>(), Some(&Inner::default()));
    assert!(scratch.is_empty());
}

/// A tuple is walked in declaration order as well, so a component it carries
/// twice keeps the value that was pushed last.
#[test]
fn writer_pushes_a_tuple_bundle() {
    let mut world = World::alloc();
    let mut entity = world.spawn((), None);
    let mut scratch = BundleScratch::default();

    let mut writer = scratch.writer();
    writer.push((Hp(1), Armor(2), Hp(3)), None);
    writer.write(&mut entity).unwrap();

    assert_eq!(entity.get::<Hp>(), Some(&Hp(3)));
    assert_eq!(entity.get::<Armor>(), Some(&Armor(2)));
    assert!(scratch.is_empty());
}

// -----------------------------------------------------------------------------
// Error handling

/// Writing to an entity that is no longer spawned fails before anything is
/// written, and discards what was pushed instead of leaving it for a later
/// write to pick up.
#[test]
fn writer_rejects_an_unspawned_entity() {
    let mut world = World::alloc();
    let mut entity = world.spawn((), None);
    let id = entity.id();
    entity.world_scope(|world| world.despawn(id).unwrap());

    let mut scratch = BundleScratch::default();
    let mut writer = scratch.writer();
    writer.push(Hp(1), None);

    assert!(matches!(
        writer.write(&mut entity),
        Err(EntityError::NotSpawned(_))
    ));
    assert!(scratch.is_empty());
}
