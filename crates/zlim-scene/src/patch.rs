//! Scene assets: a scene stored in the asset system, resolved once, and applied as often as needed.

use core::any::TypeId;
use std::sync::Arc;

use zlim_asset::assets::Assets;
use zlim_asset::derive::Asset;
use zlim_asset::handle::{ErasedHandle, Handle};
use zlim_asset::path::AssetPath;
use zlim_asset::server::AssetServer;
use zlim_core::entity::EntityId;
use zlim_core::ops::EntityOwned;
use zlim_core::world::World;
use zlim_error::{ZlimError, ZlimResult};
use zlim_reflect::TypePath;

use crate::dependency::SceneDependencies;
use crate::resolved::ResolvedScene;
use crate::scene::{ResolveContext, Scene};
use crate::scene_list::SceneList;

// -----------------------------------------------------------------------------
// ScenePatch

/// An asset that holds a scene, the assets it depends on, and the resolved form of it.
///
/// This is what makes a scene reusable: resolving one costs a walk of the description plus a clone
/// for every template the cached-scene machinery needs, while applying one costs only the writes.
/// Keeping the resolved form also makes a patch possible — a scene that includes this one and
/// replaces some of its templates — which is what copy-on-write in
/// [`ResolvedScene::get_or_init_template`] is for.
#[derive(Asset, TypePath)]
pub struct ScenePatch {
    /// The description, taken out of the patch when it is resolved.
    pub scene: Option<Box<dyn Scene>>,

    /// The assets the description depends on.
    ///
    /// They have to be loaded before the patch resolves, which is what the asset system tracks: an
    /// asset that names its dependencies is only loaded once they are.
    #[asset(dependency)]
    pub dependencies: Vec<ErasedHandle>,

    /// The resolved description, filled in by [`resolve`](Self::resolve).
    pub resolved: Option<Arc<ResolvedScene>>,
}

impl ScenePatch {
    /// Creates a patch for `scene`, without starting any loads.
    ///
    /// A description that names dependencies should use [`load`](Self::load) instead, so that they
    /// are on their way before the patch is resolved.
    #[inline]
    pub fn new(scene: impl Scene) -> Self {
        Self {
            scene: Some(Box::new(scene)),
            dependencies: Vec::new(),
            resolved: None,
        }
    }

    /// Creates a patch for `scene`, and starts loading what it depends on.
    #[inline]
    pub fn load(assets: &AssetServer, scene: impl Scene) -> Self {
        ScenePatch::load_boxed(Some(assets), Box::new(scene))
    }

    /// Creates a patch for `scene`, starting its loads when there is a server to start them with.
    ///
    /// A world without an asset server can still describe a scene, it just cannot load what the
    /// description names.
    #[inline]
    pub fn load_from(assets: Option<&AssetServer>, scene: impl Scene) -> Self {
        ScenePatch::load_boxed(assets, Box::new(scene))
    }

    /// Creates a patch for a boxed `scene`, starting its loads when there is a server to start them with.
    ///
    /// A world without an asset server can still describe a scene, it just cannot load what the
    /// description names.
    #[inline(never)]
    pub fn load_boxed(assets: Option<&AssetServer>, scene: Box<dyn Scene>) -> Self {
        let handles = match assets {
            Some(assets) => {
                let mut dependencies = SceneDependencies::new();
                scene.register_dependencies(&mut dependencies);
                dependencies
                    .into_iter()
                    .map(|dependency| {
                        assets
                            .load_builder()
                            .load_erased(dependency.type_id, dependency.path)
                    })
                    .collect()
            }
            None => Vec::new(),
        };

        Self {
            scene: Some(scene),
            dependencies: handles,
            resolved: None,
        }
    }

    /// Returns the handles of the assets the description depends on.
    #[inline]
    pub fn dependencies(&self) -> &[ErasedHandle] {
        &self.dependencies
    }
}

#[inline(always)]
fn unresolved() -> ZlimError {
    ZlimError::error(
        "this scene patch has not been resolved yet: \
        it is either still loading or was never resolved",
    )
}

impl ScenePatch {
    /// Applies the resolved scene to an entity that already exists.
    ///
    /// # Errors
    ///
    /// Returns an error if the patch has not been resolved yet.
    pub fn apply(&self, entity: &mut EntityOwned<'_>) -> ZlimResult<()> {
        match &self.resolved {
            Some(s) => s.apply(entity),
            None => Err(unresolved()),
        }
    }

    /// Spawns the resolved scene as a new root entity under `parent`.
    ///
    /// # Errors
    ///
    /// Returns [`EntityError`](zlim_core::entity::EntityError) if `parent` is `Some` but not spawned,
    /// and an error if the patch has not been resolved yet.
    pub fn spawn(&self, world: &mut World, parent: Option<EntityId>) -> ZlimResult<EntityId> {
        match &self.resolved {
            Some(s) => s.spawn(world, parent),
            None => Err(unresolved()),
        }
    }

    /// Returns the resolved scene, if the patch has been resolved.
    #[inline]
    pub fn resolved(&self) -> Option<&Arc<ResolvedScene>> {
        self.resolved.as_ref()
    }
}

