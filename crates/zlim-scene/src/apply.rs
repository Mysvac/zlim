//! Applying a [`ResolvedScene`]: spawning the entities, writing the templates, and connecting the
//! hierarchy.

use core::any::{Any, TypeId};
use std::sync::Arc;

use zlim_core::bundle::BundleScratch;
use zlim_core::entity::{EntityId, EntityMap, EntityMapper};
use zlim_core::error::{ZlimError, ZlimResult};
use zlim_core::ops::EntityOwned;
use zlim_core::template::{EntityReferences, EntityTemplate};
use zlim_core::template::{ErasedTemplate, Template, TemplateContext};
use zlim_core::world::World;
use zlim_utils::hash::{HashSet, NoopState};

use crate::resolved::ResolvedScene;

// -----------------------------------------------------------------------------
// Layers

/// The template types a layer does not have to apply.
type TypeSet = HashSet<TypeId, NoopState>;

/// Where the scene of a layer lives.
///
/// A scene of the tree being applied is borrowed; a scene inside a
/// cached patch is owned through the handle the layer that includes
/// it holds, and reached by walking a path from that patch's root.
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
    skip: TypeSet,
}

/// One entity of an application, and the layers that describe it.
struct Planned<'a> {
    /// The entity the layers are applied to.
    entity: EntityId,

    /// Whether the entity was created for this application.
    ///
    /// It is one fact with two consequences: the entity has no place in the tree the rest of the
    /// world has seen, so moving it needs no signal — and it is the application's to drop when a
    /// template fails.
    created: bool,

    /// The layers in application order: a cached scene (and what it includes, and so on) first, then
    /// the scene that includes it.
    layers: Vec<Layer<'a>>,
}

/// Appends the layers of `base` to `out`, deepest first: whatever `base` builds on comes before it.
///
/// `skip` is the set of types the scene that includes `base` cloned from it, so `base` does not have
/// to apply them.
fn push_layers<'a>(base: Source<'a>, skip: TypeSet, out: &mut Vec<Layer<'a>>) {
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

// -----------------------------------------------------------------------------
// The record of an application

/// The entities an application has spawned, in the order it spawned them.
///
/// An application fails as a whole — a template that cannot be built leaves the entity it was
/// building half described — so what it created is dropped rather than left behind. Only the
/// entities are recorded: the name scope and the id map are made by the application itself, so
/// dropping them is all it takes to leave them as they were.
///
/// A parent is pushed before its children, so dropping the list in reverse drops a child before the
/// parent that owns it.
type Spawned = Vec<EntityId>;

// -----------------------------------------------------------------------------
// Applying a tree

