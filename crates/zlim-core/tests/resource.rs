//! Integration tests for `#[derive(Resource)]` and the World resource API
//! (`ops/resource.rs`).

use zlim_core::derive::Resource;
use zlim_core::resource::Resource as ResourceTrait;
use zlim_core::resource::ResourceDB;
use zlim_core::tick::DetectChanges;
use zlim_core::world::World;
use zlim_reflect::TypePath;

use serde::{Deserialize, Serialize};

// -----------------------------------------------------------------------------
// Test types
// -----------------------------------------------------------------------------

#[derive(TypePath, Resource, Debug, PartialEq, Eq)]
struct Health {
    value: u32,
}

#[derive(TypePath, Resource, Debug, PartialEq, Eq)]
struct Score {
    points: u64,
}

#[derive(TypePath, Resource)]
struct GenericRes<T: Send + Sync + 'static> {
    _data: T,
}

// -----------------------------------------------------------------------------
// Derive macro tests
// -----------------------------------------------------------------------------

/// A resource registered with serialization support.
#[derive(TypePath, Resource, Serialize, Deserialize)]
#[resource(serialize)]
struct SerializeRes(u32);

#[test]
fn derive_serialize_flag() {
    let db = ResourceDB::of::<SerializeRes>();
    assert!(db.serialize.is_some());
    assert!(db.deserialize.is_some());
    const {
        assert!(SerializeRes::SERIALIZE);
    }
    // The default is `false` for unmarked resources.
    const {
        assert!(!Health::SERIALIZE);
    }
}

#[test]
fn derive_generic_compiles() {
    // Prove that the generic impl exists with the correct bounds.
    fn _assert_resource<T: ResourceTrait>() {}
    _assert_resource::<GenericRes<i32>>();
}

// -----------------------------------------------------------------------------
// World resource API tests — Send resources
// -----------------------------------------------------------------------------

#[test]
fn insert_and_get() {
    let mut world = World::alloc();

    assert_eq!(
        *world.insert_resource(Health { value: 10 }),
        Health { value: 10 }
    );
    assert!(world.contains_resource::<Health>());
    assert_eq!(world.get_resource::<Health>(), Some(&Health { value: 10 }));
    assert_eq!(world.resource::<Health>(), &Health { value: 10 });
}

#[test]
fn insert_replaces_existing() {
    let mut world = World::alloc();
    world.insert_resource(Health { value: 1 });
    world.insert_resource(Health { value: 2 });
    assert_eq!(world.resource::<Health>().value, 2);
}

#[test]
fn remove_resource() {
    let mut world = World::alloc();
    world.insert_resource(Score { points: 100 });

    let removed = world.remove_resource::<Score>();
    assert_eq!(removed, Some(Score { points: 100 }));
    assert!(!world.contains_resource::<Score>());
    assert_eq!(world.get_resource::<Score>(), None);
}

#[test]
fn remove_nonexistent() {
    let mut world = World::alloc();
    assert_eq!(world.remove_resource::<Health>(), None);
}

#[test]
fn drop_resource() {
    let mut world = World::alloc();
    world.insert_resource(Health { value: 99 });
    world.drop_resource::<Health>();
    assert!(!world.contains_resource::<Health>());
}

// -----------------------------------------------------------------------------
// World resource API tests — Change detection
// -----------------------------------------------------------------------------

#[test]
fn resource_ref_change_detection() {
    let mut world = World::alloc();
    world.insert_resource(Health { value: 10 });

    let r = world.resource_ref::<Health>();
    assert!(r.is_added());
    assert!(r.is_changed());
    assert_eq!(r.value, 10);

    world.clear_trackers();
    let r2 = world.get_resource_ref::<Health>().unwrap();
    assert!(!r2.is_added());
    assert!(!r2.is_changed());
}

