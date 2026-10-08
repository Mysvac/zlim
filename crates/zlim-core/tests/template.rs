//! Integration tests for `#[derive(IntoTemplate)]`.

use zlim_core::derive::IntoTemplate;
use zlim_core::entity::{EntityId, EntityMap};
use zlim_core::template::IntoTemplate as _;
use zlim_core::template::{EntityReference, EntityReferences, EntityTemplate};
use zlim_core::template::{Template, TemplateContext, VecTemplate};
use zlim_core::world::World;
use zlim_error::ZlimResult;

// -----------------------------------------------------------------------------
// Test types

/// A type with a templateable field: an entity reference.
#[derive(IntoTemplate, Debug, PartialEq)]
struct Widget {
    entity: EntityId,
    scale: f32,
}

/// A type that asks for the built-in template of a container field.
#[derive(IntoTemplate, Debug, PartialEq)]
struct Bag {
    #[template(built_in)]
    items: Vec<EntityId>,
}

/// A tuple struct.
#[derive(IntoTemplate, Debug, PartialEq)]
struct Pair(EntityId, f32);

/// A type whose two fields hold the same kind of reference.
#[derive(IntoTemplate, Debug, PartialEq)]
struct Link {
    from: EntityId,
    to: EntityId,
}

/// A unit struct.
#[derive(IntoTemplate, Debug, PartialEq)]
struct Marker;

/// A generic type. Its field's `IntoTemplate` bound comes from the derive, not from here, so the
/// type itself is written with no bounds at all.
#[derive(IntoTemplate, Debug, PartialEq)]
struct Holder<T> {
    value: T,
}

/// A generic type whose field asks for the built-in template,
/// which the derive constrains with `BuiltInTemplate` rather than `IntoTemplate`.
#[derive(IntoTemplate, Debug, PartialEq)]
struct Bagged<T> {
    #[template(built_in)]
    items: Vec<T>,
}

/// A generic enum, which needs a variant marked with `#[default]` just like a concrete one.
#[derive(IntoTemplate, Debug, PartialEq)]
enum Either<T> {
    #[default]
    Nothing,
    Just(T),
    Pair {
        left: T,
    },
}

/// A template of a `u32`, which is what an explicitly named field template looks like.
#[derive(Debug, Default, PartialEq)]
struct Twice(u32);

impl Template for Twice {
    type Output = u32;

    fn build_template(&self, _context: &mut TemplateContext) -> ZlimResult<Self::Output> {
        Ok(self.0 * 2)
    }

    fn clone_template(&self) -> Self {
        Self(self.0)
    }
}

