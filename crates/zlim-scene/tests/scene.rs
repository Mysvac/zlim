//! Integration tests for describing, resolving and applying scenes.

use core::any::TypeId;
use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use zlim_asset::assets::Assets;
use zlim_asset::handle::{ErasedHandle, Handle};
use zlim_asset::path::AssetPath;
use zlim_core::bundle::{Bundle, BundleScratch};
use zlim_core::component::Component;
use zlim_core::error::ZlimResult;
use zlim_core::message::{MessageQueue, ReparentSignal};
use zlim_core::template::{EntityReference, EntityReferences, EntityTemplate};
use zlim_core::template::{Template, TemplateContext, TemplateEffect, template};
use zlim_core::world::World;

use zlim_scene::{
    EntityScene, InitTemplate, InsertTemplate, PatchIntoTemplate, PatchTemplate, ResolveContext,
    ResolvedScene, Scene, SceneChildren, SceneDependencies, SceneFunction, SceneList, SceneParent,
    ScenePatch, SceneScope, WorldSceneExt,
};

// -----------------------------------------------------------------------------
// Types

/// A component that a scene describes.
#[derive(Component, Clone, Debug, PartialEq)]
struct Marker(u32);

/// A component that is `Clone + Default`, so it is its own template, and whose `IntoTemplate` comes
/// from the blanket implementation.
#[derive(Component, Clone, Default, Debug, PartialEq)]
struct Scale(f32);

/// A scene that describes one [`Marker`].
struct MarkerScene(u32);

impl Scene for MarkerScene {
    fn resolve(self, _context: &mut ResolveContext, scene: &mut ResolvedScene) -> ZlimResult<()> {
        scene.push_template(template(move |_| Ok(Marker(self.0))));
        Ok(())
    }
}

/// A scene that declares a `#Name`.
struct NameScene(EntityReference);

impl Scene for NameScene {
    fn resolve(self, _context: &mut ResolveContext, scene: &mut ResolvedScene) -> ZlimResult<()> {
        scene.add_entity_reference(self.0);
        Ok(())
    }
}

/// A template that is deliberately not `Clone`, so that it can implement [`Template`] by hand
/// instead of being covered by the blanket implementation for `Clone` types.
#[derive(Default)]
struct Count(u32);

impl Template for Count {
    type Output = Marker;

    fn build_template(&self, _context: &mut TemplateContext) -> ZlimResult<Marker> {
        Ok(Marker(self.0))
    }

    fn clone_template(&self) -> Self {
        Self(self.0)
    }
}

/// A component required by [`NeedsIt`], with a `Default` that is easy to tell apart from zeroed
/// memory.
#[derive(Component, Clone, Debug, PartialEq)]
struct Needed(u32);

impl Default for Needed {
    fn default() -> Self {
        Self(0x5A5A)
    }
}

/// A component that requires [`Needed`], so that a bundle carrying it still initialises both.
#[derive(Component, Clone, Debug, PartialEq)]
#[require(Needed)]
struct NeedsIt(u32);

/// A bundle: several components that a single template produces.
#[derive(Bundle)]
struct Gear {
    marker: Marker,
    scale: Scale,
    needs: NeedsIt,
}

/// A template whose output is a whole bundle rather than one component.
struct GearTemplate;

impl Template for GearTemplate {
    type Output = Gear;

    fn build_template(&self, _context: &mut TemplateContext) -> ZlimResult<Gear> {
        Ok(Gear {
            marker: Marker(9),
            scale: Scale(2.0),
            needs: NeedsIt(1),
        })
    }

    fn clone_template(&self) -> Self {
        Self
    }
}

// -----------------------------------------------------------------------------
// Helpers

/// Applies the component templates of `scene` to a fresh entity, and returns the component of type
/// `T` they left behind.
fn apply_to<T: Component + Clone>(scene: &ResolvedScene) -> T {
    let mut world = World::alloc();
    let mut entity = world.spawn_empty(None);

    {
        let mut references = EntityReferences::new();
        let mut context = TemplateContext::new(&mut entity, &mut references);
        let mut scratch = BundleScratch::default();
        let mut writer = scratch.writer();

        for template in scene.component_templates() {
            template
                .apply(&mut context, &mut writer)
                .expect("the template applies");
        }

        writer
            .write(&mut *context.entity)
            .expect("the entity is live");
    }

    entity
        .get::<T>()
        .cloned()
        .expect("the component was written")
}

