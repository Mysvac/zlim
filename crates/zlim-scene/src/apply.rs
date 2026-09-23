//! Applying a [`ResolvedScene`]: spawning the entities, writing the templates, and connecting the
//! hierarchy.

use core::any::{Any, TypeId};
use std::sync::Arc;

use zlim_core::bundle::BundleScratch;
use zlim_core::entity::EntityId;
use zlim_core::error::{ZlimError, ZlimResult};
use zlim_core::ops::EntityOwned;
use zlim_core::template::{EntityReferences, ErasedTemplate, Template, TemplateContext};
use zlim_core::world::World;
use zlim_utils::hash::HashSet;

use crate::resolved::ResolvedScene;

// -----------------------------------------------------------------------------
// Layers

/// Where the scene of a layer lives.
///
/// A scene of the tree being applied is borrowed; a scene inside a cached patch is owned through the
/// handle the layer that includes it holds, and reached by walking a path from that patch's root.
enum Source<'a> {
    /// A scene of the tree being applied.
    Own(&'a ResolvedScene),

    /// A scene of a cached patch: the resolved root, and the children to walk down from it.
    Cached {
        /// The resolved root of the cached patch.
        root: Arc<ResolvedScene>,

        /// The child indices leading from `root` to this scene.
        path: Vec<usize>,
    },
}

impl<'a> Source<'a> {
    /// Returns the scene this source stands for.
    fn get(&self) -> &ResolvedScene {
        match self {
            Self::Own(scene) => scene,
            Self::Cached { root, path } => {
                let mut scene: &ResolvedScene = root;
                for &index in path {
                    scene = &scene.children()[index];
                }
                scene
            }
        }
    }

    /// Returns the source of the `index`th child of this scene.
    fn child(&self, index: usize) -> Source<'a> {
        match self {
            Self::Own(scene) => Source::Own(&scene.children()[index]),
            Self::Cached { root, path } => {
                let mut path = path.clone();
                path.push(index);
                Source::Cached {
                    root: Arc::clone(root),
                    path,
                }
            }
        }
    }
}

/// One scene sharing an entity with the layers around it.
struct Layer<'a> {
    /// The scene this layer contributes from.
    source: Source<'a>,

    /// The template types a layer *above* this one describes.
    ///
    /// Those are exactly the templates this layer contributed that the scene above cloned to edit —
    /// so it applies its own copy, and this one is skipped instead of being built and written only
    /// to be overwritten by the copy derived from it.
    skip: HashSet<TypeId>,
}

/// One entity of an application, and the layers that describe it.
struct Planned<'a> {
    /// The entity the layers are applied to.
    entity: EntityId,

    /// Whether the entity was created for this application.
    created: bool,

    /// The layers in application order: a cached scene (and what it includes, and so on) first, then
    /// the scene that includes it.
    layers: Vec<Layer<'a>>,
}

/// Appends the layers of `base` to `out`, deepest first: whatever `base` builds on comes before it.
///
/// `skip` is the set of types the scene that includes `base` cloned from it, so `base` does not have
/// to apply them.
fn push_layers<'a>(base: Source<'a>, skip: HashSet<TypeId>, out: &mut Vec<Layer<'a>>) {
    if let Some(cached) = base.get().cached_info() {
        push_layers(
            Source::Cached {
                root: Arc::clone(cached.resolved()),
                path: Vec::new(),
            },
            cached.duplicate_templates().clone(),
            out,
        );
    }

    out.push(Layer { source: base, skip });
}

/// Spawns one entity per scene of the tree, and binds every name the scenes declare to the entity
/// that declares them.
///
/// This runs before any component is built, so that a template may mention a name that is declared
/// later in the scene: by the time templates are built, every name of the whole tree is bound.
fn place<'a>(
    entity: EntityId,
    created: bool,
    source: Source<'a>,
    world: &mut World,
    references: &mut EntityReferences,
    plan: &mut Vec<Planned<'a>>,
) {
    let mut layers = Vec::new();
    push_layers(source, HashSet::new(), &mut layers);

    for layer in &layers {
        for reference in layer.source.get().entity_references() {
            references.set(*reference, entity);
        }
    }

    // Children come from every layer, in layer order: a cached scene's children
    // are spawned before the children of the scene that builds on it.
    let mut children = Vec::new();
    for layer in &layers {
        let scene = layer.source.get();
        for index in 0..scene.children().len() {
            children.push(layer.source.child(index));
        }
    }

    plan.push(Planned {
        entity,
        created,
        layers,
    });

    for child_source in children {
        let child = world.spawn_empty(Some(entity)).id();
        place(child, true, child_source, world, references, plan);
    }
}

