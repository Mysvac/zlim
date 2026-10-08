//! Queueing a scene: the requests, the queue they wait in, and what turns a description into one.
//!
//! This is the half of the asset story a caller touches; the `plugin` module is the job that answers
//! what is queued here. The entry points that write the queue are the `queue_*` methods of
//! [`WorldSceneExt`](crate::WorldSceneExt).
//!
//! A request is not a component of an entity. What a queued scene describes may be an entity that
//! does not exist yet, so there is nothing to attach a component to, and the requests live in one
//! [`SceneQueue`] resource instead. That also keeps the queue away from the archetypes: a world that
//! queues scenes does not grow a table for them.
//!
//! The entity a request *belongs to* is a different matter, and whether it exists yet depends on the
//! entry point: a scene spawned through [`WorldSceneExt`](crate::WorldSceneExt) is given an empty
//! entity on the spot, so there is something to point at while its components are still loading,
//! while a list is only ever given the parent its entities will be spawned under.

use std::sync::Arc;

use zlim_asset::asset::Asset;
use zlim_asset::assets::Assets;
use zlim_asset::handle::{ErasedHandle, Handle};
use zlim_asset::ident::AssetId;
use zlim_asset::server::AssetServer;
use zlim_core::borrow::Res;
use zlim_core::derive::job_fn;
use zlim_core::entity::EntityId;
use zlim_core::resource::Resource;
use zlim_core::system::If;
use zlim_core::world::World;
use zlim_error::{ZlimError, ZlimResult};
use zlim_reflect::TypePath;

use crate::patch::{SceneListPatch, ScenePatch};
use crate::resolved::ResolvedScene;
use crate::scene::Scene;
use crate::scene_list::SceneList;

// -----------------------------------------------------------------------------
// ScenePatchInstance
// -----------------------------------------------------------------------------

/// A scene waiting to be built.
///
/// The patch is applied by the [`SpawnScene`] job once it and everything it depends on is loaded and
/// resolved — which is what lets a scene be named now and built later: its entity is created when the
/// job runs, and the components arrive when the asset does.
///
/// [`SpawnScene`]: zlim_app::SpawnScene
#[derive(Clone, Debug)]
pub struct ScenePatchInstance {
    /// The patch to apply.
    pub handle: Handle<ScenePatch>,
}

impl ScenePatchInstance {
    /// Creates a request for `handle`.
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
// -----------------------------------------------------------------------------

/// A scene list waiting to be built.
///
/// Like [`ScenePatchInstance`], but for a [`SceneListPatch`]: the entities it describes are spawned
/// as children of the entity the request names, which only serves as the parent — the list does not
/// describe it.
#[derive(Clone, Debug)]
pub struct SceneListPatchInstance {
    /// The list patch to spawn.
    pub handle: Handle<SceneListPatch>,
}

impl SceneListPatchInstance {
    /// Creates a request for `handle`.
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
// SceneQueue

/// The scenes waiting to be built, in the order they were queued.
///
/// A queued scene is not a component of an entity: a scene that has not been built yet describes an
/// entity that does not exist, so there is nothing for a component to be attached to. The requests
/// live here instead, and the entities are made when the [`SpawnScene`] job answers them.
///
/// Each entry is a request and the entity it belongs to: the entity a scene is applied to, the parent
/// a list is spawned under, or `None` when the job is to create one.
///
/// The job takes both lists with [`core::mem::take`], so a scene that queues another one — which the
/// scene it applies may well do — has that request answered by a later run rather than while the
/// queue is being walked.
///
/// [`SpawnScene`]: zlim_app::SpawnScene
#[derive(Resource, Default)]
pub struct SceneQueue {
    /// The scene requests and the entities they are applied to, in the order they were queued.
    pub(crate) scenes: Vec<(Option<EntityId>, ScenePatchInstance)>,

    /// The scene list requests and the parents they are spawned under, in queue order.
    pub(crate) lists: Vec<(Option<EntityId>, SceneListPatchInstance)>,
}

impl SceneQueue {
    /// Queues `scene` to be applied to `target`, or to be spawned when it names none.
    #[inline]
    pub(crate) fn push_scene(&mut self, target: Option<EntityId>, scene: ScenePatchInstance) {
        self.scenes.push((target, scene));
    }

    /// Queues `list` to be spawned under `parent`, or as roots when it names none.
    #[inline]
    pub(crate) fn push_list(&mut self, parent: Option<EntityId>, list: SceneListPatchInstance) {
        self.lists.push((parent, list));
    }

