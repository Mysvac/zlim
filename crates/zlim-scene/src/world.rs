//! The scene entry points of a [`World`]: resolving a description, and building it.
//!
//! The `apply` module is what happens once a description has been resolved, and `spawn` is where the
//! deferred form — the requests the asset system answers later — is written.
//!
//! Everything here defers to one of those two, and nothing here builds anything itself: the traits
//! are the shapes a caller reaches for, over the machinery the other modules hold.

use zlim_asset::assets::Assets;
use zlim_asset::server::AssetServer;
use zlim_core::command::{Command, Commands, EntityCommands};
use zlim_core::entity::EntityId;
use zlim_core::error::{ZlimError, ZlimResult};
use zlim_core::world::World;

use crate::patch::ScenePatch;
use crate::resolved::ResolvedScene;
use crate::scene::{ResolveContext, Scene};
use crate::scene_list::SceneList;
use crate::spawn::{SceneListPatchInstance, ScenePatchInstance};
use crate::spawn::{SceneQueue, add_list_patch, add_patch};

// -----------------------------------------------------------------------------
// WorldSceneExt

/// The scene entry points of a [`World`].
///
/// The first three build a description that is already in hand — the shorthand for the two steps of
/// resolving and then applying. The rest queue one instead: the `spawn` module is where those are
/// implemented, since answering them is the asset system's affair.
///
/// A description is resolved against the patches of the world, when it has any, so a scene that
/// includes a cached patch resolves like it would on the asset side.
///
/// # Queued scenes
///
/// The `queue_*` entry points hand a description to the asset system and leave it to be built later,
/// by the [`SpawnScene`] job. That is the form to reach for when a scene is named before it can be
/// built — a level that streams in, or one that comes from a file that is still being read.
///
/// What waits is the *description*: the patch resolves once the assets it depends on are loaded, and
/// the entities a scene describes are built then. The entity a request *belongs to* does not wait, and
/// is not something the job creates:
///
/// - [`queue_spawn_scene`](Self::queue_spawn_scene) spawns its entity on the spot, before queueing —
///   so the entity exists right away, with nothing in it yet, which is what a caller that wants to
///   point at it before the scene arrives is after. The id is not handed back, because the entity is
///   the scene's and a scene naming its own entity is
///   [`ResolvedScene::id`](crate::ResolvedScene::id)'s business. Use [`spawn_scene`](Self::spawn_scene)
///   to have the id.
/// - [`queue_spawn_scene_list`](Self::queue_spawn_scene_list) spawns nothing: a list describes
///   entities of its own, so the entity it names is a parent, and it is only ever named rather than
///   made.
/// - [`queue_apply_scene`](Self::queue_apply_scene) names an entity that already exists.
///
/// Whatever entity a request names is checked when the call is made, so a call that names one the
/// world does not have fails on the spot. What that cannot cover is the wait for the assets: an
/// entity that is despawned before the scene is ready has no request left to answer, which is
/// reported through [`zlim_log::warn`] then — the request is dropped, except for a list whose parent
/// is gone, which is spawned as roots instead.
///
/// [`SpawnScene`]: zlim_app::SpawnScene
pub trait WorldSceneExt {
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

    /// Resolves `scene` and spawns an entity for it under `parent`, together with its children.
    ///
    /// # Errors
    ///
    /// Returns an error if `parent` is `Some` but not spawned, or if the scene cannot be resolved or
    /// built. Nothing is left behind: the entity the call spawned is dropped again.
    fn spawn_scene(&mut self, scene: impl Scene, parent: Option<EntityId>) -> ZlimResult<EntityId>;

