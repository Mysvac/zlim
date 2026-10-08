//! The two ways a scene describes the hierarchy: its children, and its parent.

use zlim_core::entity::EntityId;
use zlim_core::template::{EntityReference, EntityTemplate};
use zlim_error::ZlimResult;

use crate::dependency::SceneDependencies;
use crate::resolved::ResolvedScene;
use crate::scene::{ResolveContext, Scene};
use crate::scene_list::SceneList;

// -----------------------------------------------------------------------------
// SceneChildren

/// The entities that belong under the entity this scene describes.
///
/// Because the hierarchy lives in the world rather than in a component (see the [module documentation](super)),
/// these entities are not related back to their parent by a component: they are spawned *with* the parent,
/// which is the one operation that both links them and puts them in [`Children`]. Nothing has to be connected
/// afterwards, and the order of the list is the order of the children.
///
/// This is the counterpart of Bevy's `RelatedScenes<ChildOf, L>`; since the hierarchy is the only
/// relation a scene can describe, it carries no relationship type.
///
/// # Example
///
/// ```rust
/// use zlim_scene::{ResolveContext, ResolvedScene, Scene, SceneChildren};
///
/// let mut context = ResolveContext::new();
/// let mut scene = ResolvedScene::new();
///
/// // `()` is a scene that describes nothing, so this adds two empty child scenes.
/// SceneChildren(vec![(), ()])
///     .resolve(&mut context, &mut scene)
///     .unwrap();
///
/// assert_eq!(scene.children().len(), 2);
/// ```
///
/// [`Children`]: zlim_core::query::Children
pub struct SceneChildren<L: SceneList>(pub L);

impl<L: SceneList> SceneChildren<L> {
    /// Creates a scene that adds the given list as children.
    #[inline]
    pub const fn new(children: L) -> Self {
        Self(children)
    }
}

impl<L: SceneList> Scene for SceneChildren<L> {
    fn resolve(self, context: &mut ResolveContext, scene: &mut ResolvedScene) -> ZlimResult<()> {
        let mut children = Vec::new();
        self.0.resolve_list(context, &mut children)?;
        scene.children_mut().append(&mut children);
        Ok(())
    }

    #[inline]
    fn register_dependencies(&self, dependencies: &mut SceneDependencies) {
        self.0.register_dependencies(dependencies);
    }
}

// -----------------------------------------------------------------------------
// SceneParent

/// The parent of the entity this scene describes.
///
/// This is the edge that a scene *applied to an existing entity* needs, and the counterpart of
/// Bevy's `ChildOf` — except that there is no component to insert: the edge is recorded on the
/// [`ResolvedScene`] and applied once every entity of the scene exists. That is what lets it name an
/// entity declared later in the same scene, and what keeps the entity tree the single source of
/// truth.
///
/// An entity the scene created is moved with [`reparent_without_signal`]: it is new, so its
/// own ticks already tell the transform propagation to look at it, and a [`ReparentSignal`] would
/// only add a message that says nothing new. An entity that already existed — a scene applied to it,
/// rather than spawned for it — is moved with [`reparent`], because moving something that is already
/// placed in the tree is exactly what propagation has to hear about.
///
/// # What an edge can say
///
/// A scene that carries no `SceneParent` leaves the hierarchy alone: the entity keeps whatever parent
/// it has. Carrying one always says something, and [`EntityTemplate::None`] is an answer rather than
/// silence — it names no parent, so the entity is moved to the root:
///
/// ```rust
/// use zlim_core::template::EntityTemplate;
/// use zlim_scene::{ResolveContext, ResolvedScene, Scene, SceneParent};
///
/// let mut scene = ResolvedScene::new();
/// SceneParent::new(EntityTemplate::None)
///     .resolve(&mut ResolveContext::new(), &mut scene)
///     .unwrap();
///
/// // An answer, not silence: the entity is described as having no parent.
/// assert_eq!(scene.parent(), Some(EntityTemplate::None));
/// ```
///
/// The children of a scene do not need this: they are spawned with their parent already in place.
/// When both are present, the explicit edge wins, because it is applied last.
///
/// # Example
///
/// ```rust
/// use zlim_core::entity::EntityId;
/// use zlim_scene::{ResolveContext, ResolvedScene, Scene, SceneParent};
/// use zlim_core::template::EntityTemplate;
///
/// let parent = EntityId::from_bits(0x0000_0001_0000_0001).unwrap();
///
/// let mut scene = ResolvedScene::new();
/// SceneParent::from(parent)
///     .resolve(&mut ResolveContext::new(), &mut scene)
///     .unwrap();
///
/// assert_eq!(scene.parent(), Some(EntityTemplate::Entity(parent)));
/// ```
///
/// [`reparent_without_signal`]: zlim_core::ops::EntityOwned::reparent_without_signal
/// [`reparent`]: zlim_core::ops::EntityOwned::reparent
/// [`ReparentSignal`]: zlim_core::message::ReparentSignal
#[derive(Copy, Clone, Debug, Default)]
pub struct SceneParent {
    /// The entity that becomes this entity's parent, or no entity at all for the root.
    pub parent: EntityTemplate,
}

impl SceneParent {
    /// Creates a parent edge to `parent`.
    #[inline]
    pub const fn new(parent: EntityTemplate) -> Self {
        Self { parent }
    }
}

impl From<EntityId> for SceneParent {
    #[inline]
    fn from(parent: EntityId) -> Self {
        Self::new(parent.into())
    }
}

impl From<EntityReference> for SceneParent {
    #[inline]
    fn from(parent: EntityReference) -> Self {
        Self::new(parent.into())
    }
}

impl Scene for SceneParent {
    #[inline]
    fn resolve(self, _ctx: &mut ResolveContext, scene: &mut ResolvedScene) -> ZlimResult<()> {
        scene.set_parent(self.parent);
        Ok(())
    }
}