/// Writes the components and the parent edge of everything `place` planned.
///
/// `entity` is any live handle of the same world — the plans address entities by id, so the caller
/// only needs a way to reach the world.
fn fill(
    plan: &[Planned<'_>],
    entity: &mut EntityOwned<'_>,
    references: &mut EntityReferences,
    scratch: &mut BundleScratch,
) -> ZlimResult<()> {
    for planned in plan {
        let target = planned.entity;
        let created = planned.created;

        let result = entity.world_scope(|world| -> ZlimResult<()> {
            let mut owned = world.get_entity_owned(target).map_err(ZlimError::error)?;

            {
                let mut writer = scratch.writer();
                let mut context = TemplateContext::new(&mut owned, references);

                for layer in &planned.layers {
                    let scene = layer.source.get();

                    for template in scene.component_templates() {
                        let template: &dyn ErasedTemplate = &**template;
                        if layer.skip.contains(&(template as &dyn Any).type_id()) {
                            continue;
                        }
                        template.apply(&mut context, &mut writer)?;
                    }
                }

                writer
                    .write(&mut *context.entity)
                    .map_err(ZlimError::error)?;
            }

            // The last layer that carries an edge wins: the scene is applied on
            // top of the cached one, so what it says about the hierarchy comes last.
            let mut edge = None;
            for layer in planned.layers.iter().rev() {
                if let Some(parent) = layer.source.get().parent() {
                    edge = Some(parent);
                    break;
                }
            }

            let Some(parent) = edge else {
                return Ok(());
            };

            // The edge is resolved here, and not while the scene was resolved, because the entity it
            // names may be declared anywhere in the tree.
            let parent = {
                let mut context = TemplateContext::new(&mut owned, references);
                parent.build_template(&mut context)?
            };

            if created {
                owned
                    .reparent_without_signal(Some(parent))
                    .map_err(ZlimError::error)?;
            } else {
                owned.reparent(Some(parent)).map_err(ZlimError::error)?;
            }

            Ok(())
        });

        // A template that failed partway leaves what it pushed in the scratch space, where nothing
        // would ever drop it: the write that would have handed it over never happened.
        if result.is_err() && !scratch.is_empty() {
            scratch.manual_drop(entity.world().components());
        }

        result?;
    }

    Ok(())
}

// -----------------------------------------------------------------------------
// Apply

impl ResolvedScene {
    /// Spawns the entities of this scene — and of the cached scenes it builds on — and binds every
    /// name they declare.
    ///
    /// This is the first half of applying a scene, and separable from the second so that a whole
    /// list of roots can be spawned and named before any of them is written: the roots of a list
    /// share one name scope, so one of them may point at a name another declares, in either order.
    fn place<'a>(
        &'a self,
        entity: &mut EntityOwned<'_>,
        references: &mut EntityReferences,
        created: bool,
        plan: &mut Vec<Planned<'a>>,
    ) {
        let root = entity.id();
        entity
            .world_scope(|world| place(root, created, Source::Own(self), world, references, plan));
    }

    /// Applies this scene to `entity`, without the bookkeeping its callers do.
    ///
    /// [`WorldSceneExt::apply_scene`] is the only caller from outside: it describes an entity that
    /// already exists, so the parent edge it carries has to signal.
    ///
    /// [`WorldSceneExt::apply_scene`]: crate::WorldSceneExt::apply_scene
    pub(crate) fn apply_internal(
        &self,
        entity: &mut EntityOwned<'_>,
        references: &mut EntityReferences,
        scratch: &mut BundleScratch,
        created: bool,
    ) -> ZlimResult<()> {
        // --- 1 & 2: every entity, and every name ---
        let mut plan = Vec::new();
        self.place(entity, references, created, &mut plan);

        // --- 3 & 4: the components, then the parent edges ---
        fill(&plan, entity, references, scratch)
    }

    /// Applies this scene to `entity`, using the given name scope and scratch space.
    ///
    /// This is the form to reach for when several roots are applied as one scene: they share
    /// `references` — so a `#Name` of one resolves for the others — and they reuse `scratch`, whose
    /// arena is only reset between writes. The entity is taken to have been spawned for the scene, so
    /// a parent edge it carries is applied with [`reparent_without_signal`] rather than
    /// [`reparent`]: the move is not something the rest of the world has seen yet, so there is no
    /// [`ReparentSignal`] to send.
    ///
    /// # What an application does
    ///
    /// 1. Every entity of the tree is spawned: the root is `entity`, and every child scene is
    ///    spawned under the entity that describes it.
    /// 2. Every name the tree declares is bound to the entity that declares it.
    /// 3. Every entity gets its templates, written in one go through a [`BundleWriter`]: a component
    ///    template pushes one column, a bundle template hands over everything it carries.
    /// 4. Every explicit parent edge is applied, after which the hierarchy is final.
    ///
    /// The order of 1 and 3 is what makes forward references work: the entities exist, and their
    /// names are bound, before any template asks for them.
    ///
    /// A scene that builds on a cached one applies that scene first — its templates, skipping the
    /// ones this scene replaced, and its children before this scene's own.
    ///
    /// # What it does not do
    ///
    /// A scene carries data and structure, so nothing runs for it after its components are written,
    /// exactly as with [`EntityOwned::insert`]. Readiness is not announced either: zlim has
    /// no observers yet, so the `Ready` event of Bevy's scenes has no counterpart here.
    ///
    /// [`reparent_without_signal`]: zlim_core::ops::EntityOwned::reparent_without_signal
    /// [`reparent`]: zlim_core::ops::EntityOwned::reparent
    /// [`ReparentSignal`]: zlim_core::message::ReparentSignal
    /// [`BundleWriter`]: zlim_core::bundle::BundleWriter
    pub fn apply_with(
        &self,
        entity: &mut EntityOwned<'_>,
        references: &mut EntityReferences,
        scratch: &mut BundleScratch,
    ) -> ZlimResult<()> {
        self.apply_internal(entity, references, scratch, true)
    }

    /// Applies this scene to an entity that is already part of the world.
    ///
    /// The entity is described as it is, so its own parent edge — if the scene carries one — is
    /// applied with the signalling [`reparent`]. Use [`spawn`](Self::spawn) for an entity that does
    /// not exist yet.
    ///
    /// [`reparent`]: zlim_core::ops::EntityOwned::reparent
    pub fn apply(&self, entity: &mut EntityOwned<'_>) -> ZlimResult<()> {
        let mut references = EntityReferences::new();
        let mut scratch = BundleScratch::new();
        self.apply_internal(entity, &mut references, &mut scratch, false)
    }

    /// Spawns an entity under `parent` and applies this scene to it.
    ///
    /// # Panics
    ///
    /// Panics if `parent` is `Some` but not spawned.
    pub fn spawn<'w>(
        &self,
        world: &'w mut World,
        parent: Option<EntityId>,
    ) -> ZlimResult<EntityOwned<'w>> {
        let mut entity = world.spawn_empty(parent);
        let mut references = EntityReferences::new();
        let mut scratch = BundleScratch::new();
        self.apply_internal(&mut entity, &mut references, &mut scratch, true)?;
        Ok(entity)
    }
}

