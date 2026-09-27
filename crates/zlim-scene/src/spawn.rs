//! Queueing a scene: the components that ask for one, and the entry points that write them.
//!
//! This is the half of the asset story a caller touches; the `plugin` module is the job that answers
//! what is queued here.

use zlim_asset::assets::Assets;
use zlim_asset::handle::Handle;
use zlim_asset::server::AssetServer;
use zlim_core::component::Component;
use zlim_core::entity::EntityId;
use zlim_core::error::{ZlimError, ZlimResult};
use zlim_core::world::World;
use zlim_reflect::TypePath;

use crate::patch::{SceneListPatch, ScenePatch};
use crate::scene::Scene;
use crate::scene_list::SceneList;

// -----------------------------------------------------------------------------
// ScenePatchInstance

/// Asks for the patch at [`ScenePatchInstance::handle`] to be applied to this entity.
///
/// The patch is applied by the [`SpawnScene`] job once it and everything it depends on is loaded and
/// resolved — which is what lets a scene be named now and built later: the entity exists right away
/// (so it can be pointed at), and its components arrive when the asset does. The component is
/// removed once the scene has been applied.
///
/// The scene is applied *to* the entity, as [`ScenePatch::apply`] does, rather than spawned as a new
/// one: the entity is the root the scene describes.
///
/// [`SpawnScene`]: zlim_app::SpawnScene
#[derive(Component, TypePath, Clone, Debug)]
pub struct ScenePatchInstance {
    /// The patch to apply.
    pub handle: Handle<ScenePatch>,
}

impl ScenePatchInstance {
    /// Creates a request that applies `handle` to the entity it is added to.
    #[inline]
    pub fn new(handle: Handle<ScenePatch>) -> Self {
        Self { handle }
    }

    /// Returns the patch this asks for.
    #[inline]
    pub fn handle(&self) -> &Handle<ScenePatch> {
        &self.handle
    }
}

// -----------------------------------------------------------------------------
// SceneListPatchInstance

/// Asks for the list patch at [`SceneListPatchInstance::handle`] to be spawned under this entity.
///
/// Like [`ScenePatchInstance`], but for a [`SceneListPatch`]: the entities it describes are spawned
/// as children of the entity that holds this component, once the list is loaded and resolved. The
/// entity itself only serves as the parent — the list does not describe it — so this component is
/// normally added to an empty entity created for the purpose.
#[derive(Component, TypePath, Clone, Debug)]
pub struct SceneListPatchInstance {
    /// The list patch to spawn.
    pub handle: Handle<SceneListPatch>,
}

impl SceneListPatchInstance {
    /// Creates a request that spawns `handle` under the entity it is added to.
    #[inline]
    pub fn new(handle: Handle<SceneListPatch>) -> Self {
        Self { handle }
    }

    /// Returns the list patch this asks for.
    #[inline]
    pub fn handle(&self) -> &Handle<SceneListPatch> {
        &self.handle
    }
}

// -----------------------------------------------------------------------------
// WorldSceneQueueExt

/// The scene entry points that go through the asset system.
///
/// A queued scene is a [`ScenePatch`] added to the asset system, plus a [`ScenePatchInstance`] on the
/// entity it belongs to: the patch holds the description and starts its dependencies loading, and the
/// [`SpawnScene`] job applies it once everything is there. This is the form to reach for when a scene
/// is named before it can be built — a level that streams in, or one that comes from a file that is
/// still being read.
///
/// [`WorldSceneExt`](crate::WorldSceneExt) is the immediate counterpart, for a description that is
/// already in hand.
///
/// [`SpawnScene`]: zlim_app::SpawnScene
pub trait WorldSceneQueueExt {
    /// Adds a patch for `scene`, and queues it to be applied to the entity `target`.
    ///
    /// The patch starts loading what the description depends on
    /// ([`Scene::register_dependencies`]), and is resolved and applied by the next [`SpawnScene`]
    /// run that finds it ready. `target` is not spawned by this call: it is the entity the scene
    /// describes, so it has to exist.
    ///
    /// # Errors
    ///
    /// Returns an error if `target` is not a spawned entity, or if the world has no patch collection
    /// ([`ScenePlugin`](crate::ScenePlugin) registers one).
    ///
    /// [`Scene::register_dependencies`]: crate::Scene::register_dependencies
    /// [`SpawnScene`]: zlim_app::SpawnScene
    fn queue_apply_scene(&mut self, scene: impl Scene, target: EntityId) -> ZlimResult<()>;