/// Applies the component templates of `scene` to a fresh entity, and returns the marker they left
/// behind.
fn apply_to_entity(scene: &ResolvedScene) -> Marker {
    apply_to::<Marker>(scene)
}

/// Returns the number of `ReparentSignal`s the world has written so far.
fn reparent_signals(world: &World) -> usize {
    world.resource::<MessageQueue<ReparentSignal>>().len()
}

// -----------------------------------------------------------------------------
// Effects

/// A template whose output is a bundle writes every component it carries, through the same writer a
/// single-component template writes through — the required components of those components included.
#[test]
fn a_bundle_template_writes_every_component_it_carries() {
    let mut scene = ResolvedScene::new();
    scene.push_template(GearTemplate);

    let mut world = World::alloc();
    let mut entity = world.spawn_empty(None);
    let mut references = EntityReferences::new();
    let mut scratch = BundleScratch::default();

    {
        let mut context = TemplateContext::new(&mut entity, &mut references);
        let mut writer = scratch.writer();

        for template in scene.component_templates() {
            template
                .apply(&mut context, &mut writer)
                .expect("the template applies");
        }

        writer
            .write(&mut *context.entity)
            .expect("the entity is live");
    }

    assert_eq!(entity.get::<Marker>().cloned(), Some(Marker(9)));
    assert_eq!(entity.get::<Scale>().cloned(), Some(Scale(2.0)));
    assert_eq!(entity.get::<NeedsIt>().cloned(), Some(NeedsIt(1)));
    assert_eq!(
        entity.get::<Needed>().cloned(),
        Some(Needed::default()),
        "a component the bundle requires is initialised as well"
    );
}

/// An effect is applied explicitly — [`TemplateEffect::apply`] is deliberately not a method, so that
/// `apply` does not fill the completion of every component there is.
#[test]
fn an_effect_is_applied_explicitly() {
    let mut world = World::alloc();
    let mut entity = world.spawn_empty(None);
    let mut references = EntityReferences::new();
    let mut scratch = BundleScratch::default();

    {
        let mut context = TemplateContext::new(&mut entity, &mut references);
        let mut writer = scratch.writer();

        TemplateEffect::apply(Marker(7), &mut context, &mut writer);

        writer
            .write(&mut *context.entity)
            .expect("the entity is live");
    }

    assert_eq!(entity.get::<Marker>().cloned(), Some(Marker(7)));
}

// -----------------------------------------------------------------------------
// Resolution

/// A scene that describes nothing leaves the resolved scene empty.
#[test]
fn an_empty_scene_resolves_to_nothing() {
    let mut scene = ResolvedScene::new();

    ().resolve(&mut ResolveContext::new(), &mut scene)
        .expect("the empty scene resolves");

    assert!(scene.component_templates().is_empty());
    assert!(scene.entity_references().is_empty());
    assert!(scene.children().is_empty());
    assert!(scene.parent().is_none());
}

/// A tuple is resolved in declaration order, and every part of it contributes: two templates of the
/// same component are kept side by side, and the later one wins when both are applied.
#[test]
fn scenes_compose_in_declaration_order() {
    let mut scene = ResolvedScene::new();

    (MarkerScene(1), MarkerScene(2))
        .resolve(&mut ResolveContext::new(), &mut scene)
        .expect("the scene resolves");

    assert_eq!(scene.component_templates().len(), 2);
    assert_eq!(apply_to_entity(&scene), Marker(2));
}

/// A boxed scene resolves like the scene it holds, which is what the boxed traits are for.
#[test]
fn a_boxed_scene_resolves() {
    let boxed: Box<dyn Scene> = Box::new(MarkerScene(6));
    let mut scene = ResolvedScene::new();

    boxed
        .resolve(&mut ResolveContext::new(), &mut scene)
        .expect("the boxed scene resolves");

    assert_eq!(apply_to_entity(&scene), Marker(6));
}

// -----------------------------------------------------------------------------
// Children

/// The children of a scene keep the order of the list they came from, one resolved scene each.
#[test]
fn children_are_kept_in_list_order() {
    let mut scene = ResolvedScene::new();

    SceneChildren((EntityScene(MarkerScene(1)), EntityScene(MarkerScene(2))))
        .resolve(&mut ResolveContext::new(), &mut scene)
        .expect("the children resolve");

    assert_eq!(scene.children().len(), 2);
    assert_eq!(apply_to_entity(&scene.children()[0]), Marker(1));
    assert_eq!(apply_to_entity(&scene.children()[1]), Marker(2));
}