#[test]
fn resource_mut_change_detection() {
    let mut world = World::alloc();
    world.insert_resource(Health { value: 1 });

    let mut r = world.resource_mut::<Health>();
    assert!(r.is_changed());
    assert!(r.is_added());
    assert_eq!(r.value, 1);

    r.value = 111;
    assert_eq!(r.value, 111);

    world.clear_trackers();
    let mut r = world.resource_mut::<Health>();
    assert!(!r.is_changed());
    assert!(!r.is_added());
    let x: u32 = (*r).value;
    assert_eq!(x, 111);

    assert!(!r.is_changed());
    assert!(!r.is_added());

    r.value = 2233;
    assert!(r.is_changed());
    assert!(!r.is_added());
}

// -----------------------------------------------------------------------------
// World resource API tests — NonSend resources
// -----------------------------------------------------------------------------

#[test]
fn non_send_insert_and_get() {
    let mut world = World::alloc();

    world.with_non_send_mut(|w| {
        w.insert_non_send(Health { value: 7 });
    });
    world.with_non_send(|w| {
        assert!(w.contains_non_send::<Health>());
        assert_eq!(w.get_non_send::<Health>(), Some(&Health { value: 7 }));
        assert_eq!(w.non_send::<Health>(), &Health { value: 7 });
    });
}

#[test]
fn non_send_remove_and_drop() {
    let mut world = World::alloc();

    world.with_non_send_mut(|w| {
        w.insert_non_send(Score { points: 200 });
    });
    assert_eq!(
        world.with_non_send_mut(|w| w.remove_non_send::<Score>()),
        Some(Score { points: 200 })
    );
    assert!(!world.with_non_send(|w| w.contains_non_send::<Score>()));

    world.with_non_send_mut(|w| {
        w.insert_non_send(Score { points: 300 });
    });
    world.with_non_send_mut(|w| {
        w.drop_non_send::<Score>();
    });
    assert!(!world.with_non_send(|w| w.contains_non_send::<Score>()));
}

#[test]
fn non_send_ref_change_detection() {
    let mut world = World::alloc();
    world.with_non_send_mut(|w| {
        w.insert_non_send(Health { value: 3 });
    });

    world.with_non_send(|w| {
        let r = w.non_send_ref::<Health>();
        assert!(r.is_added());
        assert!(r.is_changed());
        assert_eq!(r.value, 3);
    });
}

#[test]
fn non_send_mut_change_detection() {
    let mut world = World::alloc();
    world.with_non_send_mut(|w| {
        w.insert_non_send(Health { value: 5 });
    });

    world.with_non_send_mut(|w| {
        let mut r = w.non_send_mut::<Health>();
        r.value = 55;
        assert_eq!(r.value, 55);
    });
}

// -----------------------------------------------------------------------------
// World resource API tests — Send / NonSend shared slot
// -----------------------------------------------------------------------------

#[test]
fn send_and_non_send_share_slot() {
    let mut world = World::alloc();

    world.insert_resource(Health { value: 10 });
    world.with_non_send(|w| {
        assert_eq!(w.get_non_send::<Health>(), Some(&Health { value: 10 }));
    });

    world.with_non_send_mut(|w| {
        w.insert_non_send(Health { value: 20 });
    });
    assert_eq!(world.get_resource::<Health>(), Some(&Health { value: 20 }));
}

#[test]
fn contains_resource_false_for_uninserted() {
    let world = World::alloc();
    assert!(!world.contains_resource::<Health>());
    assert!(!world.with_non_send(|w| w.contains_non_send::<Health>()));
}

#[test]
fn multiple_resource_types() {
    let mut world = World::alloc();

    world.insert_resource(Health { value: 1 });
    world.insert_resource(Score { points: 2 });

    assert!(world.contains_resource::<Health>());
    assert!(world.contains_resource::<Score>());
    assert_eq!(world.resource::<Health>().value, 1);
    assert_eq!(world.resource::<Score>().points, 2);
}
