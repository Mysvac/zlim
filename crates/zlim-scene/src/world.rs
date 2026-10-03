//! The scene entry points of a [`World`]: resolving and spawning without going through the assets.
//!
//! The `apply` module is what happens once a description has been resolved, and `spawn` is the form
//! that waits for assets.

use zlim_asset::assets::Assets;
use zlim_asset::server::AssetServer;
use zlim_core::entity::EntityId;
use zlim_core::error::ZlimResult;
use zlim_core::world::World;

use crate::patch::ScenePatch;
use crate::resolved::ResolvedScene;
use crate::scene::{ResolveContext, Scene};
use crate::scene_list::SceneList;

// -----------------------------------------------------------------------------
// WorldSceneExt

/// The scene entry points of a [`World`].
///
/// Spawning a scene is a two-step affair — resolve the description, then apply the resolved form —
/// and this is the shorthand for doing both at once. The resolved form can be kept and applied again
/// with [`ResolvedScene::spawn`] and [`ResolvedScene::apply`], which is what a
/// [`ScenePatch`](crate::ScenePatch) does.
///
/// The description is resolved against the patches of the world, when it has any, so a scene that
/// includes a cached patch resolves like it would on the asset side.
pub trait WorldSceneExt {
    /// Resolves `scene` against the patches of this world, when it has any.
    fn resolve_scene(&self, scene: impl Scene) -> ZlimResult<ResolvedScene>;

    /// Resolves `list` against the patches of this world, when it has any.
    fn resolve_scene_list(&self, list: impl SceneList) -> ZlimResult<Vec<ResolvedScene>>;

    /// Resolves `scene` and spawns an entity for it under `parent`, together with its children.
    ///
    /// # Panics
    ///
    /// Panics if `parent` is `Some` but not spawned.
    fn spawn_scene(&mut self, scene: impl Scene, parent: Option<EntityId>) -> ZlimResult<EntityId>;

    /// Resolves `list` and spawns one entity per scene of it under `parent`.
    ///
    /// The roots share one name scope, so a `#Name` declared by one of them resolves for the others
    /// — which is what lets sibling scenes of a list refer to each other.
    ///
    /// # Panics
    ///
    /// Panics if `parent` is `Some` but not spawned.
    fn spawn_scene_list(
        &mut self,
        list: impl SceneList,
        parent: Option<EntityId>,
    ) -> ZlimResult<Vec<EntityId>>;

    /// Resolves `scene` and applies it to the entity `target`, which must already exist.
    ///
    /// This is how a scene patches an entity it did not create: the entity keeps its place in the
    /// world, and a parent edge the scene carries moves it with the signalling [`reparent`].
    ///
    /// # Errors
    ///
    /// Returns an error if `target` is not a spawned entity.
    ///
    /// [`reparent`]: zlim_core::ops::EntityOwned::reparent
    fn apply_scene(&mut self, scene: impl Scene, target: EntityId) -> ZlimResult<()>;
}

impl WorldSceneExt for World {
    fn resolve_scene(&self, scene: impl Scene) -> ZlimResult<ResolvedScene> {
        let mut resolved = ResolvedScene::new();
        let mut context = resolve_context(self);
        scene.resolve(&mut context, &mut resolved)?;
        Ok(resolved)
    }

    fn resolve_scene_list(&self, list: impl SceneList) -> ZlimResult<Vec<ResolvedScene>> {
        let mut scenes = Vec::new();
        let mut context = resolve_context(self);
        list.resolve_list(&mut context, &mut scenes)?;
        Ok(scenes)
    }

    fn spawn_scene(&mut self, scene: impl Scene, parent: Option<EntityId>) -> ZlimResult<EntityId> {
        let resolved = self.resolve_scene(scene)?;
        Ok(ResolvedScene::spawn(&resolved, self, parent)?.id())
    }

    fn spawn_scene_list(
        &mut self,
        list: impl SceneList,
        parent: Option<EntityId>,
    ) -> ZlimResult<Vec<EntityId>> {
        let scenes = self.resolve_scene_list(list)?;
        ResolvedScene::spawn_batch(&scenes, self, parent)
    }

    fn apply_scene(&mut self, scene: impl Scene, target: EntityId) -> ZlimResult<()> {
        let resolved = self.resolve_scene(scene)?;
        let mut entity = self.get_entity_owned(target)?;
        resolved.apply(&mut entity)
    }
}

/// Returns the resolution context of `world`: its patches, and its asset server when it has one.
fn resolve_context(world: &World) -> ResolveContext<'_> {
    let Some(patches) = world.get_resource::<Assets<ScenePatch>>() else {
        return ResolveContext::new();
    };

    match world.get_resource::<AssetServer>() {
        Some(assets) => ResolveContext::with_assets(assets, patches),
        None => ResolveContext::with_patches(patches),
    }
}