// -----------------------------------------------------------------------------
// Parent

/// A parent template records the edge on the resolved scene, and the last one recorded is the one
/// that is kept.
#[test]
fn a_parent_template_records_an_edge() {
    let mut world = World::alloc();
    let parent = world.spawn((), None).id();
    let other = world.spawn((), None).id();

    let mut scene = ResolvedScene::new();

    SceneParent::from(parent)
        .resolve(&mut ResolveContext::new(), &mut scene)
        .expect("the parent resolves");
    assert!(matches!(scene.parent(), Some(EntityTemplate::Entity(id)) if id == parent));

    (SceneParent::from(other), SceneParent::from(parent))
        .resolve(&mut ResolveContext::new(), &mut scene)
        .expect("the parents resolve");
    assert!(matches!(scene.parent(), Some(EntityTemplate::Entity(id)) if id == parent));
}

// -----------------------------------------------------------------------------
// Lists

/// A scene list adds one resolved scene per entry: a tuple in order, a `Vec` in order, an `Option`
/// only when it is `Some`, and a wrapped scene as a single entry.
#[test]
fn a_scene_list_resolves_one_scene_per_entry() {
    let mut context = ResolveContext::new();
    let mut scenes = Vec::new();

    (EntityScene(MarkerScene(1)), EntityScene(MarkerScene(2)))
        .resolve_list(&mut context, &mut scenes)
        .expect("the list resolves");
    assert_eq!(scenes.len(), 2);
    assert_eq!(apply_to_entity(&scenes[1]), Marker(2));

    scenes.clear();
    vec![MarkerScene(3), MarkerScene(4)]
        .resolve_list(&mut context, &mut scenes)
        .expect("the list resolves");
    assert_eq!(scenes.len(), 2);
    assert_eq!(apply_to_entity(&scenes[0]), Marker(3));

    scenes.clear();
    Option::<EntityScene<MarkerScene>>::None
        .resolve_list(&mut context, &mut scenes)
        .expect("the absent list resolves");
    assert!(scenes.is_empty());

    scenes.clear();
    EntityScene(MarkerScene(5))
        .resolve_list(&mut context, &mut scenes)
        .expect("the single scene resolves");
    assert_eq!(scenes.len(), 1);
    assert_eq!(apply_to_entity(&scenes[0]), Marker(5));
}

// -----------------------------------------------------------------------------
// Templates

/// An inserted template takes the canonical slot of its type, so a second insert replaces it,
/// while a pushed template is applied in addition to it.
///
/// The slot is keyed by the template's type, so the two inserts have to be the same type — which is
/// what a derived template or a named template is, and what a closure literal is not.
#[test]
fn an_inserted_template_takes_the_canonical_slot() {
    let mut scene = ResolvedScene::new();

    scene.insert_template(Count(1));
    scene.insert_template(Count(2));

    assert_eq!(scene.component_templates().len(), 1);
    assert_eq!(apply_to_entity(&scene), Marker(2));

    scene.push_template(template(|_| Ok(Marker(3))));

    assert_eq!(scene.component_templates().len(), 2);
    assert_eq!(apply_to_entity(&scene), Marker(3));
}

/// The canonical slot of a type is handed out as a mutable template, which is how a later part of a
/// composition — or a patch — edits a template instead of adding another one.
#[test]
fn the_canonical_slot_can_be_edited_in_place() {
    let mut scene = ResolvedScene::new();

    scene.get_or_insert_template::<Count>().0 = 7;

    assert_eq!(scene.component_templates().len(), 1);
    assert_eq!(apply_to_entity(&scene), Marker(7));

    // Asking again hands out the same slot rather than adding a second template.
    scene.get_or_insert_template::<Count>();
    assert_eq!(scene.component_templates().len(), 1);
}

/// A name is declared once, however many times the same reference is declared.
#[test]
fn a_name_is_declared_once() {
    let reference = EntityReference::new(file!(), line!(), column!(), 0, 0);
    let mut scene = ResolvedScene::new();

    scene.add_entity_reference(reference);
    scene.add_entity_reference(reference);

    assert_eq!(scene.entity_references(), &[reference]);
}

// -----------------------------------------------------------------------------
// Applying

