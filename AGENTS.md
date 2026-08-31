# AGENTS.md

Zlim is a DOP (Data-Oriented Programming) game engine framework written in Rust,
heavily inspired by Bevy's implementation.

## Development Environment

- **Rust version**: 1.98+ (edition 2024)
- **Resolver**: v3

## Common Commands

The standard five-step verification suite — run these before considering a
change complete:

```sh
# 1. Type-check the entire workspace
cargo check --workspace

# 2. Lint
cargo clippy --workspace -- -D warnings

# 3. Doc-tests + verify doc links
cargo doc --workspace --no-deps

# 4. Format
cargo fmt --all -- --check

# 5. Run tests
cargo test --workspace
```

Quick check for a single crate (substitute the crate name):

```sh
cargo check -p zlim-ptr
cargo clippy -p zlim-ptr -- -D warnings
```

## Code Style

Formatting is handled by rustfmt (see [`rustfmt.toml`](./rustfmt.toml)). Additional
conventions are in [`STYLE_GUIDE.md`](./STYLE_GUIDE.md). When writing code, you
must follow STYLE_GUIDE.md, including: section divider format, import style,
TODO comment format, and Safety documentation requirements for unsafe code.

Supplemental conventions not covered by STYLE_GUIDE.md:

### Documentation
- Every `pub` item should include a `//!` or `///` doc comment.
- README files should include: short description, module list, minimal example,
  design notes.

### Naming
- Follow standard Rust naming conventions (snake_case modules, PascalCase types).
- Internal types/functions use `pub(crate)` or `pub(super)` visibility.

### Performance
- Prefer pre-allocation, `const fn`, and compile-time constants.
- Use `#[inline(never)]`, `#[inline]` and `#[inline(always)]` when necessary.

### Error Handling
- Use `Option<T>` for values that may be absent (e.g., `preferences_dir()`).
- Use `Result<T, E>` for recoverable errors.
- Use `assert!`/`debug_assert!` to check invariants.

## Project Architecture

```
zlim (root facade crate, src/lib.rs)
└── zlim-internal (engine hub, crates/zlim-internal/)
    ├── zlim-cfg        (compile-time control macros, crates/zlim-cfg/)
    ├── zlim-ptr        (type-erased pointer abstractions, crates/zlim-ptr/)
    ├── zlim-reg        (CTOR metadata collector, crates/zlim-reg/)
    ├── zlim-os         (platform abstraction layer, crates/zlim-os/)
    ├── zlim-utils      (foundational utilities, crates/zlim-utils/)
    ├── zlim-log        (tracing-based logging, crates/zlim-log/)
    ├── zlim-task       (async task pool, crates/zlim-task/)
    ├── zlim-reflect    (reflection system, crates/zlim-reflect/)
    ├── zlim-math       (math types, crates/zlim-math/)
    ├── zlim-core       (ECS core, crates/zlim-core/)
    ├── zlim-app        (app & plugin framework, crates/zlim-app/)
    └── zlim-transform  (transform propagation, crates/zlim-transform/)

Auxiliary:
├── zlim-derive-utils (proc-macro utilities, crates/zlim-derive-utils/)
└── zlim-dylib (dynamic linking optimization, crates/zlim-dylib/)
    — enabled only with `feature = "dylib"`.
```

### Crate Responsibilities

- **`zlim`**
  - **Purpose**: public facade; forwards features to `zlim-internal`.
  - **Dependencies**: `zlim-internal`.

- **`zlim-internal`**
  - **Purpose**: engine hub; re-exports all subsystems; feature management.
  - **Dependencies**: all sub-crates.

- **`zlim-cfg`**
  - **Purpose**: compile-time control macros (`enabled!`, `disabled!`, `switch!`, `define_alias!`).
  - **Dependencies**: none.

- **`zlim-ptr`**
  - **Purpose**: type-erased pointers (`Ptr`, `PtrMut`, `OwningPtr`, `ThinSlice`, `ThinSliceMut`).
  - **Dependencies**: none.

