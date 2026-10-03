//! The scene plugin: the job that builds what was queued, and the plugin that installs it.
//!
//! Nothing here is called by hand — [`WorldSceneQueueExt`](crate::WorldSceneQueueExt) writes the
//! requests, and this module answers them.

use std::sync::Arc;

use zlim_app::{App, MainSchedulePlugin, Plugin, PluginExt, SpawnScene};
use zlim_asset::asset::Asset;
use zlim_asset::assets::Assets;
use zlim_asset::handle::{ErasedHandle, Handle};
use zlim_asset::ident::AssetId;
use zlim_asset::plugin::{AppAssetExt, AssetPlugin};
use zlim_asset::server::AssetServer;
use zlim_core::derive::job_fn;
use zlim_core::entity::EntityId;
use zlim_core::query::Query;
use zlim_core::world::World;

use crate::patch::{SceneListPatch, ScenePatch};
use crate::resolved::ResolvedScene;
use crate::spawn::{SceneListPatchInstance, ScenePatchInstance, missing_patches};

// -----------------------------------------------------------------------------
// HandleSceneSpawn

/// Reports whether there is anything queued, so that the job is skipped when there is not.
fn scene_spawn_condition(
    q1: Query<&ScenePatchInstance>,
    q2: Query<&SceneListPatchInstance>,
) -> bool {
    !q1.is_empty() || !q2.is_empty()
}

/// The job that builds what was queued: it resolves the patches it finds ready, and applies them.
///
/// It runs in the [`SpawnScene`] schedule, between `Update` and `PostUpdate`, so a scene queued this
/// frame is in the world before transform propagation sees the hierarchy.
///
/// A patch whose dependencies are still loading is left alone; the next run picks it up. A patch that
/// cannot be resolved keeps its request, and the failure is reported through [`zlim_log::error`]
/// rather than silently dropped — the request stays, so it either succeeds once the asset side
/// changes, or keeps reporting.
///
/// The requests are collected before any of them is applied, so a scene that queues another one —
/// one that writes a [`ScenePatchInstance`] of its own — has that request answered by a later run.
#[job_fn(type = HandleSceneSpawn, run_if = scene_spawn_condition)]
fn handle_scene_spawn(world: &mut World) {
    // The requests are collected first: applying a scene moves entities around, and resolution takes
    // the patch out of its collection for the duration.
    let requests: Vec<(EntityId, Handle<ScenePatch>)> = world
        .query::<(EntityId, &ScenePatchInstance), ()>()
        .iter()
        .map(|(entity, instance)| (entity, instance.handle.clone()))
        .collect();

    for (entity, handle) in requests {
        if !resolve_patch(world, &handle) {
            continue;
        }

        let Some(resolved) = resolved_patch(world, &handle) else {
            continue;
        };

        let Ok(mut entity) = world.get_entity_owned(entity) else {
            // The entity was despawned between the queueing and this run.
            continue;
        };

        if let Err(error) = entity.remove::<ScenePatchInstance>() {
            zlim_log::error!("Failed to remove a queued scene request: {error}");
        }

        if let Err(error) = resolved.apply(&mut entity) {
            zlim_log::error!("Failed to apply a queued scene: {error}");
            continue;
        }
    }

    let requests: Vec<(EntityId, Handle<SceneListPatch>)> = world
        .query::<(EntityId, &SceneListPatchInstance), ()>()
        .iter()
        .map(|(entity, instance)| (entity, instance.handle.clone()))
        .collect();

    for (entity, handle) in requests {
        if !resolve_list_patch(world, &handle) {
            continue;
        }

        let Some(resolved) = resolved_list_patch(world, &handle) else {
            continue;
        };

        let Ok(mut parent) = world.get_entity_owned(entity) else {
            continue;
        };

        if let Err(error) = parent.remove::<SceneListPatchInstance>() {
            zlim_log::error!("Failed to remove a queued scene list request: {error}");
        }

        let parent = parent.id();

        if let Err(error) = ResolvedScene::spawn_batch(&resolved, world, Some(parent)) {
            zlim_log::error!("Failed to spawn a queued scene list: {error}");
        }
    }
}

// -----------------------------------------------------------------------------
// Resolving what is queued

/// Resolves the patch at `handle` if its dependencies are in, and reports whether it is resolved now.
///
/// An unresolved patch is put back where it was: the request that names it is only answered by a
/// resolved patch, and resolution may work on the next run once the dependencies have arrived.
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
            .unwrap_or_else(|| Err(missing_patches()));

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
            .unwrap_or_else(|| Err(missing_patches()));

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
// ScenePlugin

/// Registers the scene assets, and the job that builds what they queue.
///
/// Add it after [`AssetPlugin`], which provides the storage the patches
/// go into and the server whose loads they await.
///
/// # Panics
///
/// Panics during [`Plugin::apply`] if [`AssetPlugin`] is missing.
#[derive(Debug, Default)]
pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&mut self, app: &mut App) {
        MainSchedulePlugin::apply_before::<Self>(app);
        AssetPlugin::apply_before::<Self>(app);
    }

    fn apply(&mut self, app: &mut App) {
        MainSchedulePlugin::warn_if_unset(app, "ScenePlugin");

        app.init_asset::<ScenePatch>();
        app.init_asset::<SceneListPatch>();
        app.add_job::<HandleSceneSpawn>(SpawnScene, ());
    }
}
