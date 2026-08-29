# zlim-reflect

A runtime reflection system designed for the Zlim Engine, which can be divided into four parts:

## Modules

| Module | Purpose |
|--------|---------|
| `path` | Compile-time type paths, providing stable unique type identifiers |
| `info` | Compile-time type metadata, e.g. field lists, custom attributes, generic parameters |
| `ops` | Core reflection traits, plus kind-specific data-operation subtraits |
| `db` | Global type registry |
| `dynamic` | Dynamically constructed, type-erased containers, typically used for reflection-based serialization |
| `impls` | Helper functions for implementing reflection, plus reflection impls for common types |
| `derive` | Reflection-related macros |

## Type Paths

Type paths are defined by the `TypePath` trait, which provides a stable global type path used as the unique identifier of a type.

> The official docs note that `core::any::type_name` is not guaranteed to be stable, and is only recommended for debug use.

By default, `TypePath` builds the path from `module_path!()` plus the type identifier.

You can also use the `type_path = "..."` attribute to specify the path explicitly, so the path stays stable even if the code is moved.

```rust
use zlim_reflect::TypePath;

#[derive(TypePath)]
#[type_path = "example::Foo"]
struct Foo;

assert_eq!(Foo::type_path(), "example::Foo");
assert_eq!(Foo::type_name(), "Foo");
```

## Type Info

The `Reflect` macro generates all the code needed for reflection, which includes the `TypePath` part:

```rust
use zlim_reflect::Reflect;
use zlim_reflect::info::Typed;

#[derive(Reflect)]
struct Position {
    x: f32,
    y: f32,
}

// Access runtime type info
let info = Position::type_info();
println!("Type: {}", info.type_path()); // "Position"

// Type erasure.
let pos = Position { x: 1.0, y: 2.0 };
let r: &dyn Reflect = &pos;
println!("{:?}", r); // Similar: Struct<Position>({x: 1.0, y: 2.0})
```

## Common Operations

The `Reflect` trait defines a set of common reflection operations, roughly as follows:

- `reflect_assign`: strong-typed assignment; succeeds when the types are equal, fails otherwise.

- `reflect_apply`: weak-typed copy assignment; assignment works as long as the structures are similar.

- `reflect_clone`: copying data in the type-erased state, guaranteeing the returned value has the same type as the source.
  This usually always succeeds; only a few types that don't support `Clone` may fail.

- `reflect_eq`: comparison in the type-erased state, strong-typed (returns `false` directly when the types differ).
  For special types like strings, a lenient string-based comparison is used. For special types like `HashSet`,
  due to the non-deterministic iteration order, `true` is almost only returned when the sets are completely
  identical (iterating the same elements in the same order).

- `reflect_hash`: hashing in the type-erased state.

- `from_reflect`: tries to construct itself from a reflected value. Not fully weak-typed.
  If the types are equal, it always succeeds. If the type database provides a conversion function, it also always succeeds.
  Otherwise, for basic data types, it tries to convert to a string and then deserialize itself from the string.
  For composite types, it tries to construct itself field by field, but each field must be directly compatible
  (equal type or a provided conversion function); it does not recurse further.

`reflect_apply` is usually more lenient than `from_reflect`: the former is fully weak-typed and can assign as long
as the structures are similar; the latter only allows the type itself to differ, while fields and other subtypes
must be directly compatible (equal type or a provided conversion function).

Conversely, `from_reflect` is usually more efficient than `reflect_apply`, because it converts types directly
without copying values.

```rust
use zlim_reflect::Reflect;
use zlim_reflect::dynamic::DynamicStruct;

#[derive(Reflect, Clone, Debug, Default)]
struct Point { x: i32, y: f32 }

let mut dyn_struct = DynamicStruct::new();
dyn_struct.push("x".into(), Box::new(114i32));
dyn_struct.push("y".into(), Box::new(3.14f32));

let mut pt = Point::default();
pt.reflect_apply(&dyn_struct).unwrap();
assert_eq!(pt.x, 114);

let pt: Box<Point> = Point::from_reflect(Box::new(dyn_struct)).unwrap();
assert_eq!(pt.x, 114);
```

## Reflection Kinds

This crate defines eight common reflection subtypes:

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

Note that `struct T` is Opaque, `struct T{}` is Struct, and `struct T()` is Tuple.

A reflected value can be converted to a subtype for more dynamic operations, such as field access.

```rust
use zlim_reflect::ops::{Reflect, Struct};

#[derive(Reflect, Debug, Default)]
struct Point { x: i32, y: f32 }

let mut pt: Point = Point::default();

let dyn_struct: &mut dyn Struct = pt.reflect_mut().as_struct().unwrap();
let x: &mut dyn Reflect = dyn_struct.field_mut("x").unwrap();
*x.downcast_mut::<i32>().unwrap() = 5;

assert_eq!(pt.x, 5);
```