    /// Resolves `list` and spawns one entity per scene of it under `parent`.
    ///
    /// The roots share one name scope, so a `#Name` declared by one of them resolves for the others
    /// — which is what lets sibling scenes of a list refer to each other.
    ///
    /// # Errors
    ///
    /// Returns an error if `parent` is `Some` but not spawned, or if the list cannot be resolved or
    /// built. A list is spawned all at once or not at all: no root of a failed list is left behind.
    fn spawn_scene_list(
        &mut self,
        list: impl SceneList,
        parent: Option<EntityId>,
    ) -> ZlimResult<Vec<EntityId>>;

    /// Adds `scene` as a patch, queues it to be applied to the entity `target` once it is ready, and
    /// returns.
    ///
    /// The *scene* is what is deferred: the patch starts loading what the description depends on
    /// ([`Scene::register_dependencies`]), and the next [`SpawnScene`] run that finds it ready
    /// resolves it and applies it to `target`. `target` itself is checked now, so a call that names
    /// an entity the world does not have fails rather than queueing a request nothing can answer.
    /// What that check cannot cover is the wait: an entity that is despawned before the scene is
    /// ready is reported through [`zlim_log::warn`] when the request is answered, and dropped.
    ///
    /// # Errors
    ///
    /// Returns an error if `target` is not a spawned entity, or if the world has no patch collection
    /// ([`ScenePlugin`](crate::ScenePlugin) registers one).
    ///
    /// [`Scene::register_dependencies`]: crate::Scene::register_dependencies
    fn queue_apply_scene(&mut self, scene: impl Scene, target: EntityId) -> ZlimResult<()>;

    /// Spawns an empty entity under `parent`, adds `scene` as a patch, and queues it to be applied to
    /// that entity once it is ready.
    ///
    /// The entity is made **now**, under `parent`, and is empty until the scene arrives: that is what
    /// lets a caller point at it — through a `#Name` a later scene declares, or as a parent for
    /// another spawn — before anything is in it. What waits is the scene, which the next
    /// [`SpawnScene`] run that finds its assets loaded resolves and applies to the entity that is
    /// already there.
    ///
    /// `parent` is checked when this is called, so a parent the world does not have is refused rather
    /// than waited on. What that cannot cover is the wait itself: either entity may be despawned
    /// before the scene is ready, which is reported through [`zlim_log::warn`] when the request is
    /// answered. A parent that is gone takes the empty entity with it, so such a request is dropped;
    /// one whose own entity is gone is dropped too.
    ///
    /// # Errors
    ///
    /// Returns an error if `parent` is `Some` but not spawned, or if the world has no patch
    /// collection ([`ScenePlugin`](crate::ScenePlugin) registers one).
    ///
    /// [`SpawnScene`]: zlim_app::SpawnScene
    fn queue_spawn_scene(&mut self, scene: impl Scene, parent: Option<EntityId>) -> ZlimResult<()>;

    /// Adds `list` as a list patch and queues it to be spawned under `parent` once it is ready, and
    /// returns.
    ///
    /// Unlike [`queue_spawn_scene`](Self::queue_spawn_scene), nothing is spawned now. A list describes
    /// entities of its own, so the entity this names is a *parent* for them rather than a place to put
    /// the list: there would be nothing for an entity made here to be. The entities the list describes
    /// are all created by the [`SpawnScene`] run that finds the patch ready, under `parent`.
    ///
    /// `parent` is checked when this is called. What that cannot cover is the wait: a parent that is
    /// gone by the time the list is ready is reported through [`zlim_log::warn`], and the list is
    /// spawned as roots instead — a list describes roots anyway, so it does not need one.
    ///
    /// # Errors
    ///
    /// Returns an error if `parent` is `Some` but not spawned, or if the world has no list patch
    /// collection ([`ScenePlugin`](crate::ScenePlugin) registers one).
    ///
    /// [`SpawnScene`]: zlim_app::SpawnScene
    fn queue_spawn_scene_list(
        &mut self,
        list: impl SceneList,
        parent: Option<EntityId>,
    ) -> ZlimResult<()>;
}

// -----------------------------------------------------------------------------
// The immediate form

