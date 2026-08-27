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
    #[editor(get, set)]
    value: u32,
}

#[derive(TypePath, Resource, Debug, PartialEq, Eq)]
struct Score {
    #[editor(get)]
    points: u64,
}

#[derive(TypePath, Resource)]
struct Mixed {
    #[editor(get, set)]
    hp: u32,
    #[editor(get)]
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
    #[editor(get, set)] f32,
    #[editor(get, set)] f32,
    #[editor(get)] f32,
);

#[derive(TypePath, Resource)]
struct GenericRes<T: Send + Sync + 'static> {
    #[editor(get, set)]
    data: T,
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
fn derive_fields_const() {
    assert_eq!(Mixed::GETTER, &["hp", "id"]);
    assert_eq!(Mixed::SETTER, &["hp"]);
}

#[test]
fn derive_no_editor_defaults() {
    assert_eq!(NoEditor::GETTER.len(), 0);
    assert_eq!(NoEditor::SETTER.len(), 0);
}

#[test]
fn derive_get_field() {
    let m = Mixed {
        hp: 42,
        id: 7,
        _hidden: "x".into(),
    };
    let f = m.get_field("hp").unwrap();
    assert_eq!(*f.downcast_ref::<u32>().unwrap(), 42);
}

#[test]
fn derive_get_field_getter_only() {
    let m = Mixed {
        hp: 42,
        id: 7,
        _hidden: "x".into(),
    };
    let f = m.get_field("id").unwrap();
    assert_eq!(*f.downcast_ref::<u64>().unwrap(), 7);
}

#[test]
fn derive_get_field_unmarked_not_exposed() {
    let m = Mixed {
        hp: 0,
        id: 0,
        _hidden: "secret".into(),
    };
    assert!(m.get_field("_hidden").is_none());
    assert!(m.get_field("nonexistent").is_none());
}

#[test]
fn derive_set_field() {
    let mut m = Mixed {
        hp: 10,
        id: 3,
        _hidden: "a".into(),
    };
    m.set_field("hp", &99u32).unwrap();
    assert_eq!(m.hp, 99);

    // getter-only fields are not writable.
    assert!(m.set_field("id", &7u64).is_err());
    assert!(m.set_field("_hidden", &"x".to_string()).is_err());
}

#[test]
fn derive_set_field_errors() {
    let mut m = Mixed {
        hp: 10,
        id: 3,
        _hidden: "a".into(),
    };

    // Missing field.
    let err = m.set_field("nonexistent", &0u32).unwrap_err();
    assert!(
        err.contains("Type `Mixed` is missing field `nonexistent`"),
        "{err}"
    );

    // Apply failure — the wrong reflected type is left unchanged.
    let err = m.set_field("hp", &"not a number".to_string()).unwrap_err();
    assert!(
        err.contains("Type `Mixed` failed to assign field `hp`"),
        "{err}"
    );
    assert_eq!(m.hp, 10);
}

#[test]
fn derive_tuple_struct() {
    assert_eq!(TupleRes::GETTER, &["0", "1", "2"]);
    assert_eq!(TupleRes::SETTER, &["0", "1"]);

    let mut t = TupleRes(1.0, 2.0, 3.0);
    assert_eq!(
        *t.get_field("0").unwrap().downcast_ref::<f32>().unwrap(),
        1.0
    );
    assert_eq!(
        *t.get_field("2").unwrap().downcast_ref::<f32>().unwrap(),
        3.0
    );
    assert!(t.set_field("2", &9.0f32).is_err());
}

// -----------------------------------------------------------------------------
// Trait default implementations
// -----------------------------------------------------------------------------

/// A resource with no derive — exercises the trait defaults directly.
#[derive(TypePath)]
struct NoFields;

impl ResourceTrait for NoFields {}

#[test]
fn default_field_behavior() {
    let mut r = NoFields;
    assert!(r.get_field("anything").is_none());

    let err = r.set_field("anything", &0u32).unwrap_err();
    assert!(err.contains("NoFields"), "{err}");
    assert!(err.contains("exposes no fields"), "{err}");
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
    let result = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
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
    let result = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
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
