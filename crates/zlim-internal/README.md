# zlim-internal

Core implementation crate of the zlim engine.

Acts as the central feature-management hub — the public `zlim` facade forwards its features down to this crate,
which then enables the matching behavior across all engine subsystems.

## Internal Crates

- **`cfg`** — Re-exports `zlim_cfg`: compile-time control macros (`enabled!`, `disabled!`, `switch!`, `define_alias!`).
- **`ptr`** — Re-exports `zlim_ptr`: type-erased pointer abstractions (`Ptr`, `PtrMut`, `OwningPtr`, `Slice`, `SliceMut`).
- **`reg`** — Re-exports `zlim_reg`: CTOR-based metadata collector (`collect!`, `submit!`, `iter`).
- **`os`** — Re-exports `zlim_os`: platform abstraction (standard directories, time, system crate re-exports).
- **`log`** —— 重新导出 `zlim_log`：log library and performance analysis tools.
- **`utils`** — Re-exports `zlim_utils`: foundation utilities (hash containers, sync primitives, memory pools, collections).
- **`reflect`** — Re-exports `zlim_reflect`: runtime reflection (`Reflect`, `TypePath`, `TypeDB`, dynamic types).
- **`task`** — Re-exports `zlim_task`: async task pool with work-stealing (`TaskPool`, `Scope`, `block_on`, `Task`).
- **`core`** — Re-exports `zlim_core`: ECS core (Entity, Component, World, Schedule, Tick, etc.).
- **`derive`** — Re-exports all derive macros.
