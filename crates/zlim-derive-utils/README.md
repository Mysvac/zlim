Helpers for proc-macro crates.

This crate provides:

`Cargo.toml` based crate path resolution.

## crate_path Rules

Resolution order:

1. If name starts with `zlim_` and `zlim` is in `[dependencies]`, return `::zlim::<module>`.
2. If the exact name is in `[dependencies]`, return `::<name>`.
3. If name starts with `zlim_` and `zlim` is in `[dev-dependencies]`, return `::zlim::<module>`.
4. Otherwise, return `::<name>` as a fallback.
