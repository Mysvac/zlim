use crate::entity::{EntityId, EntityMapper};
use crate::error::{ZlimError, ZlimResult};

use super::context::EntityReference;
use super::{IntoTemplate, SpecializeTemplate, Template, TemplateContext};

// -----------------------------------------------------------------------------
// EntityTemplate

/// A [`Template`] of an [`EntityId`].
///
/// An entity cannot be created out of thin air, so a component that refers to one is described by
/// *how* to find that entity instead of by the entity itself: either it is already known, or it is
/// one of the named entities of the scene, which is what a `#Name` in a scene expands to.
///
/// Building a template that was left at [`EntityTemplate::None`] is an error.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub enum EntityTemplate {
    /// An entity that is already known.
    Entity(EntityId),

    /// A named entity of the scene, resolved to the entity
    /// it stands for when the template is built.
    EntityReference(EntityReference),

    /// No entity at all, which fails to build.
    #[default]
    None,
}

impl From<EntityId> for EntityTemplate {
    #[inline]
    fn from(entity: EntityId) -> Self {
        Self::Entity(entity)
    }
}

impl From<EntityReference> for EntityTemplate {
    #[inline]
    fn from(entity: EntityReference) -> Self {
        Self::EntityReference(entity)
    }
}

// -----------------------------------------------------------------------------

/// Keeps this type out of the blanket `Clone + Unpin` implementations of [`Template`]
/// and [`IntoTemplate`], which is what allows it to implement [`Template`] itself.
///
/// See [`SpecializeTemplate`] for how the condition works.
impl Unpin for EntityTemplate where for<'a> [()]: SpecializeTemplate {}

impl Template for EntityTemplate {
    type Output = EntityId;

    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    fn build_template(&self, context: &mut TemplateContext) -> ZlimResult<Self::Output> {
        const MSG: &str = "no entity was specified for this `EntityTemplate`";
        match self {
            Self::Entity(entity) => Ok(context.entity_mapper().get_mapped(*entity)),
            Self::EntityReference(x) => context.resolve_entity(*x),
            Self::None => Err(ZlimError::error(MSG)),
        }
    }

    #[inline]
    fn clone_template(&self) -> Self {
        *self
    }
}

impl IntoTemplate for EntityId {
    type Template = EntityTemplate;

    #[inline]
    fn into_template(self) -> Self::Template {
        EntityTemplate::Entity(self)
    }
}

// -----------------------------------------------------------------------------
