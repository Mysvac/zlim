//! Integration tests for `#[derive(Component)]`.

use zlim_core::component::Component as ComponentTrait;
use zlim_core::component::HookContext;
use zlim_core::derive::Component;
use zlim_core::entity::EntityId;
use zlim_core::entity::EntityMapper;
use zlim_core::world::DeferredWorld;
use zlim_reflect::TypePath;

use serde::{Deserialize, Serialize};

// ----------------------------------------------------------------------------
// Basic component
// ----------------------------------------------------------------------------

#[derive(TypePath, Component, Clone, Serialize, Deserialize)]
struct Health {
    #[editor(mutable)]
    value: u32,
}

#[test]
fn component_fields_const() {
    assert_eq!(Health::FIELDS, &["value"]);
    assert_eq!(Health::MUTABLE_FIELDS, &["value"]);
    assert_eq!(Health::READONLY_FIELDS.len(), 0);
}

#[test]
fn component_no_entity_default() {
    assert!(!Health::NO_ENTITY);
}

#[test]
fn component_cloner_default_is_clonable() {
    let _cloner = Health::CLONER;
}

// ----------------------------------------------------------------------------
// Component with hooks
// ----------------------------------------------------------------------------

fn on_add_hook(_world: DeferredWorld, _ctx: HookContext) {}

#[derive(TypePath, Component, Clone, Serialize, Deserialize)]
#[component(on_add = on_add_hook)]
struct HookedComp {
    x: f32,
}

#[test]
fn component_hook_const_is_set() {
    assert!(HookedComp::ON_ADD.is_some());
    assert_eq!(
        HookedComp::ON_ADD.unwrap() as *const (),
        on_add_hook as *const (),
    );
}

// ----------------------------------------------------------------------------
// Component with #[entities]
// ----------------------------------------------------------------------------

#[derive(TypePath, Component, Clone, Serialize, Deserialize)]
struct WithEntities {
    #[entities]
    targets: Vec<EntityId>,
    value: u32,
}

#[test]
fn component_with_entities_no_entity_true() {
    assert!(WithEntities::NO_ENTITY);
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

    comp.map_entities(&mut TestMapper);
}

// ----------------------------------------------------------------------------
// Component with copy and hooks
// ----------------------------------------------------------------------------

#[derive(TypePath, Component, Clone, Copy, Serialize, Deserialize)]
#[component(copy, on_clone = Self::on_clone)]
struct CopyComp {
    x: i32,
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

// ----------------------------------------------------------------------------
// Readonly editor field
// ----------------------------------------------------------------------------

#[derive(TypePath, Component, Clone, Serialize, Deserialize)]
struct ReadonlyField {
    #[editor(readonly)]
    id: u64,
}

#[test]
fn readonly_field_not_in_mutable() {
    assert_eq!(ReadonlyField::MUTABLE_FIELDS.len(), 0);
    assert_eq!(ReadonlyField::READONLY_FIELDS, &["id"]);
    assert!(ReadonlyField { id: 1 }.field_mut("id").is_none());
    assert!(ReadonlyField { id: 1 }.field("id").is_some());
}
