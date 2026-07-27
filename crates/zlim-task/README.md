# Asynchronous Task Executor

A lightweight async task pool for the zlim engine. Supports three execution
modes selected at compile time: multi-threaded work-stealing (Windows, Linux,
macOS, Android), single-threaded event loop (fallback), and WASM browser
microtask queue.

The overall design is inspired by `bevy_tasks`, implemented with
`async-task` and `futures-lite` rather than `async_executor`.

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

In single-threaded mode, the three types of `spawn` are almost equivalent.

### Methods

`spawn` and `scope` address different needs — the distinction goes beyond
parameter lifetimes.

**Background tasks (`spawn`, `spawn_local`, `spawn_to_main`):**

Take the single-threaded event loop as the canonical model.  These
functions enqueue work onto background queues that are **not** driven
automatically — the application must periodically call [`run_local`]
(e.g. at the start or end of each frame) to drain them.  Blocking on a
`Task` from `spawn` therefore deadlocks: the awaited task cannot run
until the thread yields to `run_local`, but the thread is blocked and
never reaches that call.

Even in multi-threaded mode — where worker threads do run `spawn` tasks
automatically and `scope` implicitly drives local tasks — you should
still follow the event loop model and periodically call `run_local`.
This keeps your code portable across all execution modes.

[`TaskPool::scope`] is an async analogue of [`std::thread::scope`].
It acts as a temporary event loop: it drives both thread-local executors
and the scope's own spawned tasks to completion, collects their results,
and only then returns.  Tasks may borrow stack-local data because the scope
guarantees they finish before the borrows expire.  Deadlock is virtually
impossible — the scope itself is the driver.

**`spawn_to_main` across threads:**

In single-threaded and WASM mode `spawn_to_main` is equivalent to
`spawn_local`.  In multi-threaded mode it dispatches the task to the
main thread's local queue.  When used inside a `Scope` on a worker
thread, the scope blocks until the main thread executes the task and
returns its result.  If the main thread is not periodically calling
`run_local` or `scope`, the worker will wait indefinitely.

### Multi-Threaded Mode

The default mode on Windows, Linux, macOS, and Android. Worker threads
use a work-stealing scheduler with per-worker local queues, a shared
global queue, and random victim selection for cross-worker stealing.
Each pool creates at least one worker thread; workers sleep when idle
and are woken one-by-one to avoid thundering herds.

### Single-Threaded Mode

Fallback mode for unknown platforms. All tasks execute on the current
thread — no background threads. The pool must be explicitly driven via
[`run_local`] or [`TaskPool::scope`] to make progress.

### WASM Mode

Tasks are submitted to the browser's microtask queue via `web_task`.
`spawn` / `spawn_local` / `spawn_to_main` all route to the browser
event loop. `scope` uses the Rust-side executors to drive tasks
synchronously.

## Examples

### Spawn

```rust
use zlim_task::TaskPool;
let pool = TaskPool::new();

// Send + 'static — dispatched to worker threads
let task = pool.spawn(async { 1 + 1 });

// !Send + 'static — stays on the current thread
let task = pool.spawn_local(async { 2 });

// Send + 'static — sent to main thread, wakes main waker
let task = pool.spawn_to_main(async { 3 - 1 });

// Drive the execution of local tasks.
zlim_task::run_local();
// Note: In multi-threaded mode, worker threads will
// automatically execute without the need for drivers.
```

Each returns a [`Task<T>`] handle which implements `Future`. The task
runs regardless of whether the handle is polled — use [`Task::detach`]
to run it in the background and drop the result, or [`Task::cancel`] to
cancel it.

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

`scope` blocks the current thread, drives all local tasks, and waits for
every spawned task to complete before collecting the results.

The `Scope` handle is `Send` — it can be moved across threads — so scopes
can be nested:

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

In single-threaded and WASM mode the three `spawn` variants behave
identically.  In multi-threaded mode `spawn` may execute on any worker
thread, `spawn_local` is pinned to the current thread, and
`spawn_to_main` is pinned to the main thread.

### Static Task Pools

Three global singleton pools for different workloads:

| Pool | Purpose |
|------|---------|
| [`MainTaskPool`] | Backend for parallel algorithms, and single-frame compute tasks |
| [`AsyncTaskPool`] | Compute-intensive tasks that may span multiple frames |
| [`IoTaskPool`] | IO-bound tasks with potentially long waits |

```rust
use zlim_task::{MainTaskPool, TaskPool};

// Initialize the global compute pool
MainTaskPool::get_or_init(TaskPool::new);

// Use it anywhere
let task = MainTaskPool::get().spawn(async { /* ... */ });
```

### ParallelSlice

Extension trait for parallel batch operations on slices:

```rust
use zlim_task::ParallelSlice;

let data = [3, 7, 9, 12];

let found = data.par_contains(&7);           // true
let pos   = data.par_position(|v| *v > 5);   // Some(1)
let doubled: Vec<_> = data.par_map(|v| *v * 2);
```

When a [`MainTaskPool`] is available, work is distributed across
worker threads. Otherwise, methods fall back to sequential iteration.

### block_on

```rust
let result = zlim_task::block_on(async { 42 });
assert_eq!(result, 42);
```

In multi-threaded mode, delegates to `futures_lite::future::block_on`
(or `async_io::block_on` with the `async_io` feature). In
single-threaded/WASM mode, busy-waits polling the future.

### Feature Flags

| Flag | Effect |
|------|--------|
| `single_thread` | Force single-threaded mode regardless of platform |
| `async_io` | Use `async_io::block_on` instead of `futures_lite` |

## Platform Modes

| Mode       | Platforms                        | `spawn` target                |
|------------|----------------------------------|-------------------------------|
| multi      | Windows, Linux, macOS, Android   | PoolExecutor (work-stealing)  |
| single     | unknown / `single_thread`      | LocalExecutor                 |
| wasm       | WASM                             | browser microtask queue        |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE)
or [MIT license](LICENSE-MIT) at your option.

[async_task]: https://docs.rs/async-task
[async_io_block_on]: https://docs.rs/async-io/latest/async_io/fn.block_on.html
[futures_lite]: https://docs.rs/futures-lite
