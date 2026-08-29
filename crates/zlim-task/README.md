# zlim-task

A lightweight yet efficient async task pool designed for the zlim engine.

Provides a unified task pool interface so the implementation can be extended later.

- On mainstream platforms, multi-threaded mode is used by default (Linux, Windows, Android, macOS).

- On WASM targets, the browser-specific single-threaded mode is used.

- On other unknown platforms, or when the `single_thread` feature is explicitly enabled, it falls back to the standard single-threaded mode.

## Architecture

```text
┌─────────────────────────────────────────────────────┐
│                      TaskPool                       │
│  ┌──────────┐  ┌─────────────┐  ┌────────────────┐  │
│  │  spawn   │  │ spawn_local │  │ spawn_to_main  │  │
│  │ (Send)   │  │  (!Send)    │  │  (Send → main) │  │
│  └────┬─────┘  └─────┬───────┘  └───────┬────────┘  │
│       │              │                  │           │
│  ┌────▼──────┐ ┌─────▼───────┐  ┌───────▼────────┐  │
│  │ Pool Exec.│ │ Local Exec. │  │  Main Executor │  │
│  │           │ │   Executor  │  │                │  │
│  │ working-  │ │             │  │                │  │
│  │ stealing  │ │ Background  │  │   Background   │  │
│  └───────────┘ └─────────────┘  └────────────────┘  │
│  ┌──────────────────────────────────────────────┐   │
│  │                  Scope                       │   │
│  │     spawn / spawn_local / spawn_to_main      │   │
│  │                                              │   │
│  │     Drives Executors + Collects Results      │   │
│  └──────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
```

- Semantically, `spawn` creates a task that may be executed by any thread, so it requires `Send`.

- Semantically, `spawn_local` creates a task that runs on the current thread, so it does not require `Send`.

- `spawn_to_main` sends a task to the main thread for execution, which also requires `Send`.

These only differ meaningfully in multi-threaded mode; in single-threaded mode all three are equivalent.

All of the functions above return a `Task<T>` handle that implements `Future`. You can cancel a task with `Task::cancel`, or drop the handle with `Task::detach` without affecting the task's execution (the task's return value is discarded).

The Scope family likewise has three functions: `spawn`, `spawn_local`, and `spawn_to_main`.

The difference is that a Scope blocks until all of its inner tasks complete, then directly returns `Vec<T>`. Therefore, a scope can create tasks that hold non-`'static` parameters.

## Modes

### Multi-Threaded Mode

The default mode on Windows, Linux, macOS, and Android.

Worker threads use a work-stealing scheduler: each worker has its own local queue, plus a shared global queue, and steals work across threads through random selection.

Each pool creates at least one worker thread; idle workers sleep and are woken one by one to avoid thundering herds.

By default, multi-threaded mode starts a **fake main thread** on the first `TaskPool` creation: it owns the global `MainExecutor` and keeps polling it until the process exits. Therefore, regardless of which thread created the task pool, `spawn_to_main` tasks are always routed to this thread, which ensures consistent behavior in test environments.

In real applications, however, it is recommended to call [`designate_main_thread`] at the very beginning of the program to mark the current thread as the main thread; in that case, creating a `TaskPool` will not spawn the fake main thread. The `#[zlim_main]` macro provided by the `zlim_app` module calls it automatically at the start of the main function.

Note that `designate_main_thread` should not be used in test environments. Test cases run on various child threads; marking a child thread with `designate_main_thread` can easily cause long sleeps (deadlocks).

### Single-Threaded Mode

The fallback mode for unknown platforms. All tasks execute on the current thread — no background threads.

The pool must be explicitly driven via [`run_local`] or [`TaskPool::scope`] to make progress.

### WASM Mode

Tasks are submitted to the browser's microtask queue via `web_task`.

`spawn` / `spawn_local` / `spawn_to_main` all route to the browser event loop.

`scope` drives tasks synchronously using the Rust-side executors.

## Semantics

### Background Tasks

The `spawn` family on `TaskPool` itself is used to create "background tasks".

Semantically, these functions put work into background queues that are **not** driven automatically; the actual execution point varies:

- In multi-threaded mode, these tasks may be completed in the background by worker threads, or may wait for the main thread to drive local tasks.

- In the standard single-threaded mode, these tasks only run when [`run_local`] is explicitly called.

- On WASM, they may only run after the Rust-side single-frame logic ends and the executor is handed back to the browser.

Therefore, blocking on a `Task` returned by `spawn` will very likely deadlock. It is almost always used for async background tasks with non-blocking waits, receiving results through polling or message passing.

### Scoped Tasks

`TaskPool::scope` is used to create scoped tasks. Unlike `TaskPool::spawn`, the scope itself drives local task execution, guaranteeing that tasks complete.

In single-threaded mode (including WASM), all tasks created by `scope` go to the local queue, regardless of which `spawn` function is used.

Multi-threaded mode is more complex: `spawn` sends the task to any worker thread, `spawn_local` puts it in the current thread's local queue, and `spawn_to_main` sends it to the "main thread".

Thanks to the main thread (a dedicated fake one unless [`designate_main_thread`] was called up front), none of the three usually deadlocks. But be aware of the program's semantics: consider handing the main-function logic to the main thread at startup (via `spawn_to_main`).

## Performance

- **Single-threaded mode**: task dispatch is roughly 2× faster than `bevy_tasks`. Tasks go straight into a thread-local block-list (`BlockList`), avoiding the many atomic-operation overheads of `async_executor`. Execution speed is theoretically identical, but thanks to the faster dispatch and the compact storage, small tasks run about **10% faster** in practice.