/// Spawning a scene creates the entity, writes its components, and spawns its children under it, in
/// list order.
#[test]
fn a_scene_spawns_its_entity_and_children() {
    let mut world = World::alloc();

    let scene = (
        MarkerScene(1),
        SceneChildren((EntityScene(MarkerScene(2)), EntityScene(MarkerScene(3)))),
    );

    let root = world.spawn_scene(scene, None).expect("the scene spawns");

    assert_eq!(
        world.entity_owned(root).get::<Marker>().cloned(),
        Some(Marker(1))
    );

    let children = world
        .entity_owned(root)
        .children()
        .expect("the root is live")
        .to_vec();

    assert_eq!(children.len(), 2);
    assert_eq!(
        world.entity_owned(children[0]).get::<Marker>().cloned(),
        Some(Marker(2))
    );
    assert_eq!(
        world.entity_owned(children[1]).get::<Marker>().cloned(),
        Some(Marker(3))
    );
}

/// Applying a resolved scene to an entity that already exists writes into it and spawns its
/// children under it, leaving the components it already had alone.
#[test]
fn a_scene_applies_to_an_existing_entity() {
    let mut world = World::alloc();
    let mut target = world.spawn(Scale(1.0), None);

    let mut resolved = ResolvedScene::new();
    (MarkerScene(1), SceneChildren(EntityScene(MarkerScene(2))))
        .resolve(&mut ResolveContext::new(), &mut resolved)
        .expect("the scene resolves");

    resolved.apply(&mut target).expect("the scene applies");

    assert_eq!(target.get::<Marker>().cloned(), Some(Marker(1)));
    assert_eq!(target.get::<Scale>().cloned(), Some(Scale(1.0)));

    let children = target.children().expect("the root is live").to_vec();
    assert_eq!(children.len(), 1);
    assert_eq!(
        world.entity_owned(children[0]).get::<Marker>().cloned(),
        Some(Marker(2))
    );
}

/// A parent edge is resolved once every entity of the scene exists, so it may name an entity that is
/// declared later in the scene than the one that points at it.
#[test]
fn a_parent_edge_can_name_an_entity_declared_later() {
    let mut world = World::alloc();

    let later = EntityReference::new(file!(), line!(), column!(), 0, 0);

    // The first child points at `later`, which its *sibling* declares.
    let scene = SceneChildren((
        EntityScene((MarkerScene(1), SceneParent::from(later))),
        EntityScene((MarkerScene(2), NameScene(later))),
    ));

    let root = world.spawn_scene(scene, None).expect("the scene spawns");

    // The child that declared the name is the only one left under the root: the other one was moved
    // under it, which is what the edge said.
    let children = world
        .entity_owned(root)
        .children()
        .expect("the root is live")
        .to_vec();
    assert_eq!(children.len(), 1);
    let second = children[0];

    let moved = world
        .entity_owned(second)
        .children()
        .expect("the sibling is live")
        .to_vec();
    assert_eq!(moved.len(), 1);
    let first = moved[0];

    assert_eq!(
        world.entity_owned(first).parent().expect("live"),
        Some(second)
    );
    assert_eq!(
        world.entity_owned(first).get::<Marker>().cloned(),
        Some(Marker(1))
    );
    assert_eq!(
        world.entity_owned(second).get::<Marker>().cloned(),
        Some(Marker(2))
    );
}

/// A scene list spawns one root per scene, and the roots share one name scope, so a name declared by
/// one of them resolves for the others.
#[test]
fn a_scene_list_spawns_its_roots_in_one_scope() {
    let mut world = World::alloc();

    let first = EntityReference::new(file!(), line!(), column!(), 0, 0);

    let list = (
        EntityScene(NameScene(first)),
        EntityScene(SceneParent::from(first)),
    );

    let roots = world.spawn_scene_list(list, None).expect("the list spawns");

    assert_eq!(roots.len(), 2);
    assert_eq!(
        world.entity_owned(roots[1]).parent().expect("live"),
        Some(roots[0])
    );
}

/// An entity that was created for the scene has no place in the tree to announce, so its parent edge
/// is applied without a [`ReparentSignal`].
#[test]
fn a_created_entity_is_re_parented_without_a_signal() {
    let mut world = World::alloc();
    let parent = world.spawn((), None).id();

    let before = reparent_signals(&world);

    let scene = (MarkerScene(1), SceneParent::from(parent));
    let root = world.spawn_scene(scene, None).expect("the scene spawns");

    assert_eq!(
        world.entity_owned(root).parent().expect("live"),
        Some(parent)
    );
    assert_eq!(reparent_signals(&world), before);
}

