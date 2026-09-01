# zlim-core

The core layer of the Zlim Engine: a game engine implementation with an
ECS-style architecture, adapted from the Bevy engine.

## The game program

This crate provides the concrete implementation of the game **World**.

A game program (`App`) is made of multiple **World**s, and each World drives
its own ECS framework. The game main loop simply drives the per-frame logic
of each World in turn; see the `zlim_app` implementation for details.

## Core concepts

### 2.1 Component

Components are **plain Rust structs**, associated with entities via
`#[derive(Component)]`:

```rust
use zlim_core::prelude::*;

#[derive(TypePath, Component, Clone)]
struct Position { x: f32, y: f32 }

#[derive(TypePath, Component, Clone)]
struct Health { current: f32, max: f32 }
```

An entity can hold any number of **differently-typed** components, but only
**one component of each type**.

Components are spawned together with the entity (see 2.2), and can also be
added or removed at runtime through entity handles or `Commands` (see 2.11).

See the `component` module docs for more details and examples.

### 2.2 Entity

An entity is a **thin handle**, like a GameObject in traditional game
engines: it carries no data itself, it is just a "container id" for
components.

```rust
use zlim_core::prelude::*;

#[derive(TypePath, Component, Clone)]
struct Position { x: f32, y: f32 }

let mut world = World::alloc();

// Spawn a root entity (the second argument is the parent; `None` means no parent)
let root = world.spawn(Position { x: 0.0, y: 0.0 }, None).id();

// Spawn a child entity attached under `root`
let child = world.spawn(Position { x: 1.0, y: 1.0 }, Some(root)).id();

// Read the entity's components through the handle
let entity = world.entity(child);
assert_eq!(entity.get::<Position>().unwrap().x, 1.0);
```

Unlike a standard ECS, zlim-core entities have **hierarchical structure
built in** — every entity can have one parent and any number of children.

See the `entity` module docs for more details and examples.

### 2.3 Resource

Resources store **globally (per-world) unique** data, such as the game clock,
message queues, configuration, etc.

A resource **does not belong to any entity**; it is uniquely identified by
its Rust type — at most one value of a given resource type exists in a
world.

```rust
use zlim_core::prelude::*;

#[derive(TypePath, Resource)]
struct Score(u32);

let mut world = World::alloc();
world.insert_resource(Score(100));
assert_eq!(world.get_resource::<Score>().unwrap().0, 100);
```

Resources are accessed from systems with `Res<T>` (read-only) and `ResMut<T>`
(mutable).

See the `resource` module docs for more details and examples.

### 2.4 World

A **World** is a data container that stores the entities, components,
resources, schedules, etc. of that world, and exposes operations to modify
them.

Entities and components always live in some world; resources and schedules
are isolated per world as well.

```rust, ignore
use zlim_core::prelude::*;

let mut world = World::alloc(); // allocate a new world

// Spawning entities, inserting resources, registering schedules ...
// every operation happens on a world
world.spawn_empty(None);
world.insert_resource(Score(0));
```

See the `world` module docs for more details and examples.

### 2.5 System

A system is a **Rust function** satisfying certain requirements, callable via
`World::invoke`.

`invoke` inspects the function **signature** to determine which parameters
are needed, then automatically constructs them from the World and runs the
system. System instances are **cached** by default to speed up repeated
calls (caching internal data such as `Local` parameters and query state).

```rust
use zlim_core::prelude::*;

#[derive(TypePath, Component, Clone)]
struct Position { x: f32, y: f32 }

#[derive(TypePath, Component, Clone)]
struct Velocity { x: f32, y: f32 }

// System parameters are fully declared by the signature:
// `Query` fetches components, `Res` reads resources ...
fn movement(mut query: Query<(&mut Position, &Velocity)>) {
    for (mut position, velocity) in query.iter_mut() {
        position.x += velocity.x;
        position.y += velocity.y;
    }
}

let mut world = World::alloc();
let bundle = (Position { x: 0.0, y: 0.0 }, Velocity { x: 1.0, y: 2.0 });
let player = world.spawn(bundle, None).id();

// invoke builds the parameters from the signature and runs the system
world.invoke(movement, ()).unwrap();
// you can also use `invoke_once` to avoid caching the system's data

let entity = world.entity(player);
let position = entity.get::<Position>().unwrap();
assert_eq!((position.x, position.y), (1.0, 2.0));
```

See the `system` module docs for more details and examples.

### 2.6 Job

**A Job is a special kind of system**:

- It has a **stable string identifier** (`JobId` = `name` + `group`, both
  findable by name at runtime);

- It has **no input parameters** (no `SystemInput`; `SystemParam` is fully
  supported);

- Its return value can be converted to the standard `ZlimResult<()>`
  (`()`, `bool`, `Result<(), E>`, or `Result<bool, E>`);

- Regular systems run through `World::invoke`; Jobs must be inserted into a
  World's **Schedule**;

