# zlim-scene

Scenes for zlim: describing entities — their components, their children, and the other entities they
point at — resolving those descriptions, and applying them to a world.

Two macros write a description: `scn!` writes one entity, and `scn_list!` writes several. The rest of
the crate is what turns a description into entities: resolving it into a [`ResolvedScene`], applying
that to a world, and keeping a description in the asset system so that another one can build on it.

## Writing a scene

`scn!` is a sequence of *entries*, and it describes **one** entity: which components it is made of,
what names it can be pointed at by, and which entities belong under it. A scene of any size is
written this way — children nest, and a name reaches across the whole description:

```rust
use zlim_core::derive::{Component, FromTemplate};
use zlim_core::entity::EntityId;
use zlim_core::world::World;
use zlim_reflect::TypePath;
use zlim_scene::{WorldSceneExt, scn};

#[derive(TypePath, Component, Clone, Default, Debug, PartialEq)]
struct Health {
    current: u32,
    max: u32,
}

/// A component that points at another entity. `#[derive(FromTemplate)]` gives it a template whose
/// field is an `EntityTemplate`, which is what a `#Name` resolves to.
#[derive(TypePath, Component, Clone, FromTemplate)]
struct Target {
    to: EntityId,
}

let mut world = World::alloc();

let root = world
    .spawn_scene(
        scn! {
            #Root
            Health { current: 10, max: 20 }
            Children [
                #Child
                Health { current: 3, max: 3 }
                --
                Target { to: #Root }
            ]
        },
        None,
    )
    .unwrap();

let children = world.entity_owned(root).children().unwrap().to_vec();
assert_eq!(children.len(), 2);
assert_eq!(
    world.entity_owned(children[0]).get::<Health>().cloned(),
    Some(Health { current: 3, max: 3 })
);

// A name is bound by the entity that declares it, and only once every entity of the scene exists —
// so a child can point back at its parent.
assert_eq!(
    world
        .entity_owned(children[1])
        .get::<Target>()
        .map(|target| target.to),
    Some(root)
);
```

`scn_list!` writes several entities, separated by `--`. They share one name scope, so a `#Name`
declared by one of them resolves for the others — which is how sibling entities point at each other,
in either order:

```rust
use zlim_core::derive::Component;
use zlim_core::world::World;
use zlim_reflect::TypePath;
use zlim_scene::{WorldSceneExt, scn_list};

#[derive(TypePath, Component, Clone, Default, Debug, PartialEq)]
struct Scale(f32);

let mut world = World::alloc();

let roots = world
    .spawn_scene_list(
        scn_list! {
            #Left
            Scale(1.0)
            --
            Scale(2.0)
            Parent(#Left)
        },
        None,
    )
    .unwrap();

assert_eq!(roots.len(), 2);
assert_eq!(
    world.entity_owned(roots[1]).parent().expect("live"),
    Some(roots[0])
);
```

### Entries

An entry is one of:

| Written | Means |
|---|---|
| `#Name` | name this entity, so the scene can point at it |
| `Type { field: value, ... }` | edit the canonical template of `Type` |
| `Type(value, ...)` | the same, for a tuple struct (fields `0`, `1`, …) |
| `Type::function(args)` | replace the canonical template of `Type` with what the call produces |
| `~template { field: value, ... }` | the same two, for a template written by hand |
| `~template(args)` / `~expression` | store what is written as the template itself |
| `Type` | make sure the canonical template of `Type` exists (from its `Default`) |
| `Children [ entry* (-- entry*)* ]` | the entities under this one, in list order |
| `Parent(value)` | an explicit parent edge for this entity |
| `: expression` | build on the cached scene asset the expression names |
| `@ expression` | include a scene where it is written, without caching |

The difference between the two ways a template is written is what a later part of a composition, or a
patch, starts from:

- `Type { … }`, `Type(…)` and `Type` **edit** the canonical template of the type (and create it from
  its `Default` if the composition has not described it yet), so what they leave out is kept.
- `Type::function(…)` and `~…` **replace** it outright.

`~` means "this path *is* the template, not the type it is built from": without it, the path goes
through [`FromTemplate`] first. That is why a hand-written template is written with `~`, and why
`Health::full(10)` and `~Health::full(10)` differ for a type whose template is not the type itself.

A *value* — the right-hand side of a field, and the argument of `Parent` — is passed through as it
was written, except for a lone `#Name`, which becomes the entity that name stands for:

```rust,ignore
#Root
Sprite { color: RED }
Target { to: #Root }                       // the entity, not the name
Transform { translation: some_fn(1, 2) }   // any expression; a comma inside a group is fine
```

Names are identified by where they were written and by the macro invocation that produced them, so
two `scn!` invocations never collide even if they use the same `#Root`, and one invocation used
twice (a function that builds a scene) produces distinct entities each run. A name that nobody
declares is not a compile error: resolving the scene reports it.

### Children and parents

`Children [ … ]` is a scene list, so its entries are entities in their own right — they nest to any
depth, and they are spawned with their parent already in place. `Parent(value)` describes an edge for
the entity that carries the entry; it is applied once every entity of the scene exists, so it may
name an entity that is declared later. Both may be written more than once: several `Children [ … ]`
are appended in the order they are written, and the last `Parent(value)` is the one that is applied.
See [hierarchy](#hierarchy) for what that means for the world.

### Cached scenes

`: "scenes/player.scene"` makes the scene build on the scene asset at that path, and `@ expression`
includes a scene where it is written. A `:` entry has to come before every entry that describes a
template or a child — names may precede it, since they describe neither. Both are described in
[caching a scene](#caching-a-scene).

### The twelve-part limit

A scene, and a scene list, are each a tuple of their entries, so a description is limited to **twelve
parts**. A *part* is a run of entries of one kind: statements written together are one part, and
scenes written together are another, so writing entries of the same kind together costs nothing and
alternating between them costs a part each time. A scene that would need more than twelve parts is a
compile error, and wants part of its description moved into a nested entity; a list of more than
twelve entities wants the same.

## How a scene works

### A description is composed of parts

[`Scene`] is the trait of a description of one entity, and [`SceneList`] the trait of a description
of a list of entities. Both are *descriptions*: they are resolved before anything exists in a world.

The two traits are implemented by the pieces a description is built from, and **a tuple of scenes is
itself a scene**, so a composition is written as a tuple of as many parts as it has:

| Piece | What it contributes |
|---|---|
| [`SceneFunction`] | whatever a closure does to the resolved scene (`scn!` uses this for its statements) |
| [`InsertTemplate`] | a canonical template slot, replaced |
| [`InitTemplate`] | makes sure a canonical slot exists, from its `Default` |
| [`TemplatePatch`] | edits a canonical slot in place (what [`PatchFromTemplate`] and [`PatchTemplate`] build) |
| `FnTemplate` | a template that is only ever applied, never edited |
| [`SceneChildren`] | the entities under the entity |
| [`SceneParent`] | an explicit parent edge for the entity |
| [`SceneScope`] / [`SceneListScope`] | a name scope of its own (what the two macros return) |
| [`EntityScene`] | one scene used where a scene list is expected |

```rust
use zlim_core::entity::EntityId;
use zlim_core::template::EntityTemplate;
use zlim_scene::{ResolveContext, ResolvedScene, Scene, SceneChildren, SceneParent};

let parent = EntityId::from_bits(0x0000_0001_0000_0001).unwrap();

let mut context = ResolveContext::new();
let mut scene = ResolvedScene::new();

// A scene is composed like a bundle: `()` describes nothing, and the parts are added to it.
// `vec![(), ()]` is a list of two scenes that describe nothing, so this adds two children.
(SceneParent::from(parent), SceneChildren(vec![(), ()]))
    .resolve(&mut context, &mut scene)
    .unwrap();

assert!(matches!(scene.parent(), Some(EntityTemplate::Entity(id)) if id == parent));
assert_eq!(scene.children().len(), 2);
```

### Templates

A [`Template`] is *how a value is built*, and it is what a scene stores: a scene does not hold
components, it holds templates that build them when the scene is applied.

- A `Clone` type is its own template, and building it clones it — which is what gives most types a
  template. A `Clone + Default` type is described by itself.
- [`FromTemplate`] names the template of a type, which is how a type is described through the
  templates of its fields. `#[derive(FromTemplate)]` writes that template for a type whose fields
  need more than a clone — a field holding a handle, say.
- A type that needs neither is a template written by hand, which is what `~` addresses.

Templates are stored under a **canonical slot** keyed by their type, which is the slot that edits
land in. That is what makes the two forms of an entry different:

```rust
use zlim_core::derive::Component;
use zlim_reflect::TypePath;
use zlim_scene::{ResolveContext, ResolvedScene, Scene, scn};

#[derive(TypePath, Component, Clone, Default, Debug, PartialEq)]
struct Health {
    current: u32,
    max: u32,
}

impl Health {
    /// A `Health` with both values set, as a template of its own could produce.
    fn full(value: u32) -> Self {
        Self {
            current: value,
            max: value,
        }
    }
}

let mut context = ResolveContext::new();
let mut scene = ResolvedScene::new();

// The first form edits the canonical template — here the `Health` a `Default` produced — so `max`
// keeps what the template had.
scn! { Health { current: 3 } }
    .resolve(&mut context, &mut scene)
    .unwrap();

// The second replaces that template outright.
scn! { Health::full(5) }
    .resolve(&mut context, &mut scene)
    .unwrap();

// Either way the scene holds one template: the canonical slot of `Health`.
assert_eq!(scene.component_templates().len(), 1);
```

### Resolving

[`Scene::resolve`] walks a description and fills in a [`ResolvedScene`], which is the resolved form
of one entity: the templates to write, the names it declares, the scenes that belong under it, and
its explicit parent edge. Resolution is given a [`ResolveContext`], which carries what a description
may need from the outside — the patches it can build on, and the asset server (see
[caching](#caching-a-scene)).

Nothing in the world is touched: resolving a scene is what lets it be resolved once and applied many
times, and what lets a whole scene be checked before any entity is created.

### Applying

Applying a resolved scene to an entity happens in four steps, in this order:

1. Every entity of the tree is spawned — the root is the entity the scene is applied to, and every
   child scene is spawned under the entity that describes it.
2. Every `#Name` the tree declares is bound to the entity that declares it.
3. Every entity gets its templates, written in one go through a `BundleWriter`: a component template
   pushes one column of the row, a template whose output is a whole bundle hands over every component
   it carries — required components included, which the writer initialises.
4. Every explicit parent edge is applied, after which the hierarchy is final.

The order of 1 and 3 is what makes forward references work: the entities exist, and their names are
bound, before any template asks for them.

```rust
use zlim_core::world::World;
use zlim_scene::{ResolveContext, ResolvedScene, Scene};

let mut world = World::alloc();

let mut resolved = ResolvedScene::new();
().resolve(&mut ResolveContext::new(), &mut resolved).unwrap();

let entity = resolved.spawn(&mut world, None).unwrap();
assert!(entity.is_spawned());
```

A scene carries data and structure, so nothing of the scene's own runs after its components are
written — exactly as with `EntityOwned::insert`, which is the same path: the components' own hooks
run, no effect of the scene does.

### Hierarchy

zlim keeps the hierarchy in the world rather than in a component: `Parent` and `Children` are query
data over the entity tree, and the tree is maintained by spawning with a parent or by re-parenting an
entity. There is consequently no relationship *component* to insert, nothing to keep in sync through
hooks, and no relationship-hook mode to choose, so this crate has no equivalent of Bevy's
`Relationship` / `RelationshipTarget` / `RelationshipHookMode`:

- [`SceneChildren`] describes the entities a scene is the parent of. They are spawned with their
  parent already in place, so no edge has to be added afterwards — and nothing has to be announced,
  because an entity that is born in place was never anywhere else. The order of the list is the order
  of `Children`.
- [`SceneParent`] describes an explicit parent edge for the entity itself. This is what a scene
  applied to an existing entity uses; it is applied once every entity of the scene exists, which is
  what lets it point at a name that is declared later in the same scene.

An entity a scene creates is never *re-parented* in the sense the transform propagation cares about:
the edge is recorded while the entity is being built, so it is applied with
`EntityOwned::reparent_without_signal` — the entity is new, and its own ticks already say so. Only a
scene applied to an entity that already has a place in the tree moves something the rest of the world
has to hear about, through `EntityOwned::reparent`.

### From a world

Spawning a scene is a two-step affair — resolve the description, then apply the resolved form — and
[`WorldSceneExt`] is the shorthand for doing both at once:

| Call | What it does |
|---|---|
| `world.resolve_scene(scene)` | resolves one description against the patches of the world |
| `world.resolve_scene_list(list)` | the same, for a list |
| `world.spawn_scene(scene, parent)` | resolves it and spawns a new root for it |
| `world.spawn_scene_list(list, parent)` | resolves a list and spawns one root per entity |
| `world.apply_scene(scene, target)` | resolves it and applies it to an entity that already exists |

The resolved form can be kept instead, and applied again and again with [`ResolvedScene::spawn`] and
[`ResolvedScene::apply`] — which is what a [`ScenePatch`] does, and what makes a scene worth keeping
around.

## Caching a scene

A scene can also live in the asset system. [`ScenePatch`] is that asset: it holds the description,
the handles of the assets the description depends on, and — once resolved — the resolved form.
Resolving a patch walks the description once; every application after it reuses the result.

```rust
use zlim_asset::assets::Assets;
use zlim_core::world::World;
use zlim_scene::ScenePatch;

let mut patches = Assets::<ScenePatch>::default();

// A patch resolves into its own collection...
let mut patch = ScenePatch::new(());
patch.resolve(None, &mut patches).unwrap();
let handle = patches.add(patch);

// ... and the resolved form is then applied as often as needed.
let mut world = World::alloc();
let entity = patches
    .get(&handle)
    .unwrap()
    .spawn(&mut world, None)
    .unwrap();
assert!(entity.is_spawned());
```

`CachedSceneAsset` is how one scene builds on another: it names a patch by path, and the scene it
appears in applies the cached scene first. A template the cached scene describes can then be taken
over with [`ResolvedScene::get_or_insert_template`], which clones the cached template the first time
it is asked for — the cached copy is then skipped when the scene is applied. That copy-on-write is
what lets a patch edit a field of a scene it does not own, and it leaves the patch asset itself
untouched:

```rust,ignore
scn! {
    : "scenes/player.scene"     // applied first
    Health { current: 5 }       // `Health` is cloned out of it, so `max` survives
}
```

`@ expression` is the uncached counterpart: the expression is a scene of its own, and it is resolved
exactly where it is written, without a patch and without copy-on-write.

The resolved form is shared through an `Arc`, and a scene that includes a cached one holds that
`Arc` — so applying needs no asset lookups, and a patch that has not been resolved yet is refused
while resolving rather than while applying.

### Dependencies

`ScenePatch::load` starts loading what a description depends on, and
[`Scene::register_dependencies`] is where a description says what that is (`: "path"` registers the
patch it names). Resolving a patch resolves the patches it depends on first — depth-first, because
"loaded" does not mean "resolved" — and so does the schedule that spawns queued scenes.

### Scene lists

[`SceneListPatch`] is the list-shaped counterpart: it holds a `SceneList` and resolves it into one
`ResolvedScene` per entity, which are spawned as a group under one parent.

## Plugins

Scene assets live in the asset system, so the crate needs a few things registered:

- [`AssetPlugin`] — from `zlim-asset` — builds the asset sources, inserts the `AssetServer`, and runs
  the load pipeline. It has to be added first: registering an asset type needs the server.
- [`ScenePlugin`] — from this crate — registers `Assets<ScenePatch>` and `Assets<SceneListPatch>`
  through `init_asset`, and inserts the job that builds queued scenes into the `SpawnScene` schedule.
  It declares itself after `AssetPlugin` and after `MainSchedulePlugin`.

```rust
use zlim_app::App;
use zlim_asset::plugin::AssetPlugin;
use zlim_scene::ScenePlugin;

let mut app = App::new();
app.add_plugins(AssetPlugin {
    // The examples never read from the file system, and watching would keep a thread alive.
    watch_for_changes_override: Some(false),
    ..AssetPlugin::default()
});
app.add_plugins(ScenePlugin);
app.build();
```

### Queueing a scene

Once the plugins are in, a scene can also be built later: [`WorldSceneQueueExt`] adds a patch to the
asset system and puts a [`ScenePatchInstance`] on the entity the scene belongs to. The `SpawnScene`
schedule — between `Update` and `PostUpdate` — then resolves the patch once it and its dependencies
are loaded, applies it, and takes the request out. That is what a level that streams in needs: the
entity exists as soon as it is queued, so it can be pointed at, and its components arrive with the
asset.

```rust
# use zlim_app::App;
# use zlim_asset::plugin::AssetPlugin;
# use zlim_core::derive::Component;
# use zlim_reflect::TypePath;
# use zlim_scene::{ScenePlugin, WorldSceneQueueExt, scn};
# #[derive(TypePath, Component, Clone, Default)]
# struct Scale(f32);
let mut app = App::new();
app.add_plugins(AssetPlugin {
    watch_for_changes_override: Some(false),
    ..AssetPlugin::default()
});
app.add_plugins(ScenePlugin);
app.build();

let entity = app
    .main_world_mut()
    .queue_spawn_scene(scn! { Scale(1.5) }, None)
    .unwrap();

// The scene is built by the `SpawnScene` schedule, later in the frame.
app.update();
```

[`SceneListPatchInstance`] is the list-shaped counterpart, and a [`ScenePatchInstance`] on an
existing entity applies the scene *to* that entity instead of spawning a new one. A queued scene
whose entity is gone is dropped, and a patch that cannot be resolved keeps its request and reports
the failure through `zlim_log`.

## Ported from Bevy

The design follows `bevy_scene`, adapted to zlim's built-in hierarchy and to its `Template`,
`BundleWriter` and task APIs. Thanks to the Bevy authors.

[`AssetPlugin`]: zlim_asset::plugin::AssetPlugin
[`EntityScene`]: crate::EntityScene
[`FromTemplate`]: zlim_core::template::FromTemplate
[`InitTemplate`]: crate::InitTemplate
[`InsertTemplate`]: crate::InsertTemplate
[`PatchFromTemplate`]: crate::PatchFromTemplate
[`PatchTemplate`]: crate::PatchTemplate
[`ResolveContext`]: crate::ResolveContext
[`ResolvedScene::apply`]: crate::ResolvedScene::apply
[`ResolvedScene::get_or_insert_template`]: crate::ResolvedScene::get_or_insert_template
[`ResolvedScene::spawn`]: crate::ResolvedScene::spawn
[`ResolvedScene`]: crate::ResolvedScene
[`Scene::register_dependencies`]: crate::Scene::register_dependencies
[`Scene::resolve`]: crate::Scene::resolve
[`SceneChildren`]: crate::SceneChildren
[`SceneFunction`]: crate::SceneFunction
[`SceneListPatchInstance`]: crate::SceneListPatchInstance
[`SceneListPatch`]: crate::SceneListPatch
[`SceneListScope`]: crate::SceneListScope
[`SceneList`]: crate::SceneList
[`SceneParent`]: crate::SceneParent
[`ScenePatchInstance`]: crate::ScenePatchInstance
[`ScenePatch`]: crate::ScenePatch
[`ScenePlugin`]: crate::ScenePlugin
[`SceneScope`]: crate::SceneScope
[`Scene`]: crate::Scene
[`TemplatePatch`]: crate::TemplatePatch
[`Template`]: zlim_core::template::Template
[`WorldSceneExt`]: crate::WorldSceneExt
[`WorldSceneQueueExt`]: crate::WorldSceneQueueExt