// -----------------------------------------------------------------------------
// SceneListPatch

/// An asset that holds a scene list, the assets it depends on, and its resolved form.
///
/// The list-shaped counterpart of [`ScenePatch`]: resolving it produces one [`ResolvedScene`] per
/// entity of the list, and spawning it creates them all.
#[derive(Asset, TypePath)]
pub struct SceneListPatch {
    /// The description, taken out of the patch when it is resolved.
    pub scene_list: Option<Box<dyn SceneList>>,

    /// The assets the description depends on.
    #[asset(dependency)]
    pub dependencies: Vec<ErasedHandle>,

    /// The resolved description, filled in by [`resolve`](Self::resolve).
    pub resolved: Option<Arc<Vec<ResolvedScene>>>,
}

impl SceneListPatch {
    /// Creates a patch for `list`, without starting any loads.
    #[inline]
    pub fn new(list: impl SceneList) -> Self {
        Self {
            scene_list: Some(Box::new(list)),
            dependencies: Vec::new(),
            resolved: None,
        }
    }

    /// Creates a patch for `list`, and starts loading what it depends on.
    #[inline]
    pub fn load(assets: &AssetServer, list: impl SceneList) -> Self {
        Self::load_boxed(Some(assets), Box::new(list))
    }

    /// Creates a patch for `list`, starting its loads when there is a server to start them with.
    #[inline]
    pub fn load_from(assets: Option<&AssetServer>, list: impl SceneList) -> Self {
        Self::load_boxed(assets, Box::new(list))
    }

    /// Creates a patch for a boxed `scene list`, starting its loads when there
    /// is a server to start them with.
    ///
    /// A world without an asset server can still describe a scene, it just cannot
    /// load what the description names.
    #[inline(never)]
    pub fn load_boxed(assets: Option<&AssetServer>, list: Box<dyn SceneList>) -> Self {
        let handles = match assets {
            Some(assets) => {
                let mut dependencies = SceneDependencies::new();
                list.register_dependencies(&mut dependencies);
                dependencies
                    .into_iter()
                    .map(|dependency| {
                        assets
                            .load_builder()
                            .load_erased(dependency.type_id, dependency.path)
                    })
                    .collect()
            }
            None => Vec::new(),
        };

        Self {
            scene_list: Some(list),
            dependencies: handles,
            resolved: None,
        }
    }

    /// Returns the handles of the assets the description depends on.
    #[inline]
    pub fn dependencies(&self) -> &[ErasedHandle] {
        &self.dependencies
    }
}

impl SceneListPatch {
    /// Spawns one entity per scene of the list, under `parent`.
    ///
    /// # Errors
    ///
    /// Returns [`EntityError`](zlim_core::entity::EntityError) if `parent` is `Some` but not spawned,
    /// and an error if the patch has not been resolved yet. A list is spawned all at once or not at
    /// all: no root of a failed list is left behind.
    pub fn spawn(&self, world: &mut World, parent: Option<EntityId>) -> ZlimResult<Vec<EntityId>> {
        match &self.resolved {
            Some(s) => {
                let scenes = s.as_slice();
                ResolvedScene::spawn_batch(scenes, world, parent)
            }
            None => Err(ZlimError::error(
                "this scene list patch has not been resolved yet: \
                it is either still loading or was never resolved",
            )),
        }
    }

    /// Returns the resolved scenes, if the patch has been resolved.
    #[inline]
    pub fn resolved(&self) -> Option<&Arc<Vec<ResolvedScene>>> {
        self.resolved.as_ref()
    }
}

// -----------------------------------------------------------------------------
// resolve

/// Resolves the patches the given dependencies name, so that a description which builds on one can
/// read the resolved form of it.
///
/// An asset dependency of a patch is only *loaded* by the time the patch is resolved, and the asset
/// system has no loader for a patch that was built from code: without this, a scene that includes a
/// cached one would find it unresolved. So a patch resolves what it builds on, depth-first — which is
/// also the order [`include_cached`](ResolvedScene::include_cached) needs.
///
/// A patch that is being resolved has already been taken out of `patches`, so a cycle between two
/// patches stops at the second one: the patch that is mid-resolution is not there to be resolved
/// again.
fn resolve_patch_deps(
    dependencies: &[ErasedHandle],
    server: Option<&AssetServer>,
    assets: &mut Assets<ScenePatch>,
) -> ZlimResult<()> {
    for dependency in dependencies {
        if dependency.type_id() != TypeId::of::<ScenePatch>() {
            continue;
        }

        let id = dependency
            .id() // ↓ checked above
            .with_type_debug_checked::<ScenePatch>();

        let Some(mut patch) = assets.remove(id) else {
            continue;
        };

        let result = if patch.resolved.is_some() {
            Ok(())
        } else {
            patch.resolve(server, assets)
        };

        let _ = assets.insert(id, patch);
        result?;
    }

    Ok(())
}