- **Multi-threaded mode**: task dispatch is roughly 4× faster than `bevy_tasks`. Compute-bound tasks (where computation dominates dispatch overhead) take roughly the same time.

## Examples

### Spawn

```rust, ignore
use zlim_task::TaskPool;
let pool = TaskPool::new();

// Send + 'static — dispatched to worker threads
let task = pool.spawn(async { 1 + 1 });

// !Send + 'static — stays on the current thread
let task = pool.spawn_local(async { 2 });

// Send + 'static — sent to the main thread, wakes the main waker
let task = pool.spawn_to_main(async { 3 - 1 });
```

### Scope

```rust
use zlim_task::TaskPool;
let pool = TaskPool::new();

let results: Vec<i32> = pool.scope(|scope| {
    scope.spawn(async { 1 + 1 });     // → worker threads
    scope.spawn_local(async { 2 });   // → current thread
    scope.spawn_to_main(async { 3 - 1 }); // → main thread
});

assert_eq!(&results, &[2, 2, 2]);
```

`scope` blocks the current thread, drives all local tasks, and collects the results only after every spawned task completes.

The `Scope` handle is `Send` — it can be moved across threads — so scopes can be nested:

```rust
use zlim_task::TaskPool;
let pool = TaskPool::new();

let results: Vec<i32> = pool.scope(|scope| {
    scope.spawn(async {
        scope.spawn_local(async { 1 });
        2 - 1
    });
});

assert_eq!(&results, &[1, 1]);
```

## Static Task Pools

Three global singleton pools for different workloads:

| Pool | Purpose | Default threads (multi) |
|------|---------|-------------------------|
| [`MainTaskPool`] | Backend for parallel algorithms, and single-frame compute | 50% of available (≥ 1) |
| [`AsyncTaskPool`] | Compute-intensive tasks that may span multiple frames | 25% of available (≥ 1) |
| [`IoTaskPool`] | IO-bound tasks with potentially long waits | 25% of available (≥ 1) |

Each pool is lazily initialized: the first call to `get()` (or any `Deref` usage) implicitly creates a `TaskPool` with the default configuration shown above. No explicit setup is required for typical use:

```rust, ignore
use zlim_task::{MainTaskPool, TaskPool};

// Implicit init on first access — just use it
let task = MainTaskPool::get().spawn(async { /* ... */ });
```

For custom configuration, call `try_init` before the first access:

```rust, ignore
use zlim_task::{MainTaskPool, TaskPoolBuilder};

// Custom init — must be called before first get()
let did_init = MainTaskPool::try_init(|| {
    TaskPoolBuilder::new()
        .thread_count(8)
        .thread_name("CustomMain")
        .build()
});

assert!(did_init); // true on first call, false if already initialized
// In single threaded mode, `try_init`may always return false.
```

**Multi-threaded mode:** `try_init` returns `true` when the pool was not yet initialized (custom config applied), or `false` if already initialized.

**Single-threaded / WASM mode:** all three pools share a single global `TaskPool`. `try_init` always returns `false` — custom initialization is not supported in these modes.

### TaskPoolConfigs

[`TaskPoolConfigs`] initializes all three global pools in one call, splitting the available threads according to its per-pool [`TaskPoolConfig`] fields — by default `25%` for `IoTaskPool`, `25%` for `AsyncTaskPool`, and the remaining threads for `MainTaskPool`:

```rust
use zlim_task::TaskPoolConfigs;

TaskPoolConfigs::default().apply();
```

Call it once during startup, before any pool is first accessed (e.g. via `MainTaskPool::get()`). In single-threaded / WASM mode it is a no-op; in a test environment it returns early if a pool was already initialized.

Note: if you use `MainTaskPool`, `AsyncTaskPool`, or other global pools with implicit initialization, make sure their "first access" happens before other task pools are created — unless [`designate_main_thread`] was called up front, the fake main thread is started by the first `TaskPool` creation, and `spawn_to_main` tasks are routed to it.

## ParallelSlice

Extension trait for parallel batch operations on slices:

```rust
use zlim_task::ParallelSlice;

let data = [3, 7, 9, 12];

let found = data.par_contains(&7);           // true
let pos   = data.par_position(|v| *v > 5);   // Some(1)
let doubled: Vec<_> = data.par_map(|v| *v * 2);
```

When the `multi_thread` cfg path is enabled, work is distributed across worker threads via [`MainTaskPool`]. When multi-threading is disabled (single-threaded or WASM builds), the methods fall back to sequential iteration.

## block_on

```rust
let result = zlim_task::block_on(async { 42 });
assert_eq!(result, 42);
```

Blocks the current thread on a future while simultaneously driving the
async executors to avoid deadlock.

- On the **main thread** (multi-threaded mode), the `MainExecutor` and
  `LocalExecutor` are driven while blocking.
- On a **worker thread**, the pool executor and the `LocalExecutor` are
  driven while blocking.
- In single-threaded / WASM mode, the executors are driven while blocking.

The thread is parked via `futures_lite::future::block_on` by default, or
`async_io::block_on` when the `async_io` feature is enabled.

## invoke_on_main

Sends a single closure to the main thread for execution and blocks until
the result is available.

If called from a worker thread, the worker's own tasks are **not** driven
during the wait — in extreme cases this may cause all threads to sleep
(deadlock). It is therefore mainly called from the top-level entry point
and is usually only used internally by `App::run`.

## Cargo Features

| Flag | Effect |
|------|--------|
| `single_thread` | Force single-threaded mode regardless of platform |
| `async_io` | Use `async_io::block_on` instead of `futures_lite` |

---
