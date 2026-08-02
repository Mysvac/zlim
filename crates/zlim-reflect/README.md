# zlim-reflect

Runtime reflection system for the [Zlim](https://github.com/zlim-project/zlim) engine.

Provides type-erased introspection, manipulation, and serialization of Rust
values at runtime. Inspired by Bevy's reflection system but redesigned for
Zlim's ECS architecture where components are serialized via serde rather than
dynamic typing.

## Modules

| Module | Purpose |
|--------|---------|
| `ops` | Core `Reflect` trait and kind-specific accessor traits  |
| `info` | Compile-time type metadata: `TypeInfo`, `Type`, `Generics`, `Attributes`, field info |
| `path` | Stable, compile-time type-path identifiers (`TypePath`) |
| `db` | Global type registry (`TypeDB`) with conversion, construction, and serde hooks |
| `dynamic` | Runtime-constructible, type-erased containers (`DynamicStruct`, `DynamicList`, etc.) |
| `impls` | [`Reflect`] implementations for standard-library and primitive types |
| `derive` | Proc-macro derives: `#[derive(Reflect)]`, `#[derive(TypePath)]` |


## Quick Start

Derive `Reflect` and `TypePath` for your types:

```rust
use zlim_reflect::Reflect;
use zlim_reflect::info::Typed;

#[derive(Reflect)]
struct Position {
    x: f32,
    y: f32,
}

// Access compile-time type metadata.
let info = Position::type_info();
println!("Type: {}", info.type_path()); // "Position"

// Use through trait objects.
let pos = Position { x: 1.0, y: 2.0 };
let r: &dyn Reflect = &pos;
println!("{:?}", r); // Struct<Position>({x: 1.0, y: 2.0})
```

Convert between types at runtime with `from_reflect`:

```rust, ignore
use zlim_reflect::Reflect;
use zlim_reflect::dynamic::DynamicStruct;

#[derive(Reflect, Clone, Debug, Default)]
#[reflect(Clone, Debug, Default)]
struct Point { x: i32, y: f32 }

let mut dyn_struct = DynamicStruct::new();
dyn_struct.push("x".into(), Box::new(42i32));
dyn_struct.push("y".into(), Box::new(3.14f32));

let pt: Box<Point> = Point::from_reflect(Box::new(dyn_struct)).unwrap();
assert_eq!(pt.x, 42);
```

## Design

### Type Kinds

Every reflected type belongs to one of eight *kinds*, each with a
corresponding ops trait for data access:

| Kind | Trait | Examples |
|------|-------|----------|
| Opaque | `Opaque` | `i32`, `f64`, `String`, `bool` |
| Struct | `Struct` | `struct Pos { x: f32, y: f32 }` |
| Tuple | `Tuple` | `(i32, f32)`, tuple-structs |
| Array | `Array` | `[i32; 5]` |
| List | `List` | `Vec<T>`, `VecDeque<T>` |
| Map | `Map` | `HashMap<K, V>` |
| Set | `Set` | `HashSet<T>` |
| Enum | `Enum` | `enum Option<T> { None, Some(T) }` |

### Hash and Equality

All reflected types implement `Hash` and `Eq` through `Reflect::reflect_hash`
and `Reflect::reflect_eq`. The default implementation for `Opaque` types
uses text-based comparison (via `Opaque::stringify`), which ensures correct
behavior for `f32` / `f64` where IEEE 754 `NaN != NaN` would otherwise break
hash-table lookups.

Equality is **strict**: different concrete types always compare as unequal.

### `from_reflect` Conversion

`Reflect::from_reflect` converts a boxed reflected value into a concrete type
through a multi-phase workflow:

1. **Same type** — direct downcast (fast path).
2. **Type database** — lookup a registered conversion function.
3. **Field compatibility** — check single-layer field compatibility (non-recursive).
4. **Construct** — unpack source and build target field-by-field.

Conversion rules:

- **Tuples and arrays** are **strict**: element count must match exactly.
- **Structs and enum struct variants** are **lenient**: extra fields are ignored,
  but all non-`#[reflect(default)]` fields must be present.
- **`Opaque`** types may specialize `from_reflect`: all opaque values can
  convert between different concrete types via text serialization.

### Dynamic Types

Dynamic types (`DynamicStruct`, `DynamicTuple`, `DynamicEnum`, etc.) are
owned, type-erased containers that can hold any `Reflect` values. They serve
as data-transformation intermediaries — typically used as plumbing for
serialization / deserialization.

Key design choice: dynamic types implement the full `Reflect` trait directly
(not a "partial" variant), at the cost that their `TypeInfo` is always
`OpaqueInfo`. Use `Reflect::is_dynamic` to distinguish them from regular
concrete types.

### Type Database

The `TypeDB` is a per-type `'static` registry. Types opt in via the
`TypeDatabase` trait, and the `register!` macro submits registration
closures to a platform linker section. At startup, `TypeDB::collect`
invokes all pending registrations.

Registered types can provide:

- **Convertors** — runtime type-to-type conversion functions.
- **Constructors** — `FnOnce` factories for deserialization.
- **Serde hooks** — custom serialization/deserialization logic.
- **Pointer reconstructors** — rebuild `&dyn Reflect` / `&mut dyn Reflect`
  from raw type-erased pointers.

## Feature Flags

| Flag | Description |
|------|-------------|
| `debug` | Enables debug-mode diagnostics and assertions in derive macros. |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE)
or [MIT license](LICENSE-MIT) at your option.
