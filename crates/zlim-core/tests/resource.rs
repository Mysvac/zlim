//! Integration tests for `#[derive(Resource)]` and the World resource API
//! (`ops/resource.rs`).

use zlim_core::derive::Resource;
use zlim_core::resource::Resource as ResourceTrait;
use zlim_core::tick::DetectChanges;
use zlim_core::world::World;
use zlim_reflect::TypePath;

// -----------------------------------------------------------------------------
// Test types
// -----------------------------------------------------------------------------

#[derive(TypePath, Resource, Debug, PartialEq, Eq)]
struct Health {
    #[editor(mutable)]
    value: u32,
}

#[derive(TypePath, Resource, Debug, PartialEq, Eq)]
struct Score {
    #[editor(readonly)]
    points: u64,
}

#[derive(TypePath, Resource)]
struct Mixed {
    #[editor(mutable)]
    hp: u32,
    #[editor(readonly)]
    id: u64,
    _hidden: String,
}

#[derive(TypePath, Resource)]
struct NoEditor {
    _x: i32,
    _y: i32,
}

#[derive(TypePath, Resource)]
struct TupleRes(
    #[editor(mutable)] f32,
    #[editor(mutable)] f32,
    #[editor(readonly)] f32,
);

#[derive(TypePath, Resource)]
struct GenericRes<T: Send + Sync + 'static> {
    #[editor(mutable)]
    data: T,
}

// -----------------------------------------------------------------------------
// Derive macro tests
// -----------------------------------------------------------------------------

#[test]
fn derive_fields_const() {
    assert_eq!(Mixed::FIELDS, &["hp", "id"]);
    assert_eq!(Mixed::MUTABLE_FIELDS, &["hp"]);
    assert_eq!(Mixed::READONLY_FIELDS, &["id"]);
}

#[test]
fn derive_no_editor_defaults() {
    assert_eq!(NoEditor::FIELDS.len(), 0);
    assert_eq!(NoEditor::MUTABLE_FIELDS.len(), 0);
    assert_eq!(NoEditor::READONLY_FIELDS.len(), 0);
}

#[test]
fn derive_field_access_mutable() {
    let m = Mixed {
        hp: 42,
        id: 7,
        _hidden: "x".into(),
    };
    let f = m.field("hp").unwrap();
    assert_eq!(*f.downcast_ref::<u32>().unwrap(), 42);
}

#[test]
fn derive_field_access_readonly() {
    let m = Mixed {
        hp: 42,
        id: 7,
        _hidden: "x".into(),
    };
    let f = m.field("id").unwrap();
    assert_eq!(*f.downcast_ref::<u64>().unwrap(), 7);
}

#[test]
fn derive_field_unmarked_not_exposed() {
    let m = Mixed {
        hp: 0,
        id: 0,
        _hidden: "secret".into(),
    };
    assert!(m.field("_hidden").is_none());
    assert!(m.field("nonexistent").is_none());
}

#[test]
fn derive_field_mut_only_mutable() {
    let mut m = Mixed {
        hp: 10,
        id: 3,
        _hidden: "a".into(),
    };
    {
        let f = m.field_mut("hp").unwrap();
        *f.downcast_mut::<u32>().unwrap() = 99;
    }
    assert_eq!(m.hp, 99);

    // readonly field not accessible via field_mut
    assert!(m.field_mut("id").is_none());
    assert!(m.field_mut("_hidden").is_none());
}

#[test]
fn derive_tuple_struct() {
    assert_eq!(TupleRes::FIELDS, &["0", "1", "2"]);
    assert_eq!(TupleRes::MUTABLE_FIELDS, &["0", "1"]);
    assert_eq!(TupleRes::READONLY_FIELDS, &["2"]);

    let mut t = TupleRes(1.0, 2.0, 3.0);
    assert_eq!(*t.field("0").unwrap().downcast_ref::<f32>().unwrap(), 1.0);
    assert_eq!(*t.field("2").unwrap().downcast_ref::<f32>().unwrap(), 3.0);
    assert!(t.field_mut("2").is_none());
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

#[test]
fn resource_panics_on_missing() {
    let world = World::alloc();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        world.resource::<Health>();
    }));
    assert!(result.is_err());
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
    assert!(!r2.is_changed());
}

#[test]
fn resource_mut_change_detection() {
    let mut world = World::alloc();
    world.insert_resource(Health { value: 1 });

    let mut r = world.resource_mut::<Health>();
    assert!(r.is_changed());
    r.value = 42;
    assert_eq!(r.value, 42);
}

#[test]
fn resource_mut_panics_on_missing() {
    let mut world = World::alloc();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        world.resource_mut::<Health>();
    }));
    assert!(result.is_err());
}

// -----------------------------------------------------------------------------
// World resource API tests — NonSend resources
// -----------------------------------------------------------------------------

#[test]
fn non_send_insert_and_get() {
    let mut world = World::alloc();

    world.insert_non_send(Health { value: 7 });
    assert!(world.contains_non_send::<Health>());
    assert_eq!(world.get_non_send::<Health>(), Some(&Health { value: 7 }));
    assert_eq!(world.non_send::<Health>(), &Health { value: 7 });
}

#[test]
fn non_send_remove_and_drop() {
    let mut world = World::alloc();

    world.insert_non_send(Score { points: 200 });
    assert_eq!(
        world.remove_non_send::<Score>(),
        Some(Score { points: 200 })
    );
    assert!(!world.contains_non_send::<Score>());

    world.insert_non_send(Score { points: 300 });
    world.drop_non_send::<Score>();
    assert!(!world.contains_non_send::<Score>());
}

#[test]
fn non_send_ref_change_detection() {
    let mut world = World::alloc();
    world.insert_non_send(Health { value: 3 });

    let r = world.non_send_ref::<Health>();
    assert!(r.is_added());
    assert!(r.is_changed());
    assert_eq!(r.value, 3);
}

#[test]
fn non_send_mut_change_detection() {
    let mut world = World::alloc();
    world.insert_non_send(Health { value: 5 });

    let mut r = world.non_send_mut::<Health>();
    r.value = 55;
    assert_eq!(r.value, 55);
}

// -----------------------------------------------------------------------------
// World resource API tests — Send / NonSend shared slot
// -----------------------------------------------------------------------------

#[test]
fn send_and_non_send_share_slot() {
    let mut world = World::alloc();

    world.insert_resource(Health { value: 10 });
    assert_eq!(world.get_non_send::<Health>(), Some(&Health { value: 10 }));

    world.insert_non_send(Health { value: 20 });
    assert_eq!(world.get_resource::<Health>(), Some(&Health { value: 20 }));
}

#[test]
fn contains_resource_false_for_uninserted() {
    let world = World::alloc();
    assert!(!world.contains_resource::<Health>());
    assert!(!world.contains_non_send::<Health>());
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
