# Dynamic Linking Helper

Build-optimization shim that packages the engine into a single dynamic library for faster iterative development.

Compiled as `crate-type = ["dylib"]`, it force-links `zlim-internal` so the
bulk of the engine lives in one shared object (`.dll` / `.so` / `.dylib`).
This avoids re-linking engine code into every dependent binary on each rebuild.

## Usage

```sh
cargo build --features dylib
```

Enabled only when the workspace `dylib` feature is active. Not used in release or WASM builds.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