/// A value can be described by a template other than the canonical one, as long as it converts.
impl From<u32> for Twice {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

/// A type whose field names its template explicitly, so the field needs a conversion.
#[derive(IntoTemplate, Debug, PartialEq)]
struct Doubled {
    #[template(Twice)]
    value: u32,
}

/// The conversion a field with `into = ...` would otherwise have to get from `Into`.
fn quadrupled(value: u32) -> Twice {
    Twice(value * 4)
}

/// `into = path` names the function in place of the conversion the field would otherwise use, so
/// the field needs no `Into` impl for the named template.
#[derive(IntoTemplate, Debug, PartialEq)]
struct Quadrupled {
    #[template(Twice, into = quadrupled)]
    value: u32,
}

/// `into = path` also works with `built_in`, where it replaces the `BuiltInTemplate` conversion.
#[derive(IntoTemplate, Debug, PartialEq)]
struct Dropped {
    #[template(built_in, into = dropped)]
    items: Vec<EntityId>,
}

/// Stands in for the `BuiltInTemplate` conversion of a `Vec<EntityId>` field.
fn dropped(_items: Vec<EntityId>) -> VecTemplate<EntityTemplate> {
    VecTemplate(Vec::new())
}

/// An enum, which needs a variant marked with `#[default]`.
#[derive(IntoTemplate, Debug, PartialEq)]
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
    let mut entities = EntityMap::new();
    let mut entity = world.spawn_empty(None);
    let mut context = TemplateContext::new(&mut entity, &mut references, &mut entities);
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

/// The template of a tuple struct is a tuple struct.
#[test]
fn a_tuple_struct_template_builds_a_tuple_struct() {
    let mut world = World::alloc();
    let entity = world.spawn_empty(None).id();

    let template = PairTemplate(EntityTemplate::Entity(entity), 3.0);
    assert_eq!(build(&mut world, &template), Pair(entity, 3.0));
}

/// `into = path` is what converts the field, so it replaces the `Into` conversion the named
/// template would otherwise need — and it is the function that decides the value.
#[test]
fn an_into_function_decides_the_conversion() {
    let mut world = World::alloc();

    // `Doubled` converts with `Into` and doubles; `Quadrupled` converts with its own function.
    let doubled = Doubled { value: 21 }.into_template();
    assert_eq!(doubled.value, Twice(21));
    assert_eq!(build(&mut world, &doubled), Doubled { value: 42 });

    let quadrupled = Quadrupled { value: 21 }.into_template();
    // `quadrupled` runs on the source value and produces the template value; building that
    // template then doubles it again, because that is what `Twice` itself does.
    assert_eq!(quadrupled.value, Twice(84));
    assert_eq!(build(&mut world, &quadrupled), Quadrupled { value: 168 });
}

/// `into = path` replaces the `BuiltInTemplate` conversion of a `built_in` field.
#[test]
fn an_into_function_replaces_the_built_in_conversion() {
    let mut world = World::alloc();
    let entity = world.spawn_empty(None).id();
    let source = Dropped {
        items: vec![entity, entity],
    };

    // The named function ignores the field's contents, so the template comes out empty even though
    // the field holds two references. `VecTemplate` is not `PartialEq`, so the length is what is
    // checked.
    let template = source.into_template();
    assert!(template.items.0.is_empty());
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

    let mut entities = EntityMap::new();
    let mut entity = world.spawn_empty(None);
    let mut context = TemplateContext::new(&mut entity, &mut references, &mut entities);

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
    let mut entities = EntityMap::new();
    let mut entity = world.spawn_empty(None);
    let mut context = TemplateContext::new(&mut entity, &mut references, &mut entities);

    let template = WidgetTemplate {
        entity: EntityTemplate::None,
        scale: 1.0,
    };
    assert!(template.build_template(&mut context).is_err());
}

// -----------------------------------------------------------------------------
// into_template

/// `From` is what describes an existing value; `into_template` is `From::from(self)`.
#[test]
fn a_value_converts_through_from() {
    let entity = EntityId::PLACEHOLDER;

    // Both spellings are the same conversion, and the generated `From` impl is the one that runs.
    let via_from: WidgetTemplate = Widget { entity, scale: 1.0 }.into();
    let via_trait: WidgetTemplate = Widget { entity, scale: 2.0 }.into_template();

    assert!(matches!(via_from.entity, EntityTemplate::Entity(found) if found == entity));
    assert_eq!(via_from.scale, 1.0);
    assert_eq!(via_trait.scale, 2.0);
}

/// Every shape of a type gets a `From` impl: a tuple struct, a unit struct and each kind of enum
/// variant.
#[test]
fn every_shape_converts() {
    let entity = EntityId::PLACEHOLDER;

    let pair: PairTemplate = Pair(entity, 1.0).into();
    assert!(matches!(pair.0, EntityTemplate::Entity(found) if found == entity));
    assert_eq!(pair.1, 1.0);

    let marker: MarkerTemplate = Marker.into();
    let _ = marker;

    // A generic type keeps its own parameter in the template: the generated impl is
    // `From<Holder<T>> for HolderTemplate<T>`, so the field's template type is inferred from `T`.
    let holder: HolderTemplate<EntityId> = Holder { value: entity }.into();
    assert!(matches!(
        holder.value,
        EntityTemplate::Entity(found) if found == entity
    ));

    assert!(matches!(Shape::Empty.into(), ShapeTemplate::Empty));
    assert!(matches!(
        Shape::Point(entity).into(),
        ShapeTemplate::Point(_)
    ));
    assert!(matches!(
        Shape::Named { at: entity }.into(),
        ShapeTemplate::Named { .. }
    ));
    assert!(matches!(
        Shape::Sized(entity, 1.0).into(),
        ShapeTemplate::Sized(_, _)
    ));
}

/// The bounds a generic type's fields need are written by the derive, not by the type, and they
/// follow the choice each field made: a canonical field needs `IntoTemplate`, a `#[template(built_in)]`
/// one needs `BuiltInTemplate`, and an enum is no different from a struct.
#[test]
fn a_generic_type_needs_no_bounds_of_its_own() {
    let entity = EntityId::PLACEHOLDER;

    let holder: HolderTemplate<EntityId> = Holder { value: entity }.into();
    assert!(matches!(holder.value, EntityTemplate::Entity(found) if found == entity));

    let bagged: BaggedTemplate<EntityId> = Bagged {
        items: vec![entity],
    }
    .into();
    assert_eq!(bagged.items.0.len(), 1);

    let either: EitherTemplate<EntityId> = Either::Just(entity).into();
    assert!(matches!(either, EitherTemplate::Just(_)));
    let nothing: EitherTemplate<EntityId> = Default::default();
    assert!(matches!(nothing, EitherTemplate::Nothing));
}

/// `into_template` describes an existing value through the field templates of its type.
#[test]
fn a_value_becomes_its_template() {
    let mut world = World::alloc();
    let entity = world.spawn_empty(None).id();

    let template = Widget { entity, scale: 2.0 }.into_template();

    // The entity field went through `EntityTemplate`, the plain one stayed a `f32`.
    assert!(matches!(template.entity, EntityTemplate::Entity(found) if found == entity));
    assert_eq!(template.scale, 2.0);

    // And the result is still a working template of the original type.
    assert_eq!(build(&mut world, &template), Widget { entity, scale: 2.0 });
}

/// A `#[template(built_in)]` field converts through its type's `BuiltInTemplate`, so the elements
/// of a container become the templates of their elements.
#[test]
fn a_built_in_field_converts_through_its_built_in_template() {
    let mut world = World::alloc();
    let entity = world.spawn_empty(None).id();

    let template = Bag {
        items: vec![entity],
    }
    .into_template();

    assert_eq!(
        build(&mut world, &template),
        Bag {
            items: vec![entity]
        }
    );
}

/// Every variant of an enum is described by the template of that variant.
#[test]
fn every_enum_variant_becomes_its_template() {
    let mut world = World::alloc();
    let entity = world.spawn_empty(None).id();

    assert_eq!(
        build(&mut world, &Shape::Empty.into_template()),
        Shape::Empty
    );
    assert_eq!(
        build(&mut world, &Shape::Point(entity).into_template()),
        Shape::Point(entity)
    );
    assert_eq!(
        build(&mut world, &Shape::Named { at: entity }.into_template()),
        Shape::Named { at: entity }
    );
    assert_eq!(
        build(&mut world, &Shape::Sized(entity, 1.0).into_template()),
        Shape::Sized(entity, 1.0)
    );
}

/// A field that names its template explicitly is converted into it with `Into`, so the value can be
/// described by a template that is not the canonical one of its type.
#[test]
fn a_named_field_template_converts_the_value_into_it() {
    let mut world = World::alloc();

    let template = Doubled { value: 21 }.into_template();
    assert_eq!(template.value, Twice(21));
    assert_eq!(build(&mut world, &template), Doubled { value: 42 });
}