- **`zlim-reg`**
  - **Purpose**: CTOR-driven registry/inventory pattern (like Bevy's plugin registration).
  - **Dependencies**: none.

- **`zlim-os`**
  - **Purpose**: platform bridge: standard directory paths + time API + sys crate re-exports.
  - **Dependencies**: `zlim-cfg`, windows-sys, web-time, android-activity.

- **`zlim-utils`**
  - **Purpose**: foundation: hash containers, sync primitives, memory pools, NonMax, extended collections.
  - **Dependencies**: serde_core, hashbrown, foldhash, smol_str, fastvec, event-listener.

- **`zlim-log`**
  - **Purpose**: tracing-based logging: `LogPlugin`, subscriber setup, `log` bridge.
  - **Dependencies**: tracing, tracing-subscriber, tracing-error, tracing-log.

- **`zlim-task`**
  - **Purpose**: async task pool: work-stealing thread pool, Scope, global singleton pool.
  - **Dependencies**: `zlim-cfg`, `zlim-os`, `zlim-utils`, async-task, futures-lite.

- **`zlim-reflect`**
  - **Purpose**: runtime reflection: `Reflect` trait, `TypeInfo`, type operations.
  - **Dependencies**: `zlim-utils`, serde_core, erased-serde.

- **`zlim-math`**
  - **Purpose**: math types (glam-based re-exports for transforms).
  - **Dependencies**: glam, `zlim-log`, `zlim-utils`, `zlim-reflect`.

- **`zlim-core`**
  - **Purpose**: ECS core: Entity, Component, World, Schedule, Tick, Error, Query, System, etc.
  - **Dependencies**: `zlim-reg`, `zlim-ptr`, `zlim-os`, `zlim-log`, `zlim-utils`, `zlim-task`, `zlim-reflect`, serde.

- **`zlim-app`**
  - **Purpose**: App/SubApp/Plugin lifecycle, runner, main schedules.
  - **Dependencies**: `zlim-core`, `zlim-log`, `zlim-task`, `zlim-reflect`, `zlim-utils`, `zlim-os`, `zlim-cfg`.

- **`zlim-transform`**
  - **Purpose**: `Transform`/`GlobalTransform` + parallel hierarchy propagation.
  - **Dependencies**: `zlim-app`, `zlim-core`, `zlim-math`, `zlim-task`, `zlim-reflect`, `zlim-utils`, `zlim-log`.

- **`zlim-derive-utils`**
  - **Purpose**: proc-macro helpers: crate path resolution via Cargo.toml.
  - **Dependencies**: syn, serde, toml.

- **`zlim-dylib`**
  - **Purpose**: dynamic linking helper crate for faster dev iteration (enabled only with `feature = "dylib"`).
  - **Dependencies**: `zlim-internal`.

#### Embedded Derive Crates

- `zlim-core/derive` 
  - `#[derive(Error)]` macro
  - `#[derive(Bundle)]` macro
  - `#[derive(Component)]` macro
  - `#[derive(Resource)]` macro
  - `#[derive(SystemParam)]` macro
  - `#[derive(QueryData)]` macro
  - `#[derive(ScheduleLabel)]` macro
  - `#[derive(ScheduleStage)]` macro
  - `#[derive(Message)]` macro
  - `#[job_fn]` macro
  - `job!` macro
  - `job_group!` macro

- `zlim-reflect/derive`
  - `#[derive(TypePath)]` macro
  - `#[derive(Reflect)]` macro

- `zlim-app/derive`
  - `#[derive(AppLabel)]` macro
  - `#[zlim_main]` attribute macro

All use `zlim-derive-utils::crate_path` to resolve paths, enabling correct
`::zlim::*` references from external crates.

### Key Design Patterns

1. **`core`-First Imports**: Prefer `core`-prefixed imports for standard
   library items; fall back to `std` only when the item does not exist in
   `core` (e.g. `Vec`, `String`, `Box`, collections).

2. **Feature Layering**: Sub-crate contents and their cargo features are
   re-exported uniformly by `zlim-internal`.  `zlim` re-exports `zlim-internal`
   and owns the high-level feature abstraction — broader features such as `3d`,
   `2d`, `dev` may be defined there to hide internal details.

3. **Platform Abstraction**: Handle platform differences with
   `zlim-cfg::switch!`, `cfg_select!`, the `define_alias!` macros, or raw
   `#[cfg]`, instead of scattering ad-hoc cfg checks.

4. **Hash Containers**: Prefer the fixed-seed hash containers from
   `zlim-utils` (e.g. `zlim_utils::hash::{HashMap, HashSet}`) for
   deterministic iteration order.  Proc-macro crates are free to use any
   hasher.

5. **Time Operations**: Use the time API defined in `zlim-os` for clock
   operations.  `Duration` may use the `core` definition directly, but
   `Instant` must use the type provided by `zlim-os`.

6. **Math Operations**: In upper-layer crates, floating-point functions
   should use the ones exported from `zlim-math`'s ops module, so precision
   stays consistent across the engine.

7. **Proc-Macro Paths**: In proc-macro implementations, always use fully
   qualified paths for standard library types, traits, and macros, e.g.
   `::core::assert!`, `::core::cmp::Eq`, `::core::marker::Sync`.  When a
   proc-macro needs paths into its own modules, query them through
   `zlim-derive-utils`' API (usually re-exported as the `path` submodule of
   the proc-macro crate).

