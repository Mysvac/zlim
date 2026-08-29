# zlim-app

The application layer of the zlim engine: `App` creation, the plugin system,
the main loop (runner) and schedule driving.

## App structure

An `App` consists of one **main sub-app** and multiple **sub-apps**:

- **Main world**: `App` itself is a `SubApp` (its "main" sub-app), holding the
  `World` where game logic lives.

- **Sub world**: added via `App::insert_sub_app(label, sub_app)`. Each
  `SubApp` has its own independent `World`, plugin list, update schedule and
  an optional **Extract function**. A render world is the typical sub-app: it
  snapshots data from the main world each frame and renders without blocking
  the main world.

```rust, ignore
let mut app = App::new();
let mut render_sub_app = SubApp::new();
// Every sub-app owns its own world:
render_sub_app.world_mut().insert_resource(/* ... */);
// Extract step: copies data from the main world into the sub world each frame:
render_sub_app.set_extract(|main, sub| { /* main: &mut World, sub: &mut World */ });
app.insert_sub_app(Render, render_sub_app);
```

## App run flow

1. **Apply plugins**: `App::run` first calls `App::build` (skipped if it was
   already called). All added plugins run in `build → apply → cleanup` order.
2. **Start the runner**: once built, `App` drives the frame loop with the
   runner set via `set_runner` (or the default runner), then returns
   `AppExit`.

## Regular main loop flow

Every world (main and sub) needs a **default schedule**; worlds without one
are skipped.

The single-frame flow (`App::update`):

1. **Main world**:
   - `World::refresh_metadata` refreshes change-detection, time, and other
     metadata.
   - Run its default schedule (usually `Main`, provided by
     `MainSchedulePlugin`).

2. **Sub worlds** (in order):
   - If an **Extract function** is registered, run
     `extract(main_world, sub_world)` first. This lets you pull data from the
     main world, or mutate the main world.
   - `World::refresh_metadata` refreshes the sub world's change-detection,
     time, and other metadata.
   - Run its default schedule, which must be provided by yourself.
   - Clear the sub world's change trackers (`clear_trackers`).

3. Finally, clear the main world's change trackers (`clear_trackers`).

## Default runner

When no runner is set, the default runner runs **one frame and then exits**:

```rust
fn run_once(mut app: App) -> AppExit {
    app.update();                       // run one frame (main + sub worlds)
    app.should_exit().unwrap_or(AppExit::Success)   // check the exit event
}
```

For continuous execution (game loops, frame stepping, …), use
`ScheduleRunnerPlugin` (`RunMode::Loop`, …) or a custom runner.

`AppExit` is a message (`zlim_core::Message`) used to signal that the program
should exit. Loop-based apps typically check for this message every frame and
finish the current frame before exiting when it arrives.

## Plugins

Plugins let you add functionality to an `App` in a modular way.

Plugins are **lazy**: `add_plugins` only stores them and does nothing until
`App::build` (or `App::run`) runs.

Plugins have three stages:

| Stage | When and why |
|---|---|
| `build` | Initialize the plugin itself; add **dependency plugins** to the app; set the `apply` order of plugins (dependency graph). Plugins may still be added in this stage. |
| `apply` | The plugin list is now **immutable** (no more additions). Usually where plugin logic runs and the world is modified (registering systems/resources/messages, …). The execution order is the topological order of the dependency graph built in `build` (cycles panic). |
| `cleanup` | Clean up the plugin's own data. **After `cleanup` finishes, all plugins are removed**, so game logic added by a plugin must not rely on the plugin object itself remaining alive. |

`apply` is required; `build` and `cleanup` have default no-op
implementations.

## Log plugin

Logging configuration is **independent**: although it is named `LogPlugin`
(from `zlim-log`), it does **not** implement the `Plugin` trait. Add it
directly via `App::with_log_plugin(..)`.

`LogPlugin` is guaranteed to run **before all plugins** (it takes effect at
the very start of `App::build`).

If it is not added, logging is disabled — warnings and errors from the
program will not be displayed.

Logging is process-global; when multiple apps configure it, only the first
one takes effect.

## Task pool configuration

The async task pool (`zlim_task`'s `MainTaskPool`) is likewise **built-in
configuration rather than a plugin**.

Unlike `LogPlugin`, when the task pool configuration is not explicitly
provided, the **default configuration** is used (auto-picking a suitable
thread count):

```rust
app.config_task_pool(TaskPoolConfigs::default());
```

The task pool is process-global; when multiple apps configure it, only the
first one takes effect.

## MainSchedulePlugin

Defines a **prefabricated main-world execution logic** and is the base plugin
that most other zlim engine plugins depend on:

- `Main` — every frame runs First → PreUpdate → RunFixedMainLoop → Update →
  SpawnScene → PostUpdate → Last (order controlled by the `MainScheduleOrder`
  resource); on the first frame it also runs the startup schedules
  PreStartup → Startup → PostStartup first.

- `FixedMain` — runs FixedFirst → FixedPreUpdate → FixedUpdate →
  FixedPostUpdate → FixedLast.

- `RunFixedMainLoop` — consumes the accumulated fixed-step time
  (`World::step_fixed`), running `FixedMain` once per step (at most 50 steps
  per frame, so the main loop is never starved).

- Built-in housekeeping: `OptimizeDelayedCommands` in `Last` (applies
  deferred commands at the end of the frame) and `UpdateMessagesSignal` in
  `FixedPostUpdate` (rotates the double-buffered message queues once per
  fixed step).

Apps created with `App::new()` / `App::default()` add this plugin
automatically and set the main world's default schedule to `Main`.

## Other built-in plugins

- `ScheduleRunnerPlugin` — drives the `App` runner according to a given
  `RunMode` (`Loop`/`Once`, …). Suited for non-graphical applications; avoid
  combining it with other plugins of the same type (only one takes effect).

- `ShutdownPlugin` — graceful shutdown: the first `Ctrl+C`/
  `gracefully_exit()` sends `AppExit` for a clean exit (handlers registered
  via `on_exit` run once); a second call forces `std::process::exit`. This
  plugin relies on global state and is not suitable for multi-app concurrent
  environments such as regular tests.

- `PanicHandlerPlugin` — sets a sensible panic hook for `App` (on wasm it
  logs to the browser console).
