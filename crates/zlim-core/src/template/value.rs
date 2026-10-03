//! The template of a *value*: a component that describes itself.

use crate::component::Component;
use crate::error::ZlimResult;
use crate::template::{Template, TemplateContext};

// -----------------------------------------------------------------------------
// ComponentTemplate

/// A component value that is its own template.
///
/// A component that is serialized has two possible templates, and they answer different questions:
///
/// - the canonical template of the *type* — what `#[derive(IntoTemplate)]` generates — describes
///   which templates its fields are made of, so a component can be spelled out field by field;
/// - this one describes one *particular value*, which is what a component read back from a scene
///   document is. Building it clones the value and rewrites the entities it carries, so the same
///   document can be applied to a world over and over.
///
/// The second is what a scene needs, because reflection hands back a value and knows nothing about
/// the type's canonical template.
///
/// # Layout
///
/// This is `#[repr(transparent)]` over `T`, which is what lets a value that is already behind a
/// pointer be reinterpreted as its own template instead of being copied into a new allocation.
/// See [`ComponentDB::into_template`](crate::component::ComponentDB::into_template).
#[repr(transparent)]
pub struct ComponentTemplate<T>(pub T);

impl<T> Template for ComponentTemplate<T>
where
    T: Component + Clone,
{
    type Output = T;

    fn build_template(&self, context: &mut TemplateContext) -> ZlimResult<Self::Output> {
        let mut value = T::clone(&self.0);
        if !T::NO_ENTITY {
            Component::map_entities(&mut value, context.entity_mapper);
        }
        Ok(value)
    }

    fn clone_template(&self) -> Self {
        Self(T::clone(&self.0))
    }
}

impl<T> From<T> for ComponentTemplate<T> {
    #[inline]
    fn from(value: T) -> Self {
        Self(value)
    }
}

// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::component::Component;
    use crate::entity::{EntityId, EntityMap, EntityMapper};
    use crate::template::{EntityReferences, Template, TemplateContext};
    use crate::world::World;

    /// A component that points at another entity, which is what a serialized component carries.
    ///
    /// The field is marked `#[entities]` so that `#[derive(Component)]` generates the remapping: the
    /// derive always emits one, so a hand-written implementation would be shadowed by it.
    #[derive(Component, Clone, PartialEq, Debug)]
    struct Link {
        #[entities]
        to: EntityId,
    }

    /// One value serves every application, each pointing at the entity its own scene spawned.
    #[test]
    fn a_component_template_maps_the_entities_it_carries() {
        let document_id = EntityId::from_bits(0x0000_0001_0000_0001).unwrap();
        let template = super::ComponentTemplate(Link { to: document_id });

        let mut world = World::alloc();
        let target = world.spawn_empty(None).id();

        let mut entities = EntityMap::with_capacity(1);
        entities.set_mapped(document_id, target);
        let mut references = EntityReferences::new();
        let mut entity = world.spawn_empty(None);
        let mut context = TemplateContext::new(&mut entity, &mut references, &mut entities);

        let built = template
            .build_template(&mut context)
            .expect("the template is expected to build");
        assert_eq!(built, Link { to: target });

        // The original value is untouched, so the template can be applied again.
        assert_eq!(template.0.to, document_id);
    }

    /// An id no scene declared is kept, which is what lets a template built in Rust work.
    #[test]
    fn an_undeclared_id_is_kept() {
        let id = EntityId::from_bits(0x0000_0002_0000_0003).unwrap();
        let template = super::ComponentTemplate(Link { to: id });

        let mut world = World::alloc();
        let mut entities = EntityMap::new();
        let mut references = EntityReferences::new();
        let mut entity = world.spawn_empty(None);
        let mut context = TemplateContext::new(&mut entity, &mut references, &mut entities);

        let built = template
            .build_template(&mut context)
            .expect("the template is expected to build");
        assert_eq!(built, Link { to: id });
    }
}