/// An entity that already existed is already part of the tree, so moving it is announced with a
/// [`ReparentSignal`].
#[test]
fn an_existing_entity_is_re_parented_with_a_signal() {
    let mut world = World::alloc();
    let parent = world.spawn((), None).id();
    let target = world.spawn((), None).id();

    let before = reparent_signals(&world);

    world
        .apply_scene((MarkerScene(1), SceneParent::from(parent)), target)
        .expect("the scene applies");

    assert_eq!(
        world.entity_owned(target).parent().expect("live"),
        Some(parent)
    );
    assert_eq!(reparent_signals(&world), before + 1);
}

// -----------------------------------------------------------------------------
// Composition

/// A scope resolves like the scene it wraps.
#[test]
fn a_scope_resolves_its_scene() {
    let mut scene = ResolvedScene::new();

    SceneScope(MarkerScene(4))
        .resolve(&mut ResolveContext::new(), &mut scene)
        .expect("the scope resolves");

    assert_eq!(apply_to_entity(&scene), Marker(4));
}

/// A scene can be a function, which is how a description is composed at runtime.
#[test]
fn a_scene_can_be_a_function() {
    let mut scene = ResolvedScene::new();

    SceneFunction(|_context: &mut ResolveContext, scene: &mut ResolvedScene| {
        scene.push_template(template(|_| Ok(Marker(5))));
    })
    .resolve(&mut ResolveContext::new(), &mut scene)
    .expect("the function scene resolves");

    assert_eq!(apply_to_entity(&scene), Marker(5));
}

/// `InitTemplate` creates the canonical template from its `Default`, a patch edits that same slot,
/// and `InsertTemplate` replaces it — in every case without adding a second template.
#[test]
fn a_template_can_be_initialised_patched_and_replaced() {
    let mut scene = ResolvedScene::new();
    let mut context = ResolveContext::new();

    InitTemplate::<Count>::new()
        .resolve(&mut context, &mut scene)
        .expect("the template is initialised");

    assert_eq!(scene.component_templates().len(), 1);
    assert_eq!(apply_to_entity(&scene), Marker(0));

    Count::patch_template(|template, _context| template.0 = 9)
        .resolve(&mut context, &mut scene)
        .expect("the template is patched");

    assert_eq!(scene.component_templates().len(), 1);
    assert_eq!(apply_to_entity(&scene), Marker(9));

    InsertTemplate::new(Count(3))
        .resolve(&mut context, &mut scene)
        .expect("the template is replaced");

    assert_eq!(scene.component_templates().len(), 1);
    assert_eq!(apply_to_entity(&scene), Marker(3));
}

/// A type that has a template of its own can patch it through `PatchIntoTemplate`, without naming the
/// template type.
#[test]
fn a_type_can_patch_the_template_it_is_built_from() {
    let mut scene = ResolvedScene::new();

    // `Scale` is `Clone + Default`, so its template is itself, and the patch edits a default value.
    Scale::patch(|template, _context| template.0 = 2.5)
        .resolve(&mut ResolveContext::new(), &mut scene)
        .expect("the patch resolves");

    assert_eq!(scene.component_templates().len(), 1);
    assert_eq!(apply_to::<Scale>(&scene), Scale(2.5));
}

// -----------------------------------------------------------------------------
// Cached patches

/// A scene that builds on the patch at a handle.
///
/// This is the asset-less form of [`CachedSceneAsset`], which looks a patch up by path.
struct Cached(Handle<ScenePatch>);

impl Scene for Cached {
    fn resolve(self, context: &mut ResolveContext, scene: &mut ResolvedScene) -> ZlimResult<()> {
        scene.include_cached(context.patches(), self.0)
    }
}

/// A scene that describes a [`Scale`] through its canonical template.
struct ScaleScene(f32);

impl Scene for ScaleScene {
    fn resolve(self, _context: &mut ResolveContext, scene: &mut ResolvedScene) -> ZlimResult<()> {
        scene.insert_template(Scale(self.0));
        Ok(())
    }
}

/// A scene that edits the canonical [`Scale`] template — which, on a scene that builds on a cached
/// one, is the cached template's clone.
struct PatchScale(f32);