- A Schedule **automatically parallelizes its Jobs as much as possible**
  (detecting data-access conflicts from the function signatures, running
  non-conflicting jobs in parallel), and also lets you explicitly order
  certain jobs;

- **Cycle detection is built in**: a dependency cycle causes the program to
  Panic.

You can mark a system-able **function** with the `#[job_fn]` attribute, or a
system-able **expression** with the `job!` macro:

```rust
use zlim_core::prelude::*;

#[job_fn(type = GreetJob)]
fn greet() { println!("Hello World!"); }

job! {
    type: PipeGreetJob,
    system: (|| true).pipe(|In(flag)| { let _ = flag; }),
}
```

`type` names a type, which generates an empty marker type used to identify
the job. In other words, **jobs are strongly typed**.

By default a non-generic job also **auto-registers** itself at startup (so it
shows up in `JobDB::collect`); pass `auto_register = false` to skip the
registration. Generic jobs cannot be auto-registered — omit the attribute or
set it to `false`, and register each instantiation manually with
`JobDB::register`.

You can also customize `name` and add run conditions:

```rust
use zlim_core::prelude::*;

// Local<T>: system-local data, should support `Default`
fn once(mut flag: Local<bool>) -> bool {
    // The first time it is called, the value is `false` (bool::default()).
    if !*flag { *flag = true; true } else { false }
}

#[job_fn(type = GreetJob, name = "test::GreetJob", run_if = once)]
fn greet() { println!("Hello World!"); }
```

Jobs can be organized efficiently in batches with **JobGroup**:

```rust
use zlim_core::prelude::*;

#[job_fn(type = JobA, name = "group_job_a")]
fn job_a() {}

#[job_fn(type = JobB, name = "group_job_b")]
fn job_b() {}

job_group! {
    type: MyGroup,
    name: "my_group",
    jobs: [JobA, JobB],
    order: [[JobA, JobB]], // declares the execution order: B runs after A
}
```

See the `job` module docs for more details and examples.

### 2.7 Schedule

A **Schedule is a container for Jobs** that plans the execution order of its
jobs.

Usually **schedules run serially with respect to each other** (e.g. Update
before Render), but **the jobs inside a schedule can run in parallel**.

The hierarchy is: `Schedule > ScheduleStage > JobGroup > Job`:

- Every Job belongs to a **JobGroup** (the anonymous group if unspecified);

- Every JobGroup and Job also belongs to a **Stage** (the anonymous stage if
  unspecified);

- Two stages with no ordering constraint between them can run entirely in
  parallel.

This hierarchy lets you organize job execution order efficiently.

A named Stage automatically gets `StageBegin` / `StageEnd` marker jobs, which
enable ordering constraints between stages:

- `insert_order`: strong order — the next job runs after the previous one
  **succeeded**, and its deferred commands are guaranteed visible

- `insert_weak_order`: weak order — the next job runs after the previous one
  finished, whether it succeeded or not; deferred commands are guaranteed
  visible

- `insert_relaxed_order`: relaxed order — like `weak_order`, but deferred
  commands are **not** guaranteed visible

```rust
use zlim_core::prelude::*;

#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MainLoop {
    Update,
    Render,
}

#[derive(TypePath, ScheduleStage)]
enum FixedStage {
    PreUpdate,
    Update,
}

#[job_fn(type = PhysicsStep, name = "physics_step")]
fn physics_step() {}

#[job_fn(type = RenderFrame, name = "render_frame")]
fn render_frame() {}

let mut world = World::alloc();

// Update schedule: PhysicsStep belongs to the Update stage
world.schedules_mut()
    .entry(MainLoop::Update)
    .insert::<PhysicsStep>(FixedStage::Update);
    // ^ the stage is auto-inserted when it doesn't exist

world.schedules_mut()
    .entry(MainLoop::Update)
    .insert_stage(FixedStage::PreUpdate);
// note: `insert_order` does not insert stages;
// make sure the stages exist before calling it.

// Set the execution order of `FixedStage`:
// `PreUpdate`'s stage-end runs before `Update`'s stage-begin.
world.schedules_mut()
    .entry(MainLoop::Update)
    .insert_order(&[
        FixedStage::PreUpdate.stage_end(),
        FixedStage::Update.stage_begin(),
    ]);

// Render schedule: serial with Update
world.schedules_mut()
    .entry(MainLoop::Render)
    .insert::<RenderFrame>(()); // anonymous stage

// Run the schedules
world.run_schedule(MainLoop::Update);
world.run_schedule(MainLoop::Render);
```

Within a single Schedule, jobs are distinguished by their
[`JobId`](crate::job::JobId), so no two identical jobs can exist.

A [`JobId`](crate::job::JobId) has two parts: `name` and `group` — the job's
own name and the group it belongs to. Jobs added through a
[`JobGroup`](crate::job::JobGroup) naturally belong to that group, while jobs
added directly all belong to the anonymous group.

Note that a [`JobId`](crate::job::JobId) has no `stage`:
[`ScheduleStage`](crate::schedule::ScheduleStage) is only used to organize
execution order, never to distinguish jobs.