    /// Returns whether nothing is waiting to be built.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.scenes.is_empty() && self.lists.is_empty()
    }
}

// -----------------------------------------------------------------------------
// HandleSceneSpawn

/// Reports whether anything is queued, so that the job is skipped when there is not.
fn scene_spawn_condition(queue: If<Res<SceneQueue>>) -> bool {
    !queue.is_empty()
}

/// The job that builds what was queued: it resolves the patches it finds ready, and applies them.
///
/// It runs in the `SpawnScene` schedule, between `Update` and `PostUpdate`, so a scene queued this
/// frame is in the world before transform propagation sees the hierarchy.
///
/// The whole queue is taken before any of it is answered — [`core::mem::take`], so a scene that
/// queues another one has that request answered by a later run rather than while this one is walking
/// the list.
///
/// A request whose patch is not resolved yet goes back into the queue, behind anything that was
/// queued while this run was working: what it waits for is an asset, and waiting is the point. A
/// request whose entity is gone cannot be answered ever, so it is reported and dropped instead.
#[job_fn(type = HandleSceneSpawn, run_if = scene_spawn_condition)]
fn handle_scene_spawn(world: &mut World) {
    let Some(mut queue) = world.get_resource_mut::<SceneQueue>() else {
        return;
    };

    let scenes = core::mem::take(&mut queue.scenes);
    let lists = core::mem::take(&mut queue.lists);
    let mut pending = SceneQueue::default();

    for (target, request) in scenes {
        if !resolve_patch(world, request.handle()) {
            pending.push_scene(target, request);
            continue;
        }

        let Some(resolved) = resolved_patch(world, request.handle()) else {
            // Refused by the collection it lives in, which is worse than not being ready yet.
            zlim_log::warn!("a queued scene is not in its collection; dropping the request");
            continue;
        };

        match target {
            Some(target) => {
                let Ok(mut entity) = world.get_entity_owned(target) else {
                    zlim_log::debug!(
                        "a queued scene names an entity `{target}` that is gone; dropping the request"
                    );
                    continue;
                };

                if let Err(error) = resolved.apply(&mut entity) {
                    zlim_log::error!("Failed to apply a queued scene: {error}");
                }
            }
            None => {
                if let Err(error) = ResolvedScene::spawn(&resolved, world, None) {
                    zlim_log::error!("Failed to spawn a queued scene: {error}");
                }
            }
        }
    }

    for (parent, request) in lists {
        if !resolve_list_patch(world, request.handle()) {
            pending.push_list(parent, request);
            continue;
        }

        let Some(resolved) = resolved_list_patch(world, request.handle()) else {
            zlim_log::warn!("a queued scene list is not in its collection; dropping the request");
            continue;
        };

        // A parent that is gone does not take the list with it: a list describes roots, so it is
        // spawned without one.
        let parent = match parent {
            Some(parent) => {
                if world.contains_entity(parent) {
                    Some(parent)
                } else {
                    zlim_log::debug!(
                        "a queued scene list names an entity `{parent}` that is gone; spawning it as roots"
                    );
                    None
                }
            }
            None => None,
        };

        if let Err(error) = ResolvedScene::spawn_batch(&resolved, world, parent) {
            zlim_log::error!("Failed to spawn a queued scene list: {error}");
        }
    }

    if !pending.is_empty() {
        // What is still waiting goes back, ahead of anything queued while this run was working.
        if let Some(mut queue) = world.get_resource_mut::<SceneQueue>() {
            pending.scenes.append(&mut queue.scenes);
            core::mem::swap(&mut queue.scenes, &mut pending.scenes);
            pending.lists.append(&mut queue.lists);
            core::mem::swap(&mut queue.lists, &mut pending.lists);
        }
    }
}

// -----------------------------------------------------------------------------
// Resolving what is queued

/// Resolves the patch at `handle` if its dependencies are in, and reports whether it is resolved now.
///
/// An unresolved patch is put back where it was: resolution may work on the next run once the
/// dependencies have arrived.
fn resolve_patch(world: &mut World, handle: &Handle<ScenePatch>) -> bool {
    let Some(mut patch) = take::<ScenePatch>(world, handle.id()) else {
        return false;
    };

    // Resolution reads the patch collection it lives in (a cached scene has to look the patch it
    // builds on up), which is why the patch is taken out of it for the duration.
    let resolved = if patch.resolved.is_some() {
        true
    } else if !dependencies_loaded(world, patch.dependencies()) {
        false
    } else {
        let result = world
            .try_resource_scope(|world, mut patches| {
                let server = world.get_resource::<AssetServer>();
                patch.resolve(server, &mut patches)
            })
            .unwrap_or_else(|| Err(missing_assets(ScenePatch::IDENT)));

        if let Err(error) = result {
            zlim_log::error!("Failed to resolve a queued scene: {error}");
        }

        patch.resolved.is_some()
    };

    put::<ScenePatch>(world, handle.id(), patch);

    resolved
}

/// Resolves the list patch at `handle` if its dependencies are in, and reports whether it is resolved.
///
/// See [`resolve_patch`] for the take-resolve-put dance.
fn resolve_list_patch(world: &mut World, handle: &Handle<SceneListPatch>) -> bool {
    let Some(mut patch) = take::<SceneListPatch>(world, handle.id()) else {
        return false;
    };

    let resolved = if patch.resolved.is_some() {
        true
    } else if !dependencies_loaded(world, patch.dependencies()) {
        false
    } else {
        let result = world
            .try_resource_scope(|world, mut patches| {
                let server = world.get_resource::<AssetServer>();
                patch.resolve(server, &mut patches)
            })
            .unwrap_or_else(|| Err(missing_assets(SceneListPatch::IDENT)));

        if let Err(error) = result {
            zlim_log::error!("Failed to resolve a queued scene list: {error}");
        }

        patch.resolved.is_some()
    };

    put::<SceneListPatch>(world, handle.id(), patch);

    resolved
}

/// Returns the resolved scene of a patch that is in the collection.
fn resolved_patch(world: &World, handle: &Handle<ScenePatch>) -> Option<Arc<ResolvedScene>> {
    let patches = world.get_resource::<Assets<ScenePatch>>()?;
    patches
        .get(handle.id())
        .and_then(|patch| patch.resolved.clone())
}

/// Returns the resolved scenes of a list patch that is in the collection.
fn resolved_list_patch(
    world: &World,
    handle: &Handle<SceneListPatch>,
) -> Option<Arc<Vec<ResolvedScene>>> {
    let patches = world.get_resource::<Assets<SceneListPatch>>()?;
    patches
        .get(handle.id())
        .and_then(|patch| patch.resolved.clone())
}

/// Reports whether every dependency of the patch is loaded.
///
/// Without an asset server there is nothing to wait for: the description does not name assets that
/// could be loading, so it is taken to be ready.
fn dependencies_loaded(world: &World, dependencies: &[ErasedHandle]) -> bool {
    let Some(server) = world.get_resource::<AssetServer>() else {
        return true;
    };

    dependencies
        .iter()
        .all(|dependency| server.is_loaded(dependency.id()))
}

/// Takes the patch at `id` out of the collection, so that it can be resolved against it.
fn take<A: Asset>(world: &mut World, id: AssetId<A>) -> Option<A> {
    world.get_resource_mut::<Assets<A>>()?.remove(id)
}

/// Puts a resolved patch back into the collection, where it was.
fn put<A: Asset>(world: &mut World, id: AssetId<A>, patch: A) {
    if let Some(mut assets) = world.get_resource_mut::<Assets<A>>() {
        let _ = assets.insert(id, patch);
    }
}

// -----------------------------------------------------------------------------
// Adding a patch

/// Turns `scene` into the patch that describes it, and adds it to the world's collection.
///
/// The patch starts loading what the description depends on as soon as it is made, which is what a
/// queued scene waits for.
pub(crate) fn add_patch(
    world: &mut World,
    scene: Box<dyn Scene>,
) -> ZlimResult<Handle<ScenePatch>> {
    let patch = {
        let server = world.get_resource::<AssetServer>();
        ScenePatch::load_boxed(server, scene)
    };

    let mut patches = world
        .get_resource_mut::<Assets<ScenePatch>>()
        .ok_or_else(|| missing_assets(ScenePatch::IDENT))?;

    Ok(patches.add(patch))
}

/// Turns `list` into the list patch that describes it, and adds it to the world's collection.
pub(crate) fn add_list_patch(
    world: &mut World,
    list: Box<dyn SceneList>,
) -> ZlimResult<Handle<SceneListPatch>> {
    let patch = {
        let server = world.get_resource::<AssetServer>();
        SceneListPatch::load_boxed(server, list)
    };

    let mut patches = world
        .get_resource_mut::<Assets<SceneListPatch>>()
        .ok_or_else(|| missing_assets(SceneListPatch::IDENT))?;

    Ok(patches.add(patch))
}

/// The error of queueing a scene in a world that has no collection for its patch.
///
/// The collection is what [`ScenePlugin`](crate::ScenePlugin) registers, so a world without one has
/// not had the plugin added — or has had it added before the asset plugin it builds on.
#[cold]
#[inline(never)]
pub(crate) fn missing_assets(ident: &str) -> ZlimError {
    ZlimError::error(format!(
        "this world has no `Assets<{ident}>`: add `ScenePlugin` (after `AssetPlugin`) to give it one",
    ))
}