8. **Global Memory Pools**: For data that must stay resident in memory
   indefinitely (never freed), use the memory pools provided by
   `zlim-utils::mem`.  For string retention prefer `zlim-utils::str::intern_str`
   to avoid duplicated allocation; avoid `Box::leak` for permanent retention.

9. **Parallelism**: Run parallel work through the async task pool from
   `zlim-task`, usually `MainTaskPool`.  In performance-sensitive paths,
   prefer the `zlim_task::cfg` macros to check single-threaded mode up
   front, avoiding the overhead of calling `MainTaskPool` when it is not
   needed.

10. **Doc Tests**: Doc tests should normally run to verify correctness.  Use
    `ignore` only for illustrative content that does not need to run; use
    `no_run` when the example compiles but does not need to execute.

11. **Module READMEs**: Every module needs an English `README.md` and a
    Chinese `README.zh.md`, but they should be written only after the
    module's own code is complete.

### zlim-core Module Overview

| Module | Status | Description |
|--------|--------|-------------|
| `entity` | ✅ Implemented | EntityId (index+version dual-word), lock-free allocator (Arc-based), EntityMap/EntityMapper |
| `component` | ✅ Implemented | Component trait (6 lifecycle hooks), ComponentDB global registry, `register_component!` macro |
| `tick` | ✅ Implemented | 32-bit Tick change detection (wrap-around safe), TicksRef/TicksMut/TicksSlice types, DetectChanges trait |
| `error` | ✅ Implemented | ZlimError (heap-allocated + Severity), `#[derive(Error)]` proc-macro |
| `job` | ✅ Implemented | Job trait + JobDB/JobLabel/JobGroup + `#[job_fn]`/`job!`/`job_group!` macros |
| `world` | ✅ Implemented | World struct, WorldCell (three-level safe access: read_only/data_mut/full_mut), DeferredWorld, NonSendWorld |
| `table` | ✅ Implemented | Dense columnar storage Table (organized by archetype), Column (BlobArray + TickArray), TableId, Tables manager |
| `bundle` | ✅ Implemented | Bundle trait (collect/write/apply_effect), DataBundle, tuple impls (0..=12), `#[derive(Bundle)]` |
| `borrow` | ✅ Implemented | Ref/Mut/SliceRef/SliceMut + corresponding Untyped* variants, integrated change detection |
| `resource` | ✅ Implemented | Resource trait + ResourceDB global registry, Resources with per-type storage slots, `register_resource!` macro |
| `ops` | ✅ Implemented | Entity/EntityRef/EntityMut/EntityOwned type definitions; implementation of Common Methods |
| `schedule` | ✅ Implemented | Schedule (job/group insertion by name or label, ordering, executors) + Schedules collection (owned by World) |
| `message` | ✅ Implemented | Message trait + `#[derive(Message)]`, double-buffered MessageQueue, Messages registry, MessageWriter/Reader/Mutator system params |
| `system` | ✅ Implemented | System trait + SystemParam/IntoSystem, params (Res/Query/Local/…), invoke / invoke_once / invoke_handle caching |
| `query` | ✅ Implemented | Query/QueryData/QueryFilter, QueryState, caches, Single, iteration (iter/slice/single) |
| `command` | ✅ Implemented | Deferred command system: Commands, command queue, deferred world mutations |
| `time` | ✅ Implemented | Time/Real/Virtual/Fixed clocks, Timer, Stopwatch, run conditions, delayed commands |
| `clone` | ✅ Implemented | Entity cloning: ComponentCloner, per-type clone strategies |
| `label` | ✅ Implemented | Interned label primitives (ScheduleLabel and other label traits) |
| `init` | ✅ Implemented | Global application initialization (startup CTOR collection) |
| `scene` | 🔸 Placeholder | Scene bump-storage staging (developing) |

---