impl ScenePatch {
    /// Resolves the description, and keeps the result for every application.
    ///
    /// `patches` is the collection this patch is part of — resolution may include another patch,
    /// which is resolved first, and which is read *from* this collection — and `assets` is the asset
    /// server, when the description looks an asset up by path.
    ///
    /// A patch is normally taken *out* of its collection while it resolves, because resolution reads
    /// the collection it lives in:
    ///
    /// ```rust, ignore
    /// let mut patch = patches.remove(handle.id()).expect("the patch is loaded");
    /// patch.resolve(Some(&assets), &mut patches)?;
    /// patches.insert(handle.id(), patch).expect("the patch goes back");
    /// ```
    ///
    /// # Errors
    ///
    /// Fails if the description was already resolved, if a patch it builds on cannot be resolved, or
    /// if resolving it fails.
    pub fn resolve(
        &mut self,
        server: Option<&AssetServer>,
        assets: &mut Assets<ScenePatch>,
    ) -> ZlimResult<()> {
        let scene = self
            .scene
            .take()
            .ok_or_else(|| ZlimError::error("this scene patch has already been resolved"))?;

        // A cached scene is read in its resolved form, so what this patch builds on is resolved first.
        if let Err(error) = resolve_patch_deps(&self.dependencies, server, assets) {
            ::core::hint::cold_path();
            self.scene = Some(scene);
            return Err(error);
        }

        let mut context = match server {
            Some(server) => ResolveContext::with_server(server, assets),
            None => ResolveContext::with_assets(assets),
        };

        let mut resolved = ResolvedScene::new();
        scene.resolve(&mut context, &mut resolved)?;
        self.resolved = Some(Arc::new(resolved));

        Ok(())
    }
}

impl SceneListPatch {
    /// Resolves the description, and keeps the result for every application.
    ///
    /// See [`ScenePatch::resolve`] for why a patch is taken out of its collection while it resolves.
    ///
    /// # Errors
    ///
    /// Fails if the description was already resolved, if a patch it builds on cannot be resolved, or
    /// if resolving it fails.
    pub fn resolve(
        &mut self,
        server: Option<&AssetServer>,
        assets: &mut Assets<ScenePatch>,
    ) -> ZlimResult<()> {
        let list = self
            .scene_list
            .take()
            .ok_or_else(|| ZlimError::error("this scene list patch has already been resolved"))?;

        if let Err(error) = resolve_patch_deps(&self.dependencies, server, assets) {
            ::core::hint::cold_path();
            self.scene_list = Some(list);
            return Err(error);
        }

        let mut context = match server {
            Some(server) => ResolveContext::with_server(server, assets),
            None => ResolveContext::with_assets(assets),
        };

        let mut resolved = Vec::new();
        list.resolve_list(&mut context, &mut resolved)?;
        self.resolved = Some(Arc::new(resolved));

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// CachedSceneAsset

/// A scene that builds on the patch at a path.
///
/// This is how a scene includes another one: the patch is resolved once, and every scene that
/// includes it applies the cached form, replacing whatever templates it needs to through
/// [`ResolvedScene::get_or_init_template`].
///
/// Resolving this needs an asset side, because the path has to be turned into a handle:
/// [`ResolveContext::new`] has none, and a scene that includes one fails to resolve with it.
#[derive(Clone, Debug)]
pub struct CachedSceneAsset(pub AssetPath<'static>);

impl CachedSceneAsset {
    /// Creates a scene that builds on the patch at `path`.
    #[inline]
    pub fn new(path: impl Into<AssetPath<'static>>) -> Self {
        Self(path.into())
    }

    /// Returns the path of the patch this builds on.
    #[inline]
    pub fn path(&self) -> &AssetPath<'static> {
        &self.0
    }
}

impl Scene for CachedSceneAsset {
    fn resolve(self, context: &mut ResolveContext, scene: &mut ResolvedScene) -> ZlimResult<()> {
        let Some(server) = context.server() else {
            return Err(ZlimError::error(
                "a scene that includes a cached scene has to be resolved \
                 without an AssetServer in its `ResolveContext`",
            ));
        };
        let Some(assets) = context.assets() else {
            return Err(ZlimError::error(
                "a scene that includes a cached scene has to be resolved \
                 without an Assets<ScenePatch> in its `ResolveContext`",
            ));
        };
        let handle = server
            .get_handle::<ScenePatch>(self.0.clone())
            .ok_or_else(|| {
                let e = format!(
                    "the scene patch '{}' is not known to the asset server",
                    self.0
                );
                ZlimError::error(e)
            })?;

        scene.include_cached(assets, handle)?;

        Ok(())
    }

    fn register_dependencies(&self, dependencies: &mut SceneDependencies) {
        dependencies.register::<ScenePatch>(self.0.clone());
    }
}

// -----------------------------------------------------------------------------
// Handles

/// A handle to a [`ScenePatch`].
pub type ScenePatchHandle = Handle<ScenePatch>;

/// A handle to a [`SceneListPatch`].
pub type SceneListPatchHandle = Handle<SceneListPatch>;

// -----------------------------------------------------------------------------