/// Spawns one entity per scene of the tree, and binds every name the scenes declare — and every id
/// they carry — to the entity that declares them.
///
/// This runs before any component is built, so that a template may mention a name that is declared
/// later in the scene: by the time templates are built, every name of the whole tree is bound. The
/// same holds for the ids a document gives its entities, which is what lets the components read from
/// that document name each other.
///
/// The root is the application's to drop only when it created it: a scene applied to an entity that
/// already existed keeps that entity, and only the children spawned under it are taken back.
fn place<'a>(
    entity: EntityId,
    created: bool,
    source: Source<'a>,
    world: &mut World,
    references: &mut EntityReferences,
    entities: &mut EntityMap<EntityId>,
    spawned: &mut Spawned,
    plan: &mut Vec<Planned<'a>>,
) {
    let mut layers = Vec::new();
    push_layers(source, TypeSet::with_hasher(NoopState), &mut layers);

    if created {
        spawned.push(entity);
    }

    for layer in &layers {
        let scene = layer.source.get();

        for reference in scene.entity_references() {
            references.set(*reference, entity);
        }

        // The id the document gives this entity is bound here, for the same reason the names are: a
        // component of one entity may point at another that the document declares later.
        if let EntityTemplate::Entity(id) = scene.id() {
            entities.set_mapped(id, entity);
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
        place(
            child,
            true,
            child_source,
            world,
            references,
            entities,
            spawned,
            plan,
        );
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
    entities: &mut EntityMap<EntityId>,
    scratch: &mut BundleScratch,
) -> ZlimResult<()> {
    for planned in plan {
        let target = planned.entity;

        let result = entity.world_scope(|world| -> ZlimResult<()> {
            let mut owned = world.get_entity_owned(target).map_err(ZlimError::error)?;

            {
                let mut writer = scratch.writer();
                let mut context = TemplateContext::new(&mut owned, references, entities);

                for layer in &planned.layers {
                    let scene = layer.source.get();

                    for template in scene.templates() {
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

            // The last layer that carries an edge wins: the scene is applied on top of the cached one,
            // so what it says about the hierarchy comes last. A layer that says nothing — `None` —
            // is skipped rather than taken as an answer, so it does not cancel an earlier layer's.
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
            // names may be declared anywhere in the tree. An edge that names no entity resolves to no
            // entity, which is the request to move to the root.
            let parent = match parent {
                EntityTemplate::None => None,
                parent => {
                    let mut context = TemplateContext::new(&mut owned, references, entities);
                    Some(parent.build_template(&mut context)?)
                }
            };

            // An entity this application created has no place in the tree that the rest of the world
            // has seen, so its move needs no signal; one that already existed has, so it does.
            if planned.created {
                owned
                    .reparent_without_signal(parent)
                    .map_err(ZlimError::error)?;
            } else {
                owned.reparent(parent).map_err(ZlimError::error)?;
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
    /// Applies this scene to `entity`, without the bookkeeping its callers do.
    ///
    /// The name scope and the id map are made here and die here, so a failed application only has
    /// the world to put back: the entities it spawned are all of it.
    ///
    /// `created` says whether `entity` was spawned for this application, which decides both how its
    /// parent edge is applied and whether it is dropped again when a template fails.
    ///
    /// `spawned` collects what the application creates, so that a caller which writes several roots
    /// at once can take the whole lot back.
    fn apply_internal(
        &self,
        entity: &mut EntityOwned<'_>,
        spawned: &mut Spawned,
        created: bool,
    ) -> ZlimResult<()> {
        // The name scope and the id map belong to this application alone: nothing outside reads
        // them, so a failure leaves the world as the only thing to put back.
        let mut references = EntityReferences::new();
        let mut entities = EntityMap::new();
        let mut scratch = BundleScratch::new();
        let start = spawned.len();

        // --- 1 & 2: every entity, every name, and every document id ---
        let mut plan = Vec::new();
        let root = entity.id();
        entity.world_scope(|world| {
            place(
                root,
                created,
                Source::Own(self),
                world,
                &mut references,
                &mut entities,
                spawned,
                &mut plan,
            )
        });

        // --- 3 & 4: the components, then the parent edges ---
        fill(&plan, entity, &mut references, &mut entities, &mut scratch).inspect_err(move |_| {
            ::core::hint::cold_path();
            entity.world_scope(|world| {
                for &id in spawned[start..].iter().rev() {
                    world.try_despawn(id);
                }
            });
        })
    }

    /// Applies this scene to an entity that is already part of the world.
    ///
    /// The entity is described as it is, so its own parent edge — if the scene carries one — is
    /// applied with the signalling [`reparent`]: moving something that is already placed in the tree
    /// is what propagation has to hear about. Use [`spawn`](Self::spawn) for an entity that does not
    /// exist yet.
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
    /// The name scope is the scene's own, so a `#Name` it carries has to be declared by the scene
    /// itself or by a cached scene it builds on. Use [`spawn_batch`](Self::spawn_batch) to apply
    /// several scenes under one scope.
    ///
    /// # Errors
    ///
    /// Returns an error if a template cannot be built, in which case everything this application
    /// created is dropped. The entity itself is not dropped — it was already part of the world — but
    /// everything the application spawned under it is.
    ///
    /// [`reparent`]: zlim_core::ops::EntityOwned::reparent
    /// [`ReparentSignal`]: zlim_core::message::ReparentSignal
    /// [`BundleWriter`]: zlim_core::bundle::BundleWriter
    #[inline(never)]
    pub fn apply(&self, entity: &mut EntityOwned<'_>) -> ZlimResult<()> {
        let mut spawned = Vec::with_capacity(self.children().len());

        // The entity is the caller's, and it is still holding it: only what the application spawns
        // under it is recorded, so a failure leaves the root alone.
        self.apply_internal(entity, &mut spawned, false)
    }

    /// Spawns an entity under `parent` and applies this scene to it.
    ///
    /// # Errors
    ///
    /// Returns [`EntityError`](zlim_core::entity::EntityError) if `parent` is `Some` but not spawned,
    /// and an error if a template cannot be built. In either case nothing is left behind: the entity
    /// this method spawned is dropped again, because the caller only learns of the failure through
    /// the error and would have no way to name it.
    #[inline(never)]
    pub fn spawn(&self, world: &mut World, parent: Option<EntityId>) -> ZlimResult<EntityId> {
        let id = world.try_spawn_empty(parent)?.id();
        let mut spawned = Vec::with_capacity(1 + self.children().len());

        // The handle is released before the application runs: a failed application drops this entity
        // with the rest, and a handle that outlived it would point at a slot that is gone.
        let mut entity = world.entity_owned(id);
        self.apply_internal(&mut entity, &mut spawned, true)?;

        Ok(id)
    }

    /// Spawns one entity per scene of `scenes` under `parent`, sharing one name scope.
    ///
    /// Every root is spawned, and every name and id of the list is bound, before any of them is
    /// written: the roots share one scope, so one may point at a name another declares, in either
    /// order.
    ///
    /// # Errors
    ///
    /// Returns [`EntityError`](zlim_core::entity::EntityError) if `parent` is `Some` but not spawned,
    /// or an error if applying a scene to one of the new entities fails.
    ///
    /// A batch is applied all at once or not at all: the roots may point at each other, so a root
    /// that was already written is not a usable half of the list — no root of a failed batch is left
    /// under `parent`, nor any of their children. A root that another entity already owned is not
    /// touched; only what the batch itself spawned goes.
    #[inline(never)]
    pub fn spawn_batch(
        scenes: &[Self],
        world: &mut World,
        parent: Option<EntityId>,
    ) -> ZlimResult<Vec<EntityId>> {
        // The name scope and the id map belong to the batch, and are shared by every root of it:
        // that is what lets one root point at a name or an id another declares. A failure drops them
        // with the entities they were bound to.
        let mut references = EntityReferences::new();
        let mut entities = EntityMap::new();
        let mut scratch = BundleScratch::new();
        let mut plan = Vec::new();
        let mut idents = Vec::with_capacity(scenes.len());
        let mut ranges = Vec::with_capacity(scenes.len());

        let hint = scenes.iter().map(|scene| 1 + scene.children().len()).sum();
        let mut spawned = Spawned::with_capacity(hint);

        // Placing first is what lets one root of the list point at another: the ids and names of the
        // whole list are bound before any of them is built, in either order.
        for scene in scenes {
            // A root the world does not accept fails the whole batch: it is the parent that is
            // wrong, so the next root would be refused in exactly the same way.
            let mut entity = world.try_spawn_empty(parent)?;
            let start = plan.len();
            let root = entity.id();
            entity.world_scope(|world| {
                place(
                    root,
                    true,
                    Source::Own(scene),
                    world,
                    &mut references,
                    &mut entities,
                    &mut spawned,
                    &mut plan,
                )
            });
            idents.push(root);
            ranges.push(start..plan.len());
        }

        for (id, range) in idents.iter().zip(&ranges) {
            let result = match world.get_entity_owned(*id) {
                Ok(mut entity) => fill(
                    &plan[range.clone()],
                    &mut entity,
                    &mut references,
                    &mut entities,
                    &mut scratch,
                ),
                Err(error) => Err(error.into()),
            };

            if let Err(error) = result {
                // Nothing holds a handle to the roots: the caller learns of the failure only through
                // the error, so the whole batch goes, rather than the half of it that worked.
                for &id in spawned.iter().rev() {
                    world.try_despawn(id);
                }
                return Err(error);
            }
        }

        Ok(idents)
    }
}