impl Scene for PatchScale {
    fn resolve(self, _context: &mut ResolveContext, scene: &mut ResolvedScene) -> ZlimResult<()> {
        scene.get_or_insert_template::<Scale>().0 = self.0;
        Ok(())
    }
}

/// A scene that registers one dependency.
struct DepScene;

impl Scene for DepScene {
    fn resolve(self, _context: &mut ResolveContext, _scene: &mut ResolvedScene) -> ZlimResult<()> {
        Ok(())
    }

    fn register_dependencies(&self, dependencies: &mut SceneDependencies) {
        dependencies.register_erased(TypeId::of::<Marker>(), AssetPath::from("dep.cnt"));
    }
}

/// Resolves `patch` and adds the result to `patches`, returning its handle.
fn add_patch(patch: ScenePatch, patches: &mut Assets<ScenePatch>) -> Handle<ScenePatch> {
    let mut patch = patch;
    patch.resolve(None, patches).expect("the patch resolves");
    patches.add(patch)
}

/// A scene that builds on a cached patch applies the cached one first, and a template it takes over
/// becomes its own: the cached asset keeps what it was resolved with.
#[test]
fn a_scene_builds_on_a_cached_patch() {
    let mut patches = Assets::<ScenePatch>::default();

    let base = add_patch(
        ScenePatch::new((MarkerScene(1), ScaleScene(1.5))),
        &mut patches,
    );
    let patch = add_patch(
        ScenePatch::new((Cached(base.clone()), MarkerScene(2), PatchScale(9.0))),
        &mut patches,
    );

    let mut world = World::alloc();

    // The cached scene, applied on its own, is what it was resolved to.
    let base_root = patches
        .get(&base)
        .expect("the base patch is there")
        .spawn(&mut world, None)
        .expect("the base scene spawns")
        .id();

    assert_eq!(
        world.entity_owned(base_root).get::<Marker>().cloned(),
        Some(Marker(1))
    );
    assert_eq!(
        world.entity_owned(base_root).get::<Scale>().cloned(),
        Some(Scale(1.5))
    );

    // The patch applies the cached scene, with its own templates on top: the marker it pushes wins
    // over the cached one, and the scale it edited is its own copy.
    let patch_root = patches
        .get(&patch)
        .expect("the patch is there")
        .spawn(&mut world, None)
        .expect("the patch spawns")
        .id();

    assert_eq!(
        world.entity_owned(patch_root).get::<Marker>().cloned(),
        Some(Marker(2))
    );
    assert_eq!(
        world.entity_owned(patch_root).get::<Scale>().cloned(),
        Some(Scale(9.0))
    );
}

/// The children of a cached scene are spawned before the children of the scene that builds on it.
#[test]
fn a_cached_scene_spawns_its_children_first() {
    let mut patches = Assets::<ScenePatch>::default();

    let base = add_patch(
        ScenePatch::new(SceneChildren(EntityScene(MarkerScene(1)))),
        &mut patches,
    );
    let patch = add_patch(
        ScenePatch::new((Cached(base), SceneChildren(EntityScene(MarkerScene(2))))),
        &mut patches,
    );

    let mut world = World::alloc();
    let root = patches
        .get(&patch)
        .expect("the patch is there")
        .spawn(&mut world, None)
        .expect("the patch spawns")
        .id();

    let children = world
        .entity_owned(root)
        .children()
        .expect("the root is live")
        .to_vec();

    assert_eq!(children.len(), 2);
    assert_eq!(
        world.entity_owned(children[0]).get::<Marker>().cloned(),
        Some(Marker(1))
    );
    assert_eq!(
        world.entity_owned(children[1]).get::<Marker>().cloned(),
        Some(Marker(2))
    );
}

/// A cached scene has to be included before anything else is described, and only once.
#[test]
fn a_cached_scene_is_included_first_and_once() {
    let mut patches = Assets::<ScenePatch>::default();
    let base = add_patch(ScenePatch::new(()), &mut patches);

    let mut scene = ResolvedScene::new();

    MarkerScene(1)
        .resolve(&mut ResolveContext::with_patches(&patches), &mut scene)
        .expect("the marker resolves");

    assert!(
        scene.include_cached(Some(&patches), base.clone()).is_err(),
        "a cached scene cannot be included after the scene describes something"
    );

    let mut scene = ResolvedScene::new();
    scene
        .include_cached(Some(&patches), base.clone())
        .expect("the cached scene is included first");
    assert!(scene.is_cached());
    assert_eq!(scene.cached_patch(), Some(&base));

    assert!(
        scene.include_cached(Some(&patches), base).is_err(),
        "a scene can only build on one cached scene"
    );
}

