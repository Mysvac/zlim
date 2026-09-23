//! The [`SceneList`](crate::SceneList) trait: a description of a list of entities, one [`Scene`](crate::Scene) each.

use zlim_core::error::ZlimResult;

use crate::dependency::SceneDependencies;
use crate::resolved::ResolvedScene;
use crate::scene::{ResolveContext, Scene};
// -----------------------------------------------------------------------------
// SceneList

/// A description of a list of entities, one [`Scene`](crate::Scene) each.
///
/// [`Scene`](crate::Scene) is to an entity what [`SceneList`](crate::SceneList) is to a list of entities. Resolving a list appends
/// one [`ResolvedScene`] per entity to the list it is given, in order.
pub trait SceneList: SceneListBox {
    /// Appends what this list describes to `scenes`, one [`ResolvedScene`] per entity.
    fn resolve_list(
        self,
        context: &mut ResolveContext,
        scenes: &mut Vec<ResolvedScene>,
    ) -> ZlimResult<()>;

    /// Registers every asset this list needs loaded before it can be resolved.
    ///
    /// The default implementation registers nothing.
    #[inline]
    fn register_dependencies(&self, _dependencies: &mut SceneDependencies) {}
}

/// The boxed form of [`SceneList`](crate::SceneList), which is what makes `Box<dyn SceneList>` possible.
///
/// See [`SceneBox`](crate::SceneBox) for why this exists; the reasoning is the same.
pub trait SceneListBox: Send + Sync + 'static {
    /// See [`SceneList::resolve_list`].
    fn resolve_list_box(
        self: Box<Self>,
        context: &mut ResolveContext,
        scenes: &mut Vec<ResolvedScene>,
    ) -> ZlimResult<()>;

    /// See [`SceneList::register_dependencies`].
    fn register_dependencies_box(&self, dependencies: &mut SceneDependencies);
}

impl<L: SceneList + Sized> SceneListBox for L {
    #[inline]
    fn resolve_list_box(
        self: Box<Self>,
        context: &mut ResolveContext,
        scenes: &mut Vec<ResolvedScene>,
    ) -> ZlimResult<()> {
        (*self).resolve_list(context, scenes)
    }

    #[inline]
    fn register_dependencies_box(&self, dependencies: &mut SceneDependencies) {
        self.register_dependencies(dependencies);
    }
}

impl<T: ?Sized + SceneListBox> SceneList for Box<T> {
    #[inline]
    fn resolve_list(
        self,
        context: &mut ResolveContext,
        scenes: &mut Vec<ResolvedScene>,
    ) -> ZlimResult<()> {
        SceneListBox::resolve_list_box(self, context, scenes)
    }

    #[inline]
    fn register_dependencies(&self, dependencies: &mut SceneDependencies) {
        // See `Scene for Box<T>`: forwarding `&Box<T>` would resolve to the blanket impl again.
        SceneListBox::register_dependencies_box(&**self, dependencies);
    }
}

// -----------------------------------------------------------------------------
// EntityScene

/// A single [`Scene`](crate::Scene) used where a [`SceneList`](crate::SceneList) is expected.
///
/// This is the wrapper that turns "one entity" into "a list of one entity", which is what a composed
/// scene needs when it is written into a list of siblings.
pub struct EntityScene<S>(pub S);

impl<S: Scene> SceneList for EntityScene<S> {
    fn resolve_list(
        self,
        context: &mut ResolveContext,
        scenes: &mut Vec<ResolvedScene>,
    ) -> ZlimResult<()> {
        let mut scene = ResolvedScene::new();
        self.0.resolve(context, &mut scene)?;
        scenes.push(scene);
        Ok(())
    }

    #[inline]
    fn register_dependencies(&self, dependencies: &mut SceneDependencies) {
        self.0.register_dependencies(dependencies);
    }
}

// -----------------------------------------------------------------------------
// Blanket implementations

impl<S: Scene> Scene for Option<S> {
    #[inline]
    fn resolve(self, context: &mut ResolveContext, scene: &mut ResolvedScene) -> ZlimResult<()> {
        match self {
            Some(value) => value.resolve(context, scene),
            None => Ok(()),
        }
    }

    #[inline]
    fn register_dependencies(&self, dependencies: &mut SceneDependencies) {
        if let Some(value) = self {
            value.register_dependencies(dependencies);
        }
    }
}

impl<L: SceneList> SceneList for Option<L> {
    #[inline]
    fn resolve_list(
        self,
        context: &mut ResolveContext,
        scenes: &mut Vec<ResolvedScene>,
    ) -> ZlimResult<()> {
        match self {
            Some(value) => value.resolve_list(context, scenes),
            None => Ok(()),
        }
    }

    #[inline]
    fn register_dependencies(&self, dependencies: &mut SceneDependencies) {
        if let Some(value) = self {
            value.register_dependencies(dependencies);
        }
    }
}

impl<S: Scene> SceneList for Vec<S> {
    fn resolve_list(
        self,
        context: &mut ResolveContext,
        scenes: &mut Vec<ResolvedScene>,
    ) -> ZlimResult<()> {
        for scene in self {
            let mut resolved = ResolvedScene::new();
            scene.resolve(context, &mut resolved)?;
            scenes.push(resolved);
        }
        Ok(())
    }

    fn register_dependencies(&self, dependencies: &mut SceneDependencies) {
        for scene in self {
            scene.register_dependencies(dependencies);
        }
    }
}

impl SceneList for Vec<Box<dyn SceneList>> {
    fn resolve_list(
        self,
        context: &mut ResolveContext,
        scenes: &mut Vec<ResolvedScene>,
    ) -> ZlimResult<()> {
        for list in self {
            list.resolve_list(context, scenes)?;
        }
        Ok(())
    }

    fn register_dependencies(&self, dependencies: &mut SceneDependencies) {
        for list in self {
            list.register_dependencies(dependencies);
        }
    }
}
