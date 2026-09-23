//! The [`Scene`](crate::Scene) trait, the scope a scene is resolved in, and the scene impls that need no
//! module of their own.
//!
//! See the [crate documentation](crate) for what a scene is, how a description is composed out of
//! its parts, and what resolving one produces.

use zlim_asset::assets::Assets;
use zlim_asset::server::AssetServer;
use zlim_core::error::ZlimResult;

use crate::dependency::SceneDependencies;
use crate::patch::ScenePatch;
use crate::resolved::ResolvedScene;
// -----------------------------------------------------------------------------
// ResolveContext

/// The context a [`Scene`](crate::Scene) is resolved with.
///
/// Resolution is mostly about the scene being built: a `#Name` is identified by the macro invocation
/// that produced it (see [`EntityReference`]), and the names a scene declares are recorded on the
/// scene itself. What the context adds is the asset side, which is what lets a scene build on a
/// cached one: a [`ScenePatch`] is an asset, so including one means looking it up in the patches of
/// the world, or in the assets when the scene names it by path.
///
/// [`EntityReference`]: zlim_core::template::EntityReference
#[derive(Default)]
pub struct ResolveContext<'a> {
    /// The asset server, when resolution happens with one in reach.
    assets: Option<&'a AssetServer>,

    /// The patches of the world, when it has any.
    patches: Option<&'a Assets<ScenePatch>>,
}

impl<'a> ResolveContext<'a> {
    /// Creates a context without an asset side.
    ///
    /// A scene that needs one — one that includes a [`CachedSceneAsset`] — fails to resolve; every
    /// other scene resolves as usual, without caching.
    ///
    /// [`CachedSceneAsset`]: crate::CachedSceneAsset
    #[inline]
    pub const fn new() -> Self {
        Self {
            assets: None,
            patches: None,
        }
    }

    /// Creates a context that can include cached scenes, but cannot look an asset up by path.
    #[inline]
    pub const fn with_patches(patches: &'a Assets<ScenePatch>) -> Self {
        Self {
            assets: None,
            patches: Some(patches),
        }
    }

    /// Creates a context with the whole asset side.
    #[inline]
    pub const fn with_assets(assets: &'a AssetServer, patches: &'a Assets<ScenePatch>) -> Self {
        Self {
            assets: Some(assets),
            patches: Some(patches),
        }
    }

    /// Returns the asset server, if the context has one.
    #[inline]
    pub const fn assets(&self) -> Option<&'a AssetServer> {
        self.assets
    }

    /// Returns the patches, if the context has any.
    #[inline]
    pub const fn patches(&self) -> Option<&'a Assets<ScenePatch>> {
        self.patches
    }
}

// -----------------------------------------------------------------------------
// Scene

/// A description of a single entity.
///
/// Resolving a scene adds what it describes to a [`ResolvedScene`]:
/// templates to write to the entity, the entities that belong under it, and the
/// parent edge that puts it in place. A scene is composable — a tuple of scenes
/// is a scene — so a description is built from as many parts as it has.
///
/// [`Scene`] is to an entity what [`SceneList`] is to a list of entities, and the two
/// are separate traits for the same reason [`World::spawn`] and [`World::spawn_batch`]
/// are: they describe different things.
///
/// [`SceneList`]: crate::SceneList
/// [`World::spawn`]: zlim_core::world::World::spawn
/// [`World::spawn_batch`]: zlim_core::world::World::spawn_batch
pub trait Scene: SceneBox {
    /// Adds what this scene describes to `scene`.
    ///
    /// A scene should only add to the [`ResolvedScene`]: it does not know which entity it will be
    /// applied to, and the entity may not exist yet.
    fn resolve(self, context: &mut ResolveContext, scene: &mut ResolvedScene) -> ZlimResult<()>;

    /// Registers every asset this scene needs loaded before it can be resolved.
    ///
    /// The default implementation registers nothing, which is right for a scene whose description
    /// stands on its own.
    #[inline]
    fn register_dependencies(&self, _dependencies: &mut SceneDependencies) {}
}

/// The boxed form of [`Scene`](crate::Scene), which is what makes `Box<dyn Scene>` possible.
///
/// [`Scene::resolve`] consumes `self`, which `Box<dyn Scene>` cannot do — `dyn Scene` is unsized.
/// The way out is for every scene type to also know how to resolve itself as `self: Box<Self>`:
/// this trait has a blanket implementation for every [`Sized`] scene, and [`Box`] then implements
/// [`Scene`](crate::Scene) on top of it.
///
/// Most code never names this trait.
pub trait SceneBox: Send + Sync + 'static {
    /// See [`Scene::resolve`].
    fn resolve_box(
        self: Box<Self>,
        context: &mut ResolveContext,
        scene: &mut ResolvedScene,
    ) -> ZlimResult<()>;

    /// See [`Scene::register_dependencies`].
    fn register_dependencies_box(&self, dependencies: &mut SceneDependencies);
}

impl<S: Scene + Sized> SceneBox for S {
    #[inline]
    fn resolve_box(
        self: Box<Self>,
        context: &mut ResolveContext,
        scene: &mut ResolvedScene,
    ) -> ZlimResult<()> {
        (*self).resolve(context, scene)
    }

    #[inline]
    fn register_dependencies_box(&self, dependencies: &mut SceneDependencies) {
        self.register_dependencies(dependencies);
    }
}

impl<T: ?Sized + SceneBox> Scene for Box<T> {
    #[inline]
    fn resolve(self, context: &mut ResolveContext, scene: &mut ResolvedScene) -> ZlimResult<()> {
        SceneBox::resolve_box(self, context, scene)
    }

    #[inline]
    fn register_dependencies(&self, dependencies: &mut SceneDependencies) {
        // The deref matters: forwarding `&Box<T>` would resolve to the blanket `SceneBox` impl for
        // `Box<T>` again, and the two would call each other forever.
        SceneBox::register_dependencies_box(&**self, dependencies);
    }
}