/// A patch that has not been resolved has nothing to spawn, and says so rather than spawning an
/// empty scene.
#[test]
fn an_unresolved_patch_cannot_spawn() {
    let mut patches = Assets::<ScenePatch>::default();
    let unresolved = patches.add(ScenePatch::new(MarkerScene(1)));
    let patch = patches.get(&unresolved).expect("the patch is there");

    let mut world = World::alloc();

    assert!(patch.spawn(&mut world, None).is_err());

    let mut entity = world.spawn_empty(None);
    assert!(patch.apply(&mut entity).is_err());
}

/// A patch that has not been resolved cannot be built on, and a patch resolves once.
#[test]
fn a_patch_resolves_once() {
    let mut patches = Assets::<ScenePatch>::default();
    let unresolved = patches.add(ScenePatch::new(MarkerScene(1)));

    let mut scene = ResolvedScene::new();
    assert!(
        scene
            .include_cached(Some(&patches), unresolved.clone())
            .is_err(),
        "an unresolved patch has no resolved scene to build on"
    );

    let mut patch = patches.remove(unresolved.id()).expect("the patch is there");
    patch
        .resolve(None, &mut patches)
        .expect("the patch resolves the first time");
    assert!(patch.resolved().is_some());
    assert!(
        patch.resolve(None, &mut patches).is_err(),
        "the description is gone after it was resolved"
    );
    patches
        .insert(unresolved.id(), patch)
        .expect("the patch is inserted back");
}

/// A patch resolves the patches it builds on before its own description, which is what lets a scene
/// include a cached one when the asset side has no loader for a patch.
#[test]
fn a_patch_resolves_what_it_builds_on() {
    let mut patches = Assets::<ScenePatch>::default();

    // The base is published *unresolved*: only its handle is known.
    let base = patches.add(ScenePatch::new(MarkerScene(1)));

    let mut outer = ScenePatch::new((Cached(base.clone()), MarkerScene(2)));
    outer.dependencies.push(ErasedHandle::from(base.clone()));

    outer
        .resolve(None, &mut patches)
        .expect("the patch it builds on is resolved first");

    assert!(
        patches
            .get(&base)
            .expect("the base is there")
            .resolved()
            .is_some(),
        "the base was resolved by the patch that builds on it"
    );
}

/// Dependencies registered by the parts of a composition are forwarded to the whole.
#[test]
fn dependencies_of_a_composition_are_forwarded() {
    let mut dependencies = SceneDependencies::new();

    (
        DepScene,
        SceneChildren(EntityScene(DepScene)),
        InitTemplate::<Count>::new(),
    )
        .register_dependencies(&mut dependencies);

    assert_eq!(dependencies.len(), 2);
    assert_eq!(
        dependencies.iter().next().unwrap().path,
        AssetPath::from("dep.cnt")
    );
}

// -----------------------------------------------------------------------------
// Failure

/// A component that reports whether it was dropped.
#[derive(Component, Clone, Debug)]
struct Tracked(Arc<AtomicBool>);

impl Drop for Tracked {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// A scene whose second template fails after the first one has been pushed: what the first one had
/// put in the scratch space is dropped, and the scratch space is empty again.
#[test]
fn a_failed_application_drops_what_it_pushed() {
    let dropped = Arc::new(AtomicBool::new(false));
    let flag = dropped.clone();

    let mut world = World::alloc();
    let mut entity = world.spawn_empty(None);
    let mut references = EntityReferences::new();
    let mut scratch = BundleScratch::new();

    let mut scene = ResolvedScene::new();
    scene.push_template(template(move |_| Ok(Tracked(flag.clone()))));
    scene.push_template(template(|_| {
        Err::<Scale, _>(zlim_core::error::ZlimError::error("boom"))
    }));

    let result = scene.apply_with(&mut entity, &mut references, &mut scratch);

    assert!(result.is_err(), "the second template fails");
    assert!(
        scratch.is_empty(),
        "nothing is left waiting in the scratch space"
    );
    assert!(
        dropped.load(Ordering::SeqCst),
        "the pushed component was dropped rather than leaked"
    );
}