## Type Database

`TypeDB` is the "type database" for all reflected types: it stores each type's type info, along with optional
constructors, conversion functions, and serialization/deserialization function pointers.

Composite types automatically register their subtypes when registered — for example, registering
`struct A(Vec<i32>)` also registers `Vec<i32>`.

All non-generic types that implement reflection (via the `Reflect` macro) are registered automatically through
the `zlim_reg` crate. Primitive types like integers, floats, and strings are pre-registered by this crate, so
callers can use them without worry.

Generic types are not registered automatically, because `zlim_reg` can only collect concrete types. You can
register them explicitly with the `register_reflect!` macro; duplicate registration is safe:

```rust
use zlim_reflect::{Reflect, register_reflect};

#[derive(Reflect)]
struct Foo<T>(T);

register_reflect!(Foo<u32>, Foo<i32>);
```

## Reflection-Based Serialization

The reflection system provides serialization and deserialization support based on serde, with the entry points
on `TypeDB`, in two formats:

- `reflect_serialize` / `reflect_deserialize`: **self-describing** format.
  The serialized data is wrapped in a map keyed by the type path, e.g. `{"my_crate::A":{"x":3,"y":4}}` —
  the outer `my_crate::A` is the type path. Deserialization does not need to know the target type in advance;
  the input carries its own type information.

- `serialize` / `deserialize`: **raw data** format, containing only the payload,
  e.g. `{"x":3,"y":4}`, consistent with standard serde output. For deserialization the caller must hold the
  `TypeDB` of the target type themselves; nested fields are serialized with this format internally.

Processing priority:

1. **Registered functions first**: if a type has `SerdFunc`/`DeseFunc` registered in its `TypeDB`
   (set via `insert_serializer`/`insert_deserializer`), they are called directly — this is the fast path
   for types with serde implementations.

2. **Reflection fallback**: otherwise, dispatch to the kind-specific serialization function or
   deserialization visitor (Opaque, Struct, Tuple, Array, List, Map, Set, Enum).

In addition, even if an Opaque type has no serialization function registered, it is stringified via
`Opaque::stringify` and then serialized as a string. Therefore **any reflected type can be serialized**,
just with different efficiency.

Deserialization uses a two-phase strategy: if the type has a default constructor, construct an empty value
and modify its fields in place (fast); otherwise, build a `Dynamic*` value and convert it to the target type
via `TypeDB::from_reflect` (always works, but slower).

## Code Generation

Reflection implementations mostly don't need to be written by hand — the `derive` macros handle it.
The macros live in `zlim-reflect/derive`, mainly two of them:

- `#[derive(TypePath)]`: generates the `TypePath` implementation.
  By default it builds the path from `module_path!()` plus the type name; you can also specify it yourself
  with `#[type_path = "..."]`. Generic parameters are automatically appended to the path (cached via `PathCell`).

- `#[derive(Reflect)]`: generates all the code needed for reflection at once:
  - The core `Reflect` trait (`reflect_clone`, `reflect_apply`, `reflect_eq`, `reflect_hash`,
    `reflect_debug`, `from_reflect`)
  - `TypePath` and `Typed` (for runtime type info)
  - The kind-specific subtrait (structs get `Struct`, enums get `Enum`, and so on)
  - `TypeDatabase` (type registration, conversion, and auto-discovery)

For non-generic types, the macro additionally emits a `register_reflect!` call, so the type is
auto-discovered and registered by `TypeDB::collect` at startup.

Attributes supported by `#[derive(Reflect)]`:

| Attribute | Effect |
|-----------|--------|
| `#[type_path = "..."]` | Custom type path, overriding the default |
| `#[reflect(Opaque)]` | Mark the type as Opaque (the `Opaque` trait must be implemented manually) |
| `#[reflect(Clone)]` | `reflect_clone` reuses `Clone::clone` directly |
| `#[reflect(Eq)]` | `reflect_eq` reuses `PartialEq::eq` directly |
| `#[reflect(Hash)]` | `reflect_hash` reuses `Hash::hash` directly |
| `#[reflect(Debug)]` | `reflect_debug` reuses `Debug::fmt` directly |
| `#[reflect(Default)]` | Registers a default constructor; the type can be constructed at runtime via `TypeDB::default` |
| `#[reflect(@expr)]` | Attaches custom metadata (stored in `Attributes`, readable at runtime) |

For the full details, see the documentation of `zlim_reflect::derive`.

## Cargo Features

| Feature | Description |
|---------|-------------|
| `debug` | Records the serialization/deserialization stack for clearer failure messages |
| `glam` | Serialization support for `glam` crate data structures |
| `uuid` | Serialization support for `Uuid` and `NonNilUuid` from the `uuid` crate |

---
