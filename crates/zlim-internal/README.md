# zlim-internal

Core implementation crate of the zlim engine.

Acts as the central feature-management hub — the public `zlim` facade forwards its features down to this crate,
which then enables the matching behavior across all engine subsystems.

## Internal Crates

- **`cfg`** — Re-exports `zlim_cfg`: compile-time control macros (`enabled!`, `disabled!`, `switch!`, `define_alias!`).
- **`ptr`** — Re-exports `zlim_ptr`: type-erased pointer abstractions (`Ptr`, `PtrMut`, `OwningPtr`, `Slice`, `SliceMut`).
- **`reg`** — Re-exports `zlim_reg`: CTOR-based metadata collector (`collect!`, `submit!`, `iter`).
- **`log`** — Re-exports `zlim_log`: tracing-based logging and performance analysis tools.
- **`os`** — Re-exports `zlim_os`: platform abstraction (standard directories, time, system crate re-exports).
- **`utils`** — Re-exports `zlim_utils`: foundation utilities (hash containers, sync primitives, memory pools, collections).
- **`reflect`** — Re-exports `zlim_reflect`: runtime reflection (`Reflect`, `TypePath`, `TypeDB`, dynamic types).
- **`task`** — Re-exports `zlim_task`: async task pool with work-stealing (`TaskPool`, `Scope`, `block_on`, `Task`).
- **`core`** — Re-exports `zlim_core`: ECS core (Entity, Component, World, Schedule, Tick, etc.).
- **`app`** — Re-exports `zlim_app`: app / plugin framework (`App`, `Plugin`, main schedules).
- **`math`** — Re-exports `zlim_math`: math library built on `glam` (vectors, matrices, ops).
- **`shape`** — Re-exports `zlim_shape`: primitive shapes (2D/3D), rays, bounding volumes.
- **`curve`** — Re-exports `zlim_curve`: the `Curve` trait, curve types and adaptors.
- **`color`** — Re-exports `zlim_color`: color spaces, conversions and palettes.
- **`transform`** — Re-exports `zlim_transform`: `Transform`/`GlobalTransform` + hierarchy propagation.
- **`diagnostic`** — Re-exports `zlim_diagnostic`: diagnostics store and built-in measurement plugins.
- **`sysinfo`** — Re-exports `zlim_sysinfo`: host system info plugins (enabled with the `zlim_sysinfo` feature).
- **`derive`** — Re-exports all derive macros.
