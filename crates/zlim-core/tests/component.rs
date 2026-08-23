//! Integration tests for `#[derive(Component)]`.

use zlim_core::component::Component as ComponentTrait;
use zlim_core::component::ComponentDB;
use zlim_core::component::HookContext;
use zlim_core::derive::Component;
use zlim_core::entity::EntityId;
use zlim_core::entity::EntityMapper;
use zlim_core::world::DeferredWorld;
use zlim_core::world::World;
use zlim_reflect::TypePath;

use serde::{Deserialize, Serialize};

// -----------------------------------------------------------------------------
// Basic component
// -----------------------------------------------------------------------------

#[derive(TypePath, Component, Clone)]
struct Health {
    #[editor(get, set)]
    value: u32,
}

#[test]
fn component_fields_const() {
    assert_eq!(Health::GETTER, &["value"]);
    assert_eq!(Health::SETTER, &["value"]);
}

#[test]
fn component_no_entity_default() {
    const { assert!(Health::NO_ENTITY) };
}

#[test]
fn component_cloner_default_is_clonable() {
    let _cloner = Health::CLONER;
}

// -----------------------------------------------------------------------------
// Component with hooks
// -----------------------------------------------------------------------------

fn on_add_hook(_world: DeferredWorld, _ctx: HookContext) {}

#[derive(TypePath, Component, Clone)]
#[component(on_add = on_add_hook)]
struct HookedComp {
    _x: f32,
}

#[test]
fn component_hook_const_is_set() {
    assert!(HookedComp::ON_ADD.is_some());
    assert_eq!(
        HookedComp::ON_ADD.unwrap() as *const (),
        on_add_hook as *const (),
    );
}

// -----------------------------------------------------------------------------
// Component with #[entities]
// -----------------------------------------------------------------------------

#[derive(TypePath, Component, Clone)]
struct WithEntities {
    #[entities]
    targets: Vec<EntityId>,
    value: u32,
}

#[test]
fn component_with_entities_requires_remap() {
    const { assert!(!WithEntities::NO_ENTITY) };
}

#[test]
fn component_with_entities_map_entities() {
    struct TestMapper;
    impl EntityMapper for TestMapper {
        fn get_mapped(&mut self, source: EntityId) -> EntityId {
            source
        }
        fn set_mapped(&mut self, _source: EntityId, _target: EntityId) {}
    }

    let mut comp = WithEntities {
        targets: vec![EntityId::from_bits(0x0000_0001_0000_0001).unwrap()],
        value: 42,
    };

    let _ = comp.value; // suppress clippy

    comp.map_entities(&mut TestMapper);
}

// -----------------------------------------------------------------------------
// Component with custom map_entities
// -----------------------------------------------------------------------------

fn remap_payload<M: EntityMapper>(comp: &mut CustomMapped, mapper: &mut M) {
    comp.target = mapper.get_mapped(comp.target);
    comp.calls += 1;
}

#[derive(TypePath, Component, Clone)]
#[component(map_entities = remap_payload)]
struct CustomMapped {
    target: EntityId,
    value: u32,
    calls: u32,
}

#[test]
fn component_with_custom_map_entities_requires_remap() {
    const { assert!(!CustomMapped::NO_ENTITY) };
}

#[test]
fn component_with_custom_map_entities_is_called() {
    struct ShiftMapper;
    impl EntityMapper for ShiftMapper {
        fn get_mapped(&mut self, source: EntityId) -> EntityId {
            EntityId::from_bits(source.to_bits().wrapping_add(1)).unwrap()
        }
        fn set_mapped(&mut self, _source: EntityId, _target: EntityId) {}
    }

    let source = EntityId::from_bits(0x0000_0001_0000_0001).unwrap();
    let mut comp = CustomMapped {
        target: source,
        value: 42,
        calls: 0,
    };

    comp.map_entities(&mut ShiftMapper);

    assert_eq!(comp.calls, 1);
    assert_eq!(
        comp.target,
        EntityId::from_bits(0x0000_0001_0000_0002).unwrap()
    );
    assert_eq!(comp.value, 42);
}

// -----------------------------------------------------------------------------
// Component with copy and hooks
// -----------------------------------------------------------------------------

#[derive(TypePath, Component, Clone, Copy)]
#[component(copy, on_clone = Self::on_clone)]
struct CopyComp {
    _x: i32,
}

impl CopyComp {
    fn on_clone(_world: DeferredWorld, _ctx: HookContext) {}
}

