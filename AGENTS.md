# AGENTS.md

Zlim is a DOP (Data-Oriented Programming) game engine framework written in Rust,
heavily inspired by Bevy's implementation.

## Common Commands

The standard five-step verification suite — run these before considering a
change complete:

```sh
# 1. Type-check the entire workspace
cargo check --workspace

# 2. Lint
cargo clippy --workspace -- -D warnings

# 3. Doc-tests + verify doc links
cargo doc --workspace --no-deps --document-private-items
RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc --workspace --no-deps

# 4. Format (requires nightly)
cargo fmt --all -- --check

# 5. Run tests
cargo test --workspace
```

Quick check for a single crate (substitute the crate name):

```sh
cargo check -p zlim-ptr
cargo clippy -p zlim-ptr -- -D warnings
```

Faster dev iteration with dynamic linking:

```sh
cargo check --features dylib
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
- Doc tests use the `no_run` attribute (the project may not fully compile yet).

### Naming
- Follow standard Rust naming conventions (snake_case modules, PascalCase types).
- Internal types/functions use `pub(crate)` or `pub(super)` visibility.

### Performance
- Reflection-related functions (`TypePath`, `concat`, etc.) are heavily called in
  generic contexts. Avoid inline bloat; use `#[inline(never)]` when necessary.
  Prefer pre-allocation, `const fn`, and compile-time constants.

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
    ├── zlim-task       (async task pool, crates/zlim-task/)
    ├── zlim-reflect    (reflection system, crates/zlim-reflect/)
    └── zlim-core       (ECS core, crates/zlim-core/)

Auxiliary:
├── zlim-derive-utils (proc-macro utilities, crates/zlim-derive-utils/)
└── zlim-dylib (dynamic linking optimization, crates/zlim-dylib/)
    — enabled only with `feature = "dylib"`.
```

### Crate Responsibilities

| Crate | Purpose | no_std | Key Dependencies |
|-------|---------|--------|------------------|
| `zlim` | Public facade; forwards features to `zlim-internal` | No | `zlim-internal` |
| `zlim-internal` | Engine hub; re-exports all subsystems; feature management | No | All sub-crates |
| `zlim-cfg` | Compile-time control macros (`enabled!`, `disabled!`, `switch!`, `define_alias!`) | Yes | None |
| `zlim-ptr` | Type-erased pointers (`Ptr`, `PtrMut`, `OwningPtr`, `Slice`, `SliceMut`) | Yes | None |
| `zlim-reg` | CTOR-driven registry/inventory pattern (like Bevy's plugin registration) | Yes | None |
| `zlim-os` | Platform bridge: standard directory paths + time API + sys crate re-exports | No | `zlim-cfg`, windows-sys, web-time, android-activity |
| `zlim-utils` | Foundation: hash containers, sync primitives, memory pools, NonMax, extended collections | No | serde_core, hashbrown, foldhash, smol_str, fastvec, event-listener |
| `zlim-task` | Async task pool: work-stealing thread pool, Scope, global singleton pool | No | `zlim-cfg`, `zlim-os`, `zlim-utils`, async-task, futures-lite |
| `zlim-reflect` | Runtime reflection: `Reflect` trait, `TypeInfo`, type operations | No | `zlim-utils`, serde_core, erased-serde |
| `zlim-core` | ECS core: Entity, Component, World, Schedule, Tick, Error, etc. | No | `zlim-reg`, `zlim-ptr`, `zlim-os`, `zlim-utils`, `zlim-task`, serde, log |
| `zlim-derive-utils` | Proc-macro helpers: crate path resolution via Cargo.toml | No | syn, serde, toml |
| `zlim-dylib` | Dynamic linking helper crate for faster dev iteration | No | `zlim-internal` |

#### Embedded Derive Crates

- `zlim-core/derive` — `#[derive(Error)]` and `#[derive(Bundle)]` macros
- `zlim-reflect/derive` — `#[derive(TypePath)]` and `#[derive(Reflect)]` macros

Both use `zlim-derive-utils::crate_path` to resolve paths, enabling correct
`::zlim::*` references from external crates.

### Key Design Patterns

1. **Feature Forwarding**: `zlim` accepts feature flags (`dylib`) and forwards
   them to `zlim-internal`, which then enables the corresponding subsystem
   behavior.