impl WorldSceneExt for World {
    #[inline]
    fn apply_scene(&mut self, scene: impl Scene, target: EntityId) -> ZlimResult<()> {
        let resolved = resolve_scene(self, scene)?;
        let mut entity = self.get_entity_owned(target)?;
        ResolvedScene::apply(&resolved, &mut entity)
    }

    #[inline]
    fn spawn_scene(&mut self, scene: impl Scene, parent: Option<EntityId>) -> ZlimResult<EntityId> {
        let resolved = resolve_scene(self, scene)?;
        ResolvedScene::spawn(&resolved, self, parent)
    }

    #[inline]
    fn spawn_scene_list(
        &mut self,
        list: impl SceneList,
        parent: Option<EntityId>,
    ) -> ZlimResult<Vec<EntityId>> {
        let scenes = resolve_scene_list(self, list)?;
        ResolvedScene::spawn_batch(&scenes, self, parent)
    }

    #[inline]
    fn queue_apply_scene(&mut self, scene: impl Scene, target: EntityId) -> ZlimResult<()> {
        #[inline(never)]
        fn internal(world: &mut World, scene: Box<dyn Scene>, target: EntityId) -> ZlimResult<()> {
            world.entities().get(target).map_err(ZlimError::warning)?;
            let handle = add_patch(world, scene)?;
            queue(world).push_scene(Some(target), ScenePatchInstance::new(handle));
            Ok(())
        }

        internal(self, Box::new(scene), target)
    }

    #[inline]
    fn queue_spawn_scene(&mut self, scene: impl Scene, parent: Option<EntityId>) -> ZlimResult<()> {
        #[inline(never)]
        fn internal(
            world: &mut World,
            scene: Box<dyn Scene>,
            parent: Option<EntityId>,
        ) -> ZlimResult<()> {
            let handle = add_patch(world, scene)?;
            let target = world.try_spawn_empty(parent)?.id();
            queue(world).push_scene(Some(target), ScenePatchInstance::new(handle));
            Ok(())
        }

        internal(self, Box::new(scene), parent)
    }

    #[inline]
    fn queue_spawn_scene_list(
        &mut self,
        list: impl SceneList,
        parent: Option<EntityId>,
    ) -> ZlimResult<()> {
        #[inline(never)]
        fn internal(
            world: &mut World,
            list: Box<dyn SceneList>,
            parent: Option<EntityId>,
        ) -> ZlimResult<()> {
            if let Some(p) = parent {
                world.entities().get(p).map_err(ZlimError::warning)?;
            }
            let handle = add_list_patch(world, list)?;
            queue(world).push_list(parent, SceneListPatchInstance::new(handle));
            Ok(())
        }

        internal(self, Box::new(list), parent)
    }
}

/// Returns the queue of `world`, registering it if this is the first request.
#[inline]
fn queue(world: &mut World) -> &mut SceneQueue {
    world.resource_mut_or_init::<SceneQueue>().into_inner()
}

// -----------------------------------------------------------------------------
// CommandsSceneExt

/// The scene entry points of [`Commands`], for a description a system does not have yet.
///
/// A command is deferred twice over: the [`SpawnScene`] job already waits for the scene's assets, and
/// a command waits for the schedule to reach the end of its systems before that job is even asked. So
/// nothing here is checked up front — not `parent`, not the scene — and nothing returns an error.
///
/// The `try_` forms are the ones that do not report a `parent` which is gone by the time the command
/// runs; they are the counterpart of [`Commands::try_spawn_empty`], and like it they still defer
/// everything else. A parent that is named but never spawned is reported by the command queue, which
/// is where the rest of the command API reports it.
///
/// The entity a scene describes does not exist until the command runs, so [`spawn_scene`] hands back
/// the [`EntityCommands`] of the entity that *will* exist. Its id is allocated now, which is what
/// makes it usable as a parent for further commands.
///
/// [`SpawnScene`]: zlim_app::SpawnScene
/// [`spawn_scene`]: Self::spawn_scene
pub trait CommandsSceneExt {
    /// Queues `scene` to be spawned under `parent`.
    fn spawn_scene(&mut self, scene: impl Scene, parent: Option<EntityId>) -> EntityCommands<'_>;

