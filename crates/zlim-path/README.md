# zlim-path

Stable, refactor-proof **type path** identifiers.

[`TypePath`] is a deterministic alternative to [`core::any::type_name`]:
it returns a **deterministic** type path that can be specified explicitly and
does not change across compiler versions or private refactors, so it can be
used as a stable key for reflection, serialization, editors, and runtime type
lookup.

```rust
use zlim_path::TypePath;

#[derive(TypePath)]
struct Foo;

assert_eq!(Foo::type_name(), "Foo");
```

## The `TypePath` trait

| Item | Kind | For `Option<Vec<u8>>` |
|------|------|-------------------|
| `type_path()` | fn | `"core::option::Option<alloc::vec::Vec<u8>>"` |
| `type_name()` | fn | `"Option<Vec<u8>>"` |
| `IDENT` | const | `"Option"` |
| `CRATE` | const | `Some("core")` |
| `MODULE` | const | `Some("core::option")` |

- `type_path()` is the full, unique identifier. It includes generic parameters
  (recursively) and must not collide with any other type.

- `type_name()` is the short, human-readable form. It may collide (types in
  different modules can share a name) and is meant for diagnostics and display.

- `IDENT` is the short type name without generic parameters. Paths that
  necessarily carry generics map them to `_`, e.g. `&_` and `(_,)`.

- `IDENT`, `CRATE` and `MODULE` are compile-time constants; `CRATE`/`MODULE` are
  `None` for built-in primitives.

- No returned name ever carries a leading `::`.

## Usage

### Derive

```rust
use zlim_path::TypePath;

#[derive(TypePath)]
struct Foo;

// Generated from `module_path!()` and the identifier:
//   type_path() → "{module}::Foo"
//   type_name() → "Foo"
//   IDENT       = "Foo"
//   MODULE      = Some("{module}")
//   CRATE       = Some(first segment of {module})
```

### Custom path

```rust
use zlim_path::TypePath;

#[derive(TypePath)]
#[type_path = "my_crate::bar::Baz"]
struct Foo;

assert_eq!(Foo::type_path(), "my_crate::bar::Baz");
assert_eq!(Foo::type_name(), "Baz");
assert_eq!(Foo::IDENT, "Baz");
assert_eq!(Foo::CRATE, Some("my_crate"));
assert_eq!(Foo::MODULE, Some("my_crate::bar"));
```

`#[type_path = "..."]` overrides the whole path prefix; the leading `::` must be
omitted.

### Generic types

Type and const generic parameters are included in the generated paths
automatically, cached per instantiation by [`PathCell`]:

```rust
use zlim_path::TypePath;

#[derive(TypePath)]
struct MyVec<T>(Vec<T>);

// For `T = u8`:
//   type_path() → "{module}::MyVec<u8>"
//   type_name() → "MyVec<u8>"
//   IDENT       = "MyVec"
```

The custom-path form also accepts generics; the trailing segment becomes the
`IDENT`/`type_name()` root.

```rust
use zlim_path::TypePath;

#[derive(TypePath)]
#[type_path = "my_crate::vec::MyVec"] // no generic parameters needed
struct MyVec<T>(Vec<T>);
```

### Manual implementation

See the implementations in the `impls` module for examples.

## Coverage

### `primitive`

- Primitives: `i*`/`u*`, `f32`/`f64`, `bool`, `char`, `str`, `()`
- References: `&T`, `&mut T`
- Arrays: `[T]`, `[T; N]`
- Tuples: `(T, ...)`

### `core`

- Atomics: `Ordering` and `AtomicI*`, `AtomicU*`

- Enums: `Option`, `Result`

- Time: `Duration`

- Ranges: the public structs of `core::ops` and `core::range`

- Numbers: `NonZero*`, `Wrapping`, `Saturating`

- Markers: `PhantomData`, `PhantomPinned`

- Others: `TypeId`, `&Location`, `BuildHasherDefault`, `Cell`, `RefCell`

### `alloc`

- Pointers: `Box`, `Arc`, `Cow`
- Containers: `String`, `Vec`, `VecDeque`, `LinkedList`, `BTreeSet`, `BTreeMap`, `BinaryHeap`

### `std`

- Hashing: `RandomState`, `HashSet`, `HashMap`
- Filesystem: `Path`, `PathBuf`, `OsString`, `OsStr`

### `zlim_utils`

- Hashing: `FixedState`, `NoopState`, `SparseState`, `HashSet`, `HashMap`

- Others: `NonMax*`, `SmolStr`, `SmallVec`, `ArrayVec`, `TypeMap`, `BlockList`

### `uuid` (feature)

`Uuid`, `NonNilUuid`

### `glam` (feature)

The types covered by glam's `float-types` and `integer-types` features.

(That is, most types except the `usize`/`isize` families.)

## Cargo Features

| Flag | Effect | Default |
|------|--------|---------|
| `uuid` | `TypePath` for `uuid::Uuid` and `uuid::NonNilUuid` | off |
| `glam` | `TypePath` for `glam` math types | off |
