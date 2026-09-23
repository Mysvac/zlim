//! Integration tests for `#[derive(FromTemplate)]`.

use zlim_core::derive::FromTemplate;
use zlim_core::entity::EntityId;
use zlim_core::template::{EntityReference, EntityReferences, EntityTemplate, OptionTemplate};
use zlim_core::template::{Template, TemplateContext, VecTemplate};
use zlim_core::world::World;

// -----------------------------------------------------------------------------
// Test types

/// A type with a templateable field: an entity reference.
#[derive(FromTemplate, Debug, PartialEq)]
struct Widget {
    entity: EntityId,
    scale: f32,
}

/// A type that asks for the built-in template of a container field.
#[derive(FromTemplate, Debug, PartialEq)]
struct Bag {
    #[template(built_in)]
    items: Vec<EntityId>,
}

/// A type that names the template of a container field by hand.
#[derive(FromTemplate, Debug, PartialEq)]
struct Slot {
    #[template(OptionTemplate<EntityTemplate>)]
    occupant: Option<EntityId>,
}

/// A tuple struct.
#[derive(FromTemplate, Debug, PartialEq)]
struct Pair(EntityId, f32);

/// A type whose two fields hold the same kind of reference.
#[derive(FromTemplate, Debug, PartialEq)]
struct Link {
    from: EntityId,
    to: EntityId,
}

/// A unit struct.
#[derive(FromTemplate, Debug, PartialEq)]
struct Marker;

/// An enum, which needs a variant marked with `#[default]`.
#[derive(FromTemplate, Debug, PartialEq)]
enum Shape {
    #[default]
    Empty,
    Point(EntityId),
    Named {
        at: EntityId,
    },
    Sized(EntityId, f32),
}

// -----------------------------------------------------------------------------
// Helpers

/// Builds `source` for a fresh entity of `world`.
fn build<T: Template>(world: &mut World, source: &T) -> T::Output {
    let mut references = EntityReferences::new();
    let mut entity = world.spawn_empty(None);
    let mut context = TemplateContext::new(&mut entity, &mut references);
    source
        .build_template(&mut context)
        .expect("the template is expected to build")
}

// -----------------------------------------------------------------------------
// Tests

/// A derived template builds the type it belongs to, field by field.
#[test]
fn a_derived_template_builds_its_type() {
    let mut world = World::alloc();
    let entity = world.spawn_empty(None).id();

    let template = WidgetTemplate {
        entity: EntityTemplate::Entity(entity),
        scale: 2.0,
    };
    assert_eq!(build(&mut world, &template), Widget { entity, scale: 2.0 });
}

/// `#[template(built_in)]` maps a container field to the template of its element.
#[test]
fn a_built_in_field_uses_the_template_of_its_element() {
    let mut world = World::alloc();
    let entity = world.spawn_empty(None).id();

    let template = BagTemplate {
        items: VecTemplate(vec![EntityTemplate::Entity(entity)]),
    };
    assert_eq!(
        build(&mut world, &template),
        Bag {
            items: vec![entity]
        }
    );
}

/// `#[template(Path)]` uses exactly the named template.
#[test]
fn a_named_field_template_is_used_as_it_is() {
    let mut world = World::alloc();
    let entity = world.spawn_empty(None).id();

    let template = SlotTemplate {
        occupant: OptionTemplate::Some(EntityTemplate::Entity(entity)),
    };
    assert_eq!(
        build(&mut world, &template),
        Slot {
            occupant: Some(entity)
        }
    );
}

/// The template of a tuple struct is a tuple struct.
#[test]
fn a_tuple_struct_template_builds_a_tuple_struct() {
    let mut world = World::alloc();
    let entity = world.spawn_empty(None).id();

    let template = PairTemplate(EntityTemplate::Entity(entity), 3.0);
    assert_eq!(build(&mut world, &template), Pair(entity, 3.0));
}

/// The template of a unit struct is a unit struct.
#[test]
fn a_unit_struct_template_builds_the_unit_struct() {
    let mut world = World::alloc();
    assert_eq!(build(&mut world, &MarkerTemplate), Marker);
}

/// An enum template builds every variant of its enum.
#[test]
fn an_enum_template_builds_its_variants() {
    let mut world = World::alloc();
    let entity = world.spawn_empty(None).id();

    assert_eq!(build(&mut world, &ShapeTemplate::Empty), Shape::Empty);
    assert_eq!(
        build(
            &mut world,
            &ShapeTemplate::Point(EntityTemplate::Entity(entity))
        ),
        Shape::Point(entity)
    );
    assert_eq!(
        build(
            &mut world,
            &ShapeTemplate::Named {
                at: EntityTemplate::Entity(entity)
            }
        ),
        Shape::Named { at: entity }
    );
    assert_eq!(
        build(
            &mut world,
            &ShapeTemplate::Sized(EntityTemplate::Entity(entity), 1.0)
        ),
        Shape::Sized(entity, 1.0)
    );
}

/// Two fields holding the same named reference resolve to one entity.
#[test]
fn the_same_reference_resolves_to_one_entity() {
    let mut world = World::alloc();
    let mut references = EntityReferences::new();
    let declared = world.spawn_empty(None).id();

    // The name is bound by the scene that declares it, which is what makes it resolvable at all.
    references.set(EntityReference::new("scene.rs", 3, 9, 0, 1), declared);

    let mut entity = world.spawn_empty(None);
    let mut context = TemplateContext::new(&mut entity, &mut references);

    let reference = || EntityTemplate::from(EntityReference::new("scene.rs", 3, 9, 0, 1));
    let template = LinkTemplate {
        from: reference(),
        to: reference(),
    };

    let link = template
        .build_template(&mut context)
        .expect("the template is expected to build");
    assert_eq!(link.from, link.to);
    assert_eq!(link.from, declared);
}

/// A template that names no entity fails to build.
#[test]
fn a_template_without_an_entity_fails() {
    let mut world = World::alloc();
    let mut references = EntityReferences::new();
    let mut entity = world.spawn_empty(None);
    let mut context = TemplateContext::new(&mut entity, &mut references);

    let template = WidgetTemplate {
        entity: EntityTemplate::None,
        scale: 1.0,
    };
    assert!(template.build_template(&mut context).is_err());
}
