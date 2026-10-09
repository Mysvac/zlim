//! The template of a *value*: a component that describes itself.

use zlim_error::ZlimResult;
use zlim_reflect::Reflect;

use crate::bundle::BundleWriter;
use crate::component::Component;
use crate::template::{ErasedTemplate, ReflectTemplate, Template, TemplateContext};

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
            // ↑ compile-time optimization
            value.map_entities(&mut context.entity_mapper());
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

impl<T: Reflect + Component + Clone> ReflectTemplate for ComponentTemplate<T>
where
    Self: ErasedTemplate,
{
    fn as_reflect(&self) -> &dyn Reflect {
        &self.0
    }

    fn as_reflect_mut(&mut self) -> &mut dyn Reflect {
        &mut self.0
    }

    fn into_reflect(self: Box<Self>) -> Box<dyn Reflect> {
        let ptr: *mut ComponentTemplate<T> = Box::leak(self);
        // SAFETY: ComponentTemplate<T> is transparent to T
        unsafe { Box::from_raw(ptr as *mut T) }
    }

    fn apply_owned(
        self: Box<Self>,
        context: &mut TemplateContext,
        writer: &mut BundleWriter,
    ) -> ZlimResult<()> {
        let mut component = self.0;
        if !T::NO_ENTITY {
            T::map_entities(&mut component, &mut context.entity_mapper());
        }
        writer.push(component, Some(context.components()));
        Ok(())
    }

    fn clone_reflect_template(&self) -> Box<dyn ReflectTemplate> {
        Box::new(ComponentTemplate(T::clone(&self.0)))
    }
}

// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::ComponentTemplate;
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
    fn component_template_map_entity() {
        let document_id = EntityId::new(1, 1.try_into().unwrap());
        let template = ComponentTemplate(Link { to: document_id });

        let mut world = World::alloc();
        let target = world.spawn_empty(None).id();

        let mut entities = EntityMap::with_capacity(1);
        let mut references = EntityReferences::new();
        entities.set_mapped(document_id, target);

        let mut entity = world.spawn_empty(None);
        let mut context = TemplateContext::new(&mut entity, &mut references, &mut entities);

        let built = template.build_template(&mut context).unwrap();

        assert_eq!(built, Link { to: target });
    }

    /// For an undeclared entity id, if the entity exists it is kept as-is.
    #[test]
    fn component_template_undeclared_id_spawned() {
        let mut world = World::alloc();

        let target = world.spawn_empty(None).id();
        let template = ComponentTemplate(Link { to: target });

        let mut entities = EntityMap::new();
        let mut references = EntityReferences::new();
        let mut entity = world.spawn_empty(None);
        let mut context = TemplateContext::new(&mut entity, &mut references, &mut entities);

        let built = template.build_template(&mut context).unwrap();

        assert_eq!(built, Link { to: target });
    }

    /// For an undeclared entity id, if the entity does not exist it is replaced with PLACEHOLDER.
    #[test]
    fn component_template_undeclared_id_not_spawned() {
        let id = EntityId::new(2, 3.try_into().unwrap());
        let template = ComponentTemplate(Link { to: id });

        let mut world = World::alloc();
        let mut entities = EntityMap::new();
        let mut references = EntityReferences::new();
        let mut entity = world.spawn_empty(None);
        let mut context = TemplateContext::new(&mut entity, &mut references, &mut entities);

        let built = template.build_template(&mut context).unwrap();

        assert_eq!(
            built,
            Link {
                to: EntityId::PLACEHOLDER
            }
        );
    }
}