    /// Adds a patch for `scene`, spawns an empty entity under `parent`, and queues the patch to be
    /// applied to it.
    ///
    /// The entity exists as soon as this returns, so it can be pointed at before the scene is built.
    ///
    /// # Panics
    ///
    /// Panics if `parent` is `Some` but not spawned.
    ///
    /// # Errors
    ///
    /// Returns an error if the world has no patch collection
    /// ([`ScenePlugin`](crate::ScenePlugin) registers one).
    fn queue_spawn_scene(
        &mut self,
        scene: impl Scene,
        parent: Option<EntityId>,
    ) -> ZlimResult<EntityId>;

    /// Adds a patch for `list`, spawns an empty entity under `parent`, and queues the list to be
    /// spawned under it.
    ///
    /// Returns the entity the list will be spawned under: like
    /// [`queue_spawn_scene`](Self::queue_spawn_scene), it exists before the list does.
    ///
    /// # Panics
    ///
    /// Panics if `parent` is `Some` but not spawned.
    ///
    /// # Errors
    ///
    /// Returns an error if the world has no patch collection
    /// ([`ScenePlugin`](crate::ScenePlugin) registers one).
    fn queue_spawn_scene_list(
        &mut self,
        list: impl SceneList,
        parent: Option<EntityId>,
    ) -> ZlimResult<EntityId>;
}

impl WorldSceneQueueExt for World {
    fn queue_apply_scene(&mut self, scene: impl Scene, target: EntityId) -> ZlimResult<()> {
        let handle = add_patch(self, Box::new(scene))?;
        let mut entity = self.get_entity_owned(target)?;
        entity.insert(ScenePatchInstance::new(handle))?;
        Ok(())
    }

    fn queue_spawn_scene(
        &mut self,
        scene: impl Scene,
        parent: Option<EntityId>,
    ) -> ZlimResult<EntityId> {
        let handle = add_patch(self, Box::new(scene))?;
        let mut entity = self.spawn_empty(parent);
        entity.insert(ScenePatchInstance::new(handle))?;
        Ok(entity.id())
    }

    fn queue_spawn_scene_list(
        &mut self,
        list: impl SceneList,
        parent: Option<EntityId>,
    ) -> ZlimResult<EntityId> {
        let handle = add_list_patch(self, Box::new(list))?;
        let mut entity = self.spawn_empty(parent);
        entity.insert(SceneListPatchInstance::new(handle))?;
        Ok(entity.id())
    }
}

// -----------------------------------------------------------------------------
// Adding the patch

/// Adds `scene` as a patch, starting the loads it asks for.
fn add_patch(world: &mut World, scene: Box<dyn Scene>) -> ZlimResult<Handle<ScenePatch>> {
    let patch = {
        let server = world.get_resource::<AssetServer>();
        ScenePatch::load_boxed(server, scene)
    };

    let mut patches = world
        .get_resource_mut::<Assets<ScenePatch>>()
        .ok_or_else(missing_patches)?;

    Ok(patches.add(patch))
}

/// Adds `list` as a list patch, starting the loads it asks for.
fn add_list_patch(
    world: &mut World,
    list: Box<dyn SceneList>,
) -> ZlimResult<Handle<SceneListPatch>> {
    let patch = {
        let server = world.get_resource::<AssetServer>();
        SceneListPatch::load_boxed(server, list)
    };

    let mut patches = world
        .get_resource_mut::<Assets<SceneListPatch>>()
        .ok_or_else(missing_list_patches)?;

    Ok(patches.add(patch))
}

/// The error of queueing a scene in a world that has no patch collection.
pub(crate) fn missing_patches() -> ZlimError {
    ZlimError::error(
        "this world has no `Assets<ScenePatch>`: add `ScenePlugin` (after `AssetPlugin`) to give it \
         one",
    )
}

/// The error of queueing a scene list in a world that has no list patch collection.
pub(crate) fn missing_list_patches() -> ZlimError {
    ZlimError::error(
        "this world has no `Assets<SceneListPatch>`: add `ScenePlugin` (after `AssetPlugin`) to give \
         it one",
    )
}