    /// Queues `scene` to be spawned under `parent`, without reporting a parent that is gone.
    fn try_spawn_scene(
        &mut self,
        scene: impl Scene,
        parent: Option<EntityId>,
    ) -> EntityCommands<'_>;

    /// Queues `list` to be spawned under `parent`.
    ///
    /// Nothing is handed back: a list describes entities of its own, so there is no single one to
    /// address. Queue one scene with [`spawn_scene`](Self::spawn_scene) to get an [`EntityCommands`]
    /// to hang further commands on.
    fn spawn_scene_list(&mut self, list: impl SceneList, parent: Option<EntityId>);

    /// Queues `list` to be spawned under `parent`, without reporting a parent that is gone.
    fn try_spawn_scene_list(&mut self, list: impl SceneList, parent: Option<EntityId>);
}

#[track_caller]
#[inline(never)]
fn queue_scene(cmd: &mut Commands, qs: QueueScene) {
    cmd.queue(qs);
}

#[track_caller]
#[inline(never)]
fn queue_scene_silenced(cmd: &mut Commands, qs: QueueScene) {
    cmd.queue_silenced(qs);
}

#[track_caller]
#[inline(never)]
fn queue_scene_list(cmd: &mut Commands, qsl: QueueSceneList) {
    cmd.queue(qsl);
}

#[track_caller]
#[inline(never)]
fn queue_scene_list_silenced(cmd: &mut Commands, qsl: QueueSceneList) {
    cmd.queue_silenced(qsl);
}

impl CommandsSceneExt for Commands<'_, '_> {
    #[inline]
    #[track_caller]
    fn spawn_scene(&mut self, scene: impl Scene, parent: Option<EntityId>) -> EntityCommands<'_> {
        let scene = Box::new(scene);
        let target = self.spawn_empty(parent).id();
        queue_scene(self, QueueScene { scene, target });
        self.with_entity(target)
    }

    #[inline]
    #[track_caller]
    fn try_spawn_scene(
        &mut self,
        scene: impl Scene,
        parent: Option<EntityId>,
    ) -> EntityCommands<'_> {
        let scene = Box::new(scene);
        let target = self.try_spawn_empty(parent).id();
        queue_scene_silenced(self, QueueScene { scene, target });
        self.with_entity(target)
    }

    #[inline]
    #[track_caller]
    fn spawn_scene_list(&mut self, list: impl SceneList, parent: Option<EntityId>) {
        let list = Box::new(list);
        queue_scene_list(self, QueueSceneList { list, parent });
    }

    #[inline]
    #[track_caller]
    fn try_spawn_scene_list(&mut self, list: impl SceneList, parent: Option<EntityId>) {
        let list = Box::new(list);
        queue_scene_list_silenced(self, QueueSceneList { list, parent });
    }
}

// -----------------------------------------------------------------------------
// EntityCommandsExt

/// The scene entry points of an [`EntityCommands`], for describing an entity that already exists.
///
/// This is the deferred counterpart of [`WorldSceneExt::apply_scene`]: the scene is applied *to* the
/// entity rather than spawned as a new one, and — like every scene command — it happens once the
/// schedule lets the queue run, not when it is written.
///
/// Each method takes and returns the value, so they chain:
///
/// ```ignore
/// commands
///     .with_entity(target)
///     .apply_scene(Health { current: 5 })
///     .insert(Tag);
/// ```
///
/// The `try_` form is the one that does not report an entity which is gone by the time the command
/// runs; the entity is looked up then, not now.
pub trait EntityCommandsExt: Sized {
    /// Queues `scene` to be applied to this entity.
    fn apply_scene(self, scene: impl Scene) -> Self;