// -----------------------------------------------------------------------------
// Applying a list

/// Spawns one entity per scene of `scenes` under `parent`, sharing one name scope.
///
/// The scenes are spawned and their names bound all at once, and only then written, so a `#Name`
/// declared by one root resolves for the others — in either order. This is what
/// [`SceneListPatch`](crate::SceneListPatch) spawns through, and what
/// [`WorldSceneExt::spawn_scene_list`] resolves into.
///
/// # Panics
///
/// Panics if `parent` is `Some` but not spawned.
///
/// # Errors
///
/// Returns an error if `parent` stops existing, or if applying a scene to one of the new entities
/// fails.
///
/// [`WorldSceneExt::spawn_scene_list`]: crate::WorldSceneExt::spawn_scene_list
pub(crate) fn spawn_resolved(
    world: &mut World,
    scenes: &[ResolvedScene],
    parent: Option<EntityId>,
) -> ZlimResult<Vec<EntityId>> {
    let mut references = EntityReferences::new();
    let mut scratch = BundleScratch::new();
    let mut plan = Vec::new();
    let mut ids = Vec::with_capacity(scenes.len());
    let mut ranges = Vec::with_capacity(scenes.len());

    // Every root is spawned, and every name of the list is bound, before any of them is written.
    // The roots share one scope, so one may point at a name another declares — in either order.
    for scene in scenes {
        let mut entity = world.spawn_empty(parent);
        let start = plan.len();
        scene.place(&mut entity, &mut references, true, &mut plan);
        ids.push(entity.id());
        ranges.push(start..plan.len());
    }

    for (id, range) in ids.iter().zip(&ranges) {
        let mut entity = world.get_entity_owned(*id).map_err(ZlimError::error)?;
        fill(
            &plan[range.clone()],
            &mut entity,
            &mut references,
            &mut scratch,
        )?;
    }

    Ok(ids)
}
