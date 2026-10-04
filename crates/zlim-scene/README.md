# zlim-scene

The scene system: compile-time scene macros, and a dynamic scene asset format.

Two macros write a description: `scn!` writes one entity, `scn_list!` writes several. The rest of the
crate turns a description into entities — resolving it into a `ResolvedScene`, applying that to a
world, and keeping a description in the asset system so another one can build on it.

## Writing a scene

`scn!` is a sequence of *entries*, and it describes **one** entity: which components it is made of,
what names it can be pointed at by, and which entities belong under it. A scene of any size is written
this way — children nest, and a name reaches across the whole description:

```rust
use zlim_core::derive::{Component, IntoTemplate};
use zlim_core::entity::EntityId;
use zlim_core::world::World;
use zlim_scene::prelude::*;

#[derive(Component, Clone, Default, Debug, PartialEq)]
struct Scale(f32);

#[derive(Component, Clone, Default, Debug, PartialEq)]
struct Health {
    current: u32,
    max: u32,
}

/// A component that points at another entity. `#[derive(IntoTemplate)]` gives it a template whose
/// field is an `EntityTemplate`, which is what a `#Name` resolves to.
#[derive(Component, Clone, IntoTemplate)]
struct Target {
    to: EntityId,
}

/// One entity, with children of its own.
fn scene() -> impl Scene {
    scn! {
        // `#` declares a name for this entity.
        #Root

        // Entries describe its components.
        Health { current: 10, max: 20 }

        // `Children` describes the entities under it.
        Children [
            // Child (1)
            #Child
            Health { current: 3, max: 3 }

            // The entity separator.
            --
            // Child (2)
            Target { to: #Root } // a reference to an entity, as an `EntityId`
        ]
    }
}

/// Several entities, separated by `--`, sharing one name scope.
fn scene_list() -> impl SceneList {
    scn_list! {
        #Left
        Scale(1.0)
        --
        Scale(2.0)
        Parent(#Left) // `Parent` sets the parent edge
        // Children [ .. ] is available here too
    }
}

let mut world = World::alloc();

world.spawn_scene(scene(), None).unwrap();
world.spawn_scene_list(scene_list(), None).unwrap();
```

A name is bound by the entity that declares it, and only once every entity of the scene exists — so a
child can point back at its parent, and sibling entities can point at each other in either order:

```rust
use zlim_core::derive::Component;
use zlim_core::world::World;
use zlim_scene::prelude::*;

#[derive(Component, Clone, Default, Debug, PartialEq)]
struct Health {
    current: u32,
    max: u32,
}

let mut world = World::alloc();

let roots = world
    .spawn_scene_list(
        scn_list! {
            #A
            Children [ #B ]
            --
            #C
            Parent(#A)
        },
        None,
    )
    .unwrap();

// `Parent(#A)` is applied after every entity exists, so `A` ends up with both `B` and `C`.
assert_eq!(world.entity_owned(roots[0]).children().unwrap().len(), 2);
```

### Entries

An entry is one of:

| Written | Means |
|---|---|
| `#Name` | declare a name for this entity; it produces no component |
| `Type` | make sure the canonical template of `Type` exists (from its `Default`) |
| `Type { field: value, ... }` | edit the named fields of the canonical template; the rest keep their value |
| `Type(value, ...)` | the same, for a tuple struct (fields `0`, `1`, …) |
| `Type::function(args)` | replace the canonical template of `Type` with what the call produces |
| `~expression` | store what is written as the template itself, for example `~{ B(6) }` |
| `~template { field: value, ... }` / `~template(args)` | the same two, for a path that *is* a template and must not go through `IntoTemplate` |
| `@ expression` | include a scene where it is written, without caching |
| `: expression` | build on the cached scene asset the expression names |
| `--` | the entity separator, in `scn_list!` or inside `Children` |
| `Parent(value)` | an explicit parent edge for this entity |
| `Children [ entry* (-- entry*)* ]` | the entities under this one |

### Name references

A `#` alias can be used wherever an `EntityId` is wanted:

```rust,ignore
#Root
Sprite { color: RED }
Target { to: #Root } // `#Root` resolves to an `EntityId`
```

A name lives for one macro invocation: two `scn!` invocations never collide even if they both use
`#Root`, and one invocation used twice — a function that builds a scene — points at different entities
each time.

Referring to a name nobody declares is an error while resolving:

```rust,ignore
scn! { Target { to: #Root } } // error: `#Root` is not declared
```

Declaring one and not using it is fine:

```rust,ignore
scn! { #Root Health } // `#Root` is unused, and that is allowed
```

### Default templates and explicit replacement

`Type { … }`, `Type(…)` and `Type` describe a **canonical template**, which may be left incomplete:
whatever is left out keeps the value the canonical template already had, which the first description
of it takes from `Default`. A canonical template that exists is edited rather than created again.

`Type::function(…)` and `~…` describe a complete value, and always replace the canonical template
whether or not one exists.

`~` means "this path *is* the template, not the type it is built from": without it, the path goes
through `IntoTemplate` first. For a type whose template is the type itself — most `Clone + Default`
types — the two forms are the same, and only a type with a hand-written `IntoTemplate` distinguishes
them.

### Hierarchy

A scene describes its hierarchy with `Parent` and `Children`.

`scn!` has exactly one root, so it is usually written downwards with `Children`:

```rust,ignore
scn! {
    #A
    Children [
        #B
        --
        #C
        Children [ #D ]
    ]
}
```

`scn_list!` describes a flat list of entities, so either form works:

```rust,ignore
scn_list! {
    #A
    Children [ #B ]
    --
    #C
    --
    #D
    Parent(#C)
}
```

`Parent(value)` is applied once every entity of the scene exists, so it may name an entity declared
later. This is what makes the following behave as it looks:

```rust,ignore
scn_list! {
    #A
    Children [ #B ]
    --
    #C
    Parent(#A)
}
```

`A` ends up with both `B` and `C` as children.

The next one, on the other hand, contradicts itself:

```rust,ignore
scn_list! {
    #A
    --
    #B
    Children [
        #C // ❌️
        Parent(#A)
    ]
}
```

`Parent` and `Children` disagree here: `#C` is declared under `#B`, and `Parent(#A)` gives it a
different parent edge. Resolution does not report it and both take effect, so `#C` ends up under `#A`
— an explicit parent edge is applied after the entities exist. Write it another way.

Both may be written more than once: several `Children [ … ]` are appended in the order they are
written, which is the same as one list separated by `--`, and the last `Parent(…)` is the one that
takes effect.

### Cached scenes

`@ expression` includes a scene where it is written: the expression is a `Scene` of its own, resolved
in place, without caching.

The scene it includes is merged into the entity already being described rather than adding an entity,
in both macros: the templates, children and parent edge it describes all belong to the current entity.
To add an entity in `scn_list!`, separate it with `--`.

`: expression` is similar but names an asset: the expression has to convert into an asset path, which
usually means a literal, as in `: "scenes/player.scene"`. It needs the asset system.

A `:` entry has to come before every entry that describes a template or a child — a `#` name may
precede it, since a name describes neither — or it is a compile error. That follows from what it
means: the cached scene is applied first, and the entries after it edit the templates it contributed.
Write `@` instead when that restriction is in the way.

## How a scene works

### Templates

An entry of a scene is usually called a template, which is defined by `zlim-core`: the `Template` and
`IntoTemplate` traits describe templates, and a scene holds values of types that implement `Template`.

`zlim-core` implements both traits for most types through specialization-like tricks, and in the
default case the template of a type is the type itself.

As described above, `Type { .. }` produces a value of a type that implements `IntoTemplate`, which is
then turned into its template.

Templates are put together into *parts* in the order they are written, and parts are put together into
a scene. Parts resolve in order, so an entry written later can edit the template an earlier entry left
behind:

```rust
# use zlim_core::derive::Component;
# use zlim_core::world::World;
# use zlim_scene::prelude::*;
# use zlim_scene::ResolveContext;
# 
#[derive(Component, Clone, Default, Debug, PartialEq)]
struct Health {
    current: u32,
    max: u32,
}

let mut context = ResolveContext::new();
let mut scene = ResolvedScene::new();

// `Health { current: 3 }` edits the canonical `Health` — here the one `Default` produced — so `max`
// keeps what that template had.
scn! { Health { current: 3 } }
    .resolve(&mut context, &mut scene)
    .unwrap();

scn! { Health { current: 5, max: 9 } }
    .resolve(&mut context, &mut scene)
    .unwrap();

// Either way the scene holds one template: the canonical slot of `Health`.
assert_eq!(scene.templates().len(), 1);
```

For certain special components, you must explicitly mark them with the `IntoTemplate` macro
before they can be used in scene descriptions (for example, when a field contains a special
type such as `EntityId`). Please refer to the template documentation in `zlim-core` for details.

### Resolving

A list of templates makes a scene, and before a scene can be applied the right tree has to be resolved
out of it.

`Scene::resolve` does that, storing the structure in a `ResolvedScene`.

Resolution never touches the world, which is what lets one scene be resolved once and applied many
times, and what lets a scene be cached.

### Applying

Applying a resolved scene to an entity happens in four steps, in this order:

1. Every entity of the tree is spawned — the root is the entity the scene is applied to, and every
   child scene is spawned under the entity that describes it.

2. Every `#Name` the tree declares is bound to the entity that declares it.

3. Every entity gets its templates, written in one go through a `BundleWriter`: a component template
   pushes one column of the row, and a template whose output is a whole bundle hands over every
   component it carries — required components included, which the writer initialises.

4. Every explicit parent edge is applied, after which the hierarchy is final.

The order of steps 1 and 3 is what makes forward references work: the entities exist, and their names
are bound, before any template asks for them.

```rust
# use zlim_core::world::World;
# use zlim_scene::prelude::*;
# use zlim_scene::ResolveContext;
# 
let mut world = World::alloc();

let mut resolved = ResolvedScene::new();
().resolve(&mut ResolveContext::new(), &mut resolved).unwrap();

let entity = resolved.spawn(&mut world, None).unwrap();
assert!(world.get_entity_ref(entity).is_ok());
```

A scene carries data and structure, so nothing of the scene's own runs after its components are
written — exactly as with `EntityOwned::insert`, which is the same path: the components' own hooks
run, and no effect of the scene does.

### Hierarchy

zlim keeps the hierarchy in the world rather than in a component: `Parent` and `Children` are query
data over the entity tree, and the tree is maintained by spawning with a parent or by re-parenting an
entity. There is consequently no relationship *component* to insert, nothing to keep in sync through
hooks, and no relationship-hook mode to choose, so this crate has no equivalent of Bevy's
`Relationship` / `RelationshipTarget` / `RelationshipHookMode`:

- `SceneChildren` describes the entities a scene is the parent of. They are spawned with their parent
  already in place, so no edge has to be added afterwards — and nothing has to be announced, because
  an entity that is born in place was never anywhere else. The order of the list is the order of
  `Children`.

- `SceneParent` describes an explicit parent edge for the entity itself. This is what a scene applied
  to an existing entity uses; it is applied once every entity of the scene exists, which is what lets
  it point at a name declared later in the same scene. Carrying one is always an answer about the
  hierarchy: a scene with no parent edge leaves the entity where it is, and one whose parent names no
  entity moves the entity to the root.

An entity a scene creates is never *re-parented* in the sense transform propagation cares about: the
edge is recorded while the entity is being built, so it is applied with
`EntityOwned::reparent_without_signal` — the entity is new, and its own ticks already say so. Only a
scene applied to an entity that already has a place in the tree moves something the rest of the world
has to hear about, through `EntityOwned::reparent`.

### World Extensions

Spawning a scene is a two-step affair — resolve the description, then apply the resolved form — and
`WorldSceneExt` is the shorthand for doing both at once:

| Call | What it does |
|---|---|
| `world.spawn_scene(scene, parent)` | resolves it and spawns a new root for it |
| `world.spawn_scene_list(list, parent)` | resolves a list and spawns one root per entity |
| `world.apply_scene(scene, target)` | resolves it and applies it to an entity that already exists |

The resolved form can be kept instead, and applied again and again with `ResolvedScene::spawn` and
`ResolvedScene::apply` — which is what a `ScenePatch` does, and what makes a scene worth keeping
around.

The rest of the trait defers the same two steps, which is queueing a scene, further down.

### Commands Extensions

`Commands` is the deferred counterpart: a scene is described in a system, and built once the
schedule lets the command queue run.

| Call | What it does |
|---|---|
| `commands.spawn_scene(scene, parent)` | queues a scene and returns the `EntityCommands` of the entity it will describe |
| `commands.spawn_scene_list(list, parent)` | queues a list; nothing is returned, since a list describes several entities |
| `entity_commands.apply_scene(scene)` | queues a scene to be applied to an entity that already exists |

`spawn_scene` hands back an `EntityCommands` whose entity does not exist yet — its id is allocated
now, which is what makes it usable as a parent for further commands. `apply_scene` takes and returns
itself, so it chains:

```rust
# use zlim_app::App;
# use zlim_asset::plugin::AssetPlugin;
# use zlim_core::derive::Component;
# use zlim_scene::prelude::*;
# use zlim_scene::ScenePlugin;
# 
#[derive(Component, Clone, Default)]
struct Scale(f32);

let mut app = App::new();
app.add_plugins(AssetPlugin::default());
app.add_plugins(ScenePlugin);
app.build();

let target = app.main_world_mut().spawn_empty(None).id();

app.main_world_mut()
    .commands()
    .with_entity(target)
    .apply_scene(scn! { Scale(2.5) });

// The command queue runs at the end of the schedule, and the scene is built by the `SpawnScene`
// schedule after that.
app.update();
app.update();
```

Both traits have `try_` forms, which do not report an entity that is gone by the time the command
runs; the entity is looked up then, not now.

## Caching a scene

A scene can also live in the asset system, through the `ScenePatch` asset.

```rust
# use zlim_asset::assets::Assets;
# use zlim_core::world::World;
# use zlim_scene::ScenePatch;
# 
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
assert!(world.get_entity_ref(entity).is_ok());
```

`CachedSceneAsset` is how one scene builds on another: it names a patch by path, and the scene it
appears in applies the cached scene first. A template the cached scene describes can then be taken
over with `ResolvedScene::get_or_init_template`, which clones the cached template the first time it
is asked for — the cached copy is then skipped when the scene is applied. That copy-on-write is what
lets a patch edit a field of a scene it does not own, and it leaves the patch asset itself untouched:

```rust,ignore
scn! {
    : "scenes/player.scene"     // applied first
    Health { current: 5 }       // `Health` is cloned out of it, so `max` survives
}
```

`@ expression` is the uncached counterpart: the expression is a scene of its own, resolved exactly
where it is written, without a patch and without copy-on-write.

The resolved form is shared through an `Arc`, and a scene that includes a cached one holds that `Arc`
— so applying needs no asset lookups, and a patch that has not been resolved yet is refused while
resolving rather than while applying.

### Dependencies

`ScenePatch::load` starts loading what a description depends on, and
`Scene::register_dependencies` is where a description says what that is (`: "path"` registers the
patch it names). Resolving a patch resolves the patches it depends on first — depth-first, because
"loaded" does not mean "resolved" — and so does the schedule that spawns queued scenes.

### Scene lists

`SceneListPatch` is the list-shaped counterpart: it holds a `SceneList` and resolves it into one
`ResolvedScene` per entity, which are spawned as a group under one parent.

## Plugins

Scene assets live in the asset system, so the crate needs a few things registered:

- `AssetPlugin` — from `zlim-asset` — builds the asset sources, inserts the `AssetServer`, and runs
  the load pipeline. It has to be added first: registering an asset type needs the server.

- `ScenePlugin` — from this crate — registers `Assets<ScenePatch>` and `Assets<SceneListPatch>`
  through `init_asset`, registers the `SceneQueue` resource, and inserts the job that builds queued
  scenes into the `SpawnScene` schedule. It declares itself after `AssetPlugin` and after
  `MainSchedulePlugin`.

```rust
# use zlim_app::App;
# use zlim_asset::plugin::AssetPlugin;
# use zlim_scene::ScenePlugin;
# 
let mut app = App::new();
app.add_plugins(AssetPlugin::default());
app.add_plugins(ScenePlugin);
app.build();
```

### Queueing a scene

Once the plugins are in, a scene can also be built later: the `queue_*` entry points add a patch to
the asset system and record a request in the `SceneQueue` resource. The `SpawnScene` schedule —
between `Update` and `PostUpdate` — then resolves each patch once it and its dependencies are loaded,
builds it, and takes the request out.

What waits is the description. `queue_spawn_scene` makes the entity **at once** and leaves it empty,
so there is something to point at while the scene's assets are still loading, and the components
arrive later. That is what a level that streams in needs. The scene itself is named now and resolved
later; nothing is handed back, because a scene that names its own entity uses `#Name` for that.

`queue_spawn_scene_list` is the exception: a list describes entities of its own, so the entity it
names is a parent for them rather than a place to put the list, and nothing is made until the list is
ready. A request is not a component either — a queued description may be of an entity that does not
exist yet — so the requests live in the resource and never add to the archetypes.

```rust
# use zlim_app::App;
# use zlim_asset::plugin::AssetPlugin;
# use zlim_core::derive::Component;
# use zlim_scene::prelude::*;
# use zlim_scene::ScenePlugin;
# #[derive(Component, Clone, Default)]
# struct Scale(f32);
let mut app = App::new();
app.add_plugins(AssetPlugin::default());
app.add_plugins(ScenePlugin);
app.build();

app.main_world_mut()
    .queue_spawn_scene(scn! { Scale(1.5) }, None)
    .unwrap();

// The scene is built by the `SpawnScene` schedule, later in the frame.
app.update();
```

`queue_apply_scene` is the third form: it applies the scene *to* an entity that already exists
instead of making one. A request whose entity or parent is gone by the time it is answered is reported
through `zlim_log` and dropped — except a list whose parent is gone, which is spawned as roots instead
— and a patch that cannot be resolved yet keeps its request for a later run.