    /// Queues `scene` to be applied to this entity, without reporting an entity that is gone.
    fn try_apply_scene(self, scene: impl Scene) -> Self;
}

impl EntityCommandsExt for EntityCommands<'_> {
    #[inline]
    #[track_caller]
    fn apply_scene(mut self, scene: impl Scene) -> Self {
        let scene = Box::new(scene);
        let target = self.id();
        queue_scene(&mut self.commands(), QueueScene { scene, target });
        self
    }

    #[inline]
    #[track_caller]
    fn try_apply_scene(mut self, scene: impl Scene) -> Self {
        let scene = Box::new(scene);
        let target = self.id();
        queue_scene_silenced(&mut self.commands(), QueueScene { scene, target });
        self
    }
}

// -----------------------------------------------------------------------------
// QueueScene

/// A scene waiting to be turned into a patch, as a command.
///
/// The *target* is carried rather than looked up from the command queue, which is what lets a scene
/// be queued for an entity that does not exist yet: by the time this runs, the `spawn_empty` queued
/// ahead of it has made it. It is also what lets the same command describe an entity that already
/// exists, which is [`EntityCommandsExt::apply_scene`].
///
/// It only *queues* the scene — it does not resolve or build it, which is the job's affair — so it
/// cannot fail. A world without the patch collections is one where the queueing call had nothing to
/// add to, and where the run that would build it will not find it either.
struct QueueScene {
    /// The description to queue.
    scene: Box<dyn Scene>,

    /// The entity the scene describes.
    target: EntityId,
}

/// A scene list waiting to be turned into a patch, as a command.
///
/// The list-shaped counterpart of [`QueueScene`], a type of its own because a list is not a scene and
/// a command holds one or the other, never both. Its entity is a *parent* rather than a target: the
/// entities of the list are spawned under it, and it is not one of them.
struct QueueSceneList {
    /// The description to queue.
    list: Box<dyn SceneList>,

    /// The entity the list is spawned under, if it names one.
    parent: Option<EntityId>,
}

impl Command for QueueScene {
    type Output = ();

    fn apply(self, world: &mut World) {
        let Ok(handle) = add_patch(world, self.scene) else {
            return;
        };

        queue(world).push_scene(Some(self.target), ScenePatchInstance::new(handle));
    }
}

impl Command for QueueSceneList {
    type Output = ();

    fn apply(self, world: &mut World) {
        let Ok(handle) = add_list_patch(world, self.list) else {
            return;
        };

        queue(world).push_list(self.parent, SceneListPatchInstance::new(handle));
    }
}

// -----------------------------------------------------------------------------
// Resolution

/// Resolves `scene` against the world, without building it.
pub(crate) fn resolve_scene(world: &World, scene: impl Scene) -> ZlimResult<ResolvedScene> {
    let mut resolved = ResolvedScene::new();
    let mut context = resolve_context(world);
    scene.resolve(&mut context, &mut resolved)?;
    Ok(resolved)
}

/// Resolves `list` against the world, without building it.
pub(crate) fn resolve_scene_list(
    world: &World,
    list: impl SceneList,
) -> ZlimResult<Vec<ResolvedScene>> {
    let mut scenes = Vec::new();
    let mut context = resolve_context(world);
    list.resolve_list(&mut context, &mut scenes)?;
    Ok(scenes)
}

/// Returns the resolution context of `world`: its patches, and its asset server when it has one.
pub(crate) fn resolve_context(world: &World) -> ResolveContext<'_> {
    let Some(assets) = world.get_resource::<Assets<ScenePatch>>() else {
        return ResolveContext::new();
    };

    match world.get_resource::<AssetServer>() {
        Some(server) => ResolveContext::with_server(server, assets),
        None => ResolveContext::with_assets(assets),
    }
}
