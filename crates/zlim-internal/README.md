# Engine Internals

Core implementation crate of the zlim engine.

Acts as the central feature-management hub — the public `zlim` facade forwards its features down to this crate,
which then enables the matching behavior across all engine subsystems.

## Internal Crates

- **`ptr`** — Re-exports `zlim_ptr`: type-erased pointer abstractions (`Ptr`, `PtrMut`, `OwningPtr`, `Slice`, `SliceMut`).
- **`reg`** — Re-exports `zlim_reg`: CTOR-based metadata collector (`collect!`, `submit!`, `iter`).
- **`os`** — Re-exports `zlim_os`: platform abstraction (standard directories, time, system crate re-exports).
- **`utils`** — Re-exports `zlim_utils`: foundation utilities (hash containers, sync primitives, memory pools, collections).
- **`task`** — Re-exports `zlim_task`: async task pool with work-stealing (`TaskPool`, `Scope`, `block_on`, `Task`).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
