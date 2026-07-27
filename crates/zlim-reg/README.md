# CTOR-Based Metadata Collector

Link-time registry for static metadata — values are registered through C-level constructors and collected into type-safe linked lists at runtime.

## Modules

- **`collect!`** — Declares a registry for a type.
- **`submit!`** — Registers a static value; auto-invoked via linker constructors before `main()`.
- **`iter`** — Iterates over all submitted values of a given type.

## Platforms

| Platform | Constructor mechanism |
|----------|-----------------------|
| Linux, Android, WASM, BSDs | `.init_array` |
| Windows | `.CRT$XCU` |
| macOS, iOS, tvOS | `__DATA,__mod_init_func` |

WASM does **not** require a manual `__wasm_call_ctors` call — the crate invokes it automatically on first `iter`.

## Safety

Each registry is bound to exactly one concrete type, enforced by a `TypeId` check. Sharing a `Registry` across multiple types is undefined behavior. The `collect!` macro generates the correct implementation automatically, so users never need to implement `Collect` by hand.

## Usage

```rust
struct Plugin { name: &'static str }

zlim_reg::collect!(Plugin);

zlim_reg::submit!(Plugin { name: "physics" } => Plugin);
zlim_reg::submit!(Plugin { name: "render" }  => Plugin);

for plugin in zlim_reg::iter::<Plugin>() {
    println!("loaded: {}", plugin.name);
}
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