### 2.8 Query

Query is an extremely important system parameter for efficiently accessing
**entity and component** data.

For how component data is stored, see the `table` module docs; here we only
show how queries are used:

```rust
use zlim_core::prelude::*;

#[derive(TypePath, Component, Clone)]
struct Position { x: f32, y: f32 }

#[derive(TypePath, Component, Clone)]
struct Player;

// Iterate the `Position` of every entity that has a `Player` component
fn total_x(query: Query<&Position, With<Player>>) -> f32 {
    query.iter().map(|p| p.x).sum()
}
```

A query is composed of two parts:

- **QueryData** — what to fetch: `&T` / `&mut T` / `EntityId` / tuple
  combinations, etc.
- **QueryFilter** — which entities match: `With<T>` / `Without<T>` /
  `Changed<T>` / `Added<T>` / `And` / `Or`, etc.

See the `query` module docs for more details and examples.

### 2.9 Message

The engine has a built-in **MessageQueue** implementation.

`MessageQueue` is a **double-buffered, rotating** message queue: messages are
written to the write buffer; after rotation (`update`) they switch to the
read buffer for consumption, and the old read buffer is cleared.

**A single message can be read by multiple consumers**; each consumer keeps
its own independent cursor, without interfering with the others.

```rust
use zlim_core::prelude::*;

#[derive(TypePath, Message)]
struct Ping;

let mut world = World::alloc();
world.register_message::<Ping>(); // register at startup, creating the queue resource

// Producer system
fn producer(mut writer: MessageWriter<Ping>) {
    writer.write(Ping);
}

// Consumer system
fn consumer(mut reader: MessageReader<Ping>) {
    for _ in reader.read() {
        println!("received!");
    }
}
```

The buffers are implemented through the built-in resource system, with system
parameter helpers such as `MessageWriter` / `MessageReader` / `MessageMutator`.

For the rotation and message-drop policy, see the `message` module docs — in
short, it **usually keeps two frames of data**, so jobs that run every frame
never miss messages.

### 2.10 Change detection

zlim-core tracks changes to **all resource** and **component** data, with two
moments: **Added** and **Changed**.

- **Added** — set when the data is added (not on overwrite);
- **Changed** — updated when you take a **mutable reference** to a
  resource/component;

Change detection is **time-interval based**: detection through a World only
sees changes that happened in the **current frame**; detection through a Job
only sees changes that happened **after that Job's previous run (not
including the previous run itself)**.

```rust
use zlim_core::prelude::*;

#[derive(TypePath, Component, Clone)]
struct Health { current: f32, max: f32 }

// Query filters: only handle components that were added /
// changed since the last run
fn on_created(query: Query<&Health, Added<Health>>) {
    for health in query.iter() {
        println!("new entity with max {}", health.max);
    }
}

fn on_damaged(query: Query<&Health, Changed<Health>>) {
    for health in query.iter() {
        println!("health changed to {}", health.current);
    }
}

// Resources expose change state as well
fn check_time(time: Res<Time>) {
    if time.is_changed() {
        println!("time changed!");
    }
}
```

### 2.11 Commands — Deferred Commands

Operations such as spawning entities and adding/removing resources need an
**exclusive reference** to the world, which prevents the corresponding
System / Job from running in parallel.

But usually these operations are conditional and should not force the whole
system to take the world exclusively. In that case, use **Commands** to push
the commands into a **deferred queue** — no exclusive access needed, which
improves parallelism.

**The Schedule automatically inserts sync points** that execute these
deferred commands when needed, guaranteeing visibility.

```rust
use zlim_core::prelude::*;

#[derive(TypePath, Component, Clone)]
struct Position { x: f32, y: f32 }

// Collect commands, apply them all at once later
fn spawn_units(mut commands: Commands, query: Query<EntityId>) {
    for entity in query.iter() {
        let _ = commands.with_entity(entity).insert(Position { x: 0.0, y: 0.0 });
    }
    let _ = commands.spawn_empty(None);
}

// Time-based **delayed** commands (see the time module)
fn delayed_spawn(mut commands: Commands) {
    let _ = commands.delayed().secs(1.0).spawn_empty(None);
}
```

See the `command` module docs for more details and examples.

### 2.12 Time

zlim-core has a built-in **clock system**, including a real clock, a virtual
clock, a fixed-timestep clock, etc., all stored as resources.

The clocks are usually updated through the `refresh_metadata` function (called
automatically once per frame by the app's main loop).

```rust
use zlim_core::prelude::*;

// Read the clock resource directly in a system
fn game_time(time: Res<Time>) {
    let delta = time.delta();     // time elapsed since the previous frame
    let elapsed = time.elapsed(); // total time since the clock started
}

// In the engine loop: advance the clocks, then consume fixed steps
let mut world = World::alloc();
World::refresh_metadata(&mut world);

if World::step_fixed(&mut world) {
    // Exactly one fixed timestep accumulated: run one fixed-step pass
    // (physics, etc.)
}
```

See the `time` module docs for more details and examples.