2. **Type-Erased Pointers** (zlim-ptr): Modeled after Bevy's `Ptr`, `PtrMut`,
   `OwningPtr` to avoid generic bloat and vtable overhead.
3. **CTOR Registration** (zlim-reg): Registers metadata before `main()` via
   platform-specific linker sections (`.init_array` / `.CRT$XCU` /
   `__DATA,__mod_init_func`).
4. **Platform Abstraction** (zlim-os): Uses `zlim-cfg::switch!` and
   `define_alias!` macros for platform conditional compilation, replacing raw
   `#[cfg]` attributes.
5. **Fixed-Seed Hashing**: HashMap/HashSet default to `FixedState` (fixed seed)
   for deterministic iteration order (DoS protection is unnecessary in game
   engine contexts).
6. **Niche-Optimized IDs**: The `define_ident!` macro uses `NonMaxU32` to
   enable niche optimization, making `Option<Id>` zero-overhead.
7. **Proc-Macro Path Resolution** (zlim-derive-utils): Derive macros read the
   consumer's `Cargo.toml` to determine whether `zlim` is a direct dependency,
   generating either `::zlim::module` or `::zlim_module` paths accordingly.

### zlim-core Module Overview

| Module | Status | Description |
|--------|--------|-------------|
| `entity` | ✅ Implemented | EntityId (index+version dual-word), lock-free allocator (Arc-based), EntityMap/EntityMapper |
| `component` | ✅ Implemented | Component trait (6 lifecycle hooks), ComponentDB global registry, `register_component!` macro, ComponentId (niche-optimized) |
| `tick` | ✅ Implemented | 32-bit Tick change detection (wrap-around safe), TicksRef/TicksMut/TicksSlice types, DetectChanges trait |
| `error` | ✅ Implemented | ZlimError (heap-allocated + Severity), `#[derive(Error)]` proc-macro |
| `script` | ✅ Implemented | Script trait, ScriptFlags (noop/exclusive/independent/main_thread) |
| `world` | ✅ Implemented | World struct, WorldCell (three-level safe access: read_only/data_mut/full_mut), DeferredWorld, WorldId |
| `table` | ✅ Implemented | Dense columnar storage Table (organized by archetype), Column (BlobArray + TickArray), TableId, Tables manager |
| `bundle` | ✅ Implemented | Bundle trait (collect/write/apply_effect), DataBundle, tuple impls (0..=12), `#[derive(Bundle)]` |
| `borrow` | ✅ Implemented | Ref/Mut/SliceRef/SliceMut + corresponding Untyped* variants, integrated change detection |
| `slot` | ✅ Implemented | Single-resource storage Slot (memory management + change detection ticks), ResourceSlots |
| `resource` | ✅ Implemented | Resource trait + ResourceDB global registry, Resources, `register_resource!` macro |
| `handle` | 🔸 Skeleton | Entity/EntityRef/EntityMut type definitions; most methods still pending |
| `schedule` | 🔸 Placeholder | — |
| `message` | 🔸 Placeholder | — |
| `scene` | 🔸 Placeholder | — |

## Development Environment

- **Rust version**: 1.96+ (edition 2024)
- **Resolver**: v3
- **Formatting**: Uses `rustfmt.toml` (style_edition 2024) — requires nightly:
  `cargo +nightly fmt`

## Current Status

The project is in early development. The ECS core (`zlim-core`) has implemented:
Entity allocation/mapping, Component registration/hook system, Tick change
detection, Error type system, Script base trait, World struct with safe access
layer, Table columnar storage, Bundle composition and writing, Slot resource
storage, Resource registry, and Borrow type-erased reference system.
Schedule/Scene modules are still placeholders.

All sub-crate infrastructure is in place:
- Compile macros (`zlim-cfg`) operational
- Pointer abstractions (`zlim-ptr`) operational
- Registry system (`zlim-reg`) operational
- Platform abstraction (`zlim-os`) covers desktop/WASM
- Utility library (`zlim-utils`) provides rich collections and sync primitives
- Task pool (`zlim-task`) supports multi/single/WASM modes
- Reflection system (`zlim-reflect`) provides type info, serialization, operation traits
- ECS core (`zlim-core`): Entity/Component/Tick/Error/World/Table/Bundle/Borrow/Slot/Resource implemented