#[test]
fn copy_component_compiles() {
    fn _assert_component<T: ComponentTrait>() {}
    _assert_component::<CopyComp>();
    assert!(CopyComp::ON_CLONE.is_some());
}

// -----------------------------------------------------------------------------
// Getter-only editor field
// -----------------------------------------------------------------------------

#[derive(TypePath, Component, Clone)]
struct ReadonlyField {
    #[editor(get)]
    id: u64,
}

#[test]
fn readonly_field_not_in_setter() {
    assert_eq!(ReadonlyField::GETTER, &["id"]);
    assert_eq!(ReadonlyField::SETTER.len(), 0);
    assert!(ReadonlyField { id: 1 }.get_field("id").is_some());
    assert!(ReadonlyField { id: 1 }.set_field("id", &7u64).is_err());
}

// -----------------------------------------------------------------------------
// Serialization opt-in
// -----------------------------------------------------------------------------

/// A component that does **not** implement serialization.
#[derive(TypePath, Component, Clone)]
struct NoSerde(u32);

/// A component registered with serialization support.
#[derive(TypePath, Component, Clone, Serialize, Deserialize)]
#[component(serialize)]
struct SerdeComp(u32);

#[test]
fn non_serializable_component_registers_without_serializer() {
    let db = ComponentDB::of::<NoSerde>();
    assert!(db.serialize.is_none());
    assert!(db.deserialize.is_none());
    assert!(!NoSerde::SERIALIZE);

    // The component still works in a world.
    let mut world = World::alloc();
    world.spawn((NoSerde(1),), None);

    let probe = NoSerde(7);
    assert_eq!(probe.0, 7);
}

#[test]
fn serializable_component_registers_with_serializer() {
    let db = ComponentDB::of::<SerdeComp>();
    assert!(db.serialize.is_some());
    assert!(db.deserialize.is_some());
    assert!(SerdeComp::SERIALIZE);

    let probe = SerdeComp(9);
    assert_eq!(probe.0, 9);
}

// -----------------------------------------------------------------------------
// Required components
// -----------------------------------------------------------------------------

#[derive(TypePath, Component, Clone, Default, Debug, PartialEq, Eq)]
struct Global(u32);

#[derive(TypePath, Component, Clone, Default, Debug, PartialEq, Eq)]
struct Local(u32);

#[derive(TypePath, Component, Clone, Default)]
#[require(Global)]
struct Transform;

#[derive(TypePath, Component, Clone, Default)]
#[require(Global, Local)]
struct RigidBody;

// Nested chain: `Node` requires `Anchor`, which requires `Global`.
#[derive(TypePath, Component, Clone, Default)]
#[require(Anchor)]
struct Node;

#[derive(TypePath, Component, Clone, Default)]
#[require(Global)]
struct Anchor;

#[test]
fn required_component_auto_inserted_on_spawn() {
    let mut world = World::alloc();
    let entity = world.spawn(Transform, None).id();

    let owned = world.entity_owned(entity);
    assert!(owned.get::<Transform>().is_some());
    // Required component initialised with its `Default` value.
    assert_eq!(owned.get::<Global>(), Some(&Global(0)));
}

#[test]
fn required_component_not_duplicated_when_explicit() {
    let mut world = World::alloc();
    let entity = world.spawn((Transform, Global(7)), None).id();

    let owned = world.entity_owned(entity);
    // The explicitly provided value wins over the auto-inserted default.
    assert_eq!(owned.get::<Global>(), Some(&Global(7)));
}

#[test]
fn required_components_multiple_and_nested() {
    let mut world = World::alloc();

    // Multiple required components.
    let entity = world.spawn(RigidBody, None).id();
    let owned = world.entity_owned(entity);
    assert_eq!(owned.get::<Global>(), Some(&Global(0)));
    assert_eq!(owned.get::<Local>(), Some(&Local(0)));

    // Nested: `Node` requires `Anchor`, which requires `Global`.
    let entity = world.spawn(Node, None).id();
    let owned = world.entity_owned(entity);
    assert!(owned.get::<Anchor>().is_some());
    assert!(owned.get::<Global>().is_some());
}

#[test]
fn required_components_auto_inserted_on_insert() {
    let mut world = World::alloc();
    let entity = world.spawn((), None).id();
    world.entity_owned(entity).insert(Transform).unwrap();

    let owned = world.entity_owned(entity);
    assert!(owned.get::<Transform>().is_some());
    assert_eq!(owned.get::<Global>(), Some(&Global(0)));
}
