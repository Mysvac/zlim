# Zlim Coding Style Guide

Most formatting is handled by rustfmt — see [`rustfmt.toml`](./rustfmt.toml) for project-specific
tweaks. This guide covers conventions that rustfmt does *not* enforce, and that improve consistency
and readability across the codebase.

## Section Dividers

Comment dividers make it easier to scan large single-file modules. The project uses them heavily
to separate logical regions within a file.

Template:

```rust
// -----------------------------------------------------------------------------
// Description
```

Or

```rust
// -----------------------------------------------------------------------------
// Description
// -----------------------------------------------------------------------------
```

Width is locked to **80 columns**: `//` + 1 space + 77 hyphens.

## Imports

Imports follow the rustfmt `imports_granularity = "Module"` convention:
**only the final path segment** may use `{ ... }` braces.

```rust
// rustfmt default — Should Avoid
crate_1::{
    hello,
    module_1::{A, B, c, d},
    module_2::C,
};

// Zlim style
crate_1::hello;
crate_1::module_1::{A, B, c, d};
crate_1::module_2::C;
```

When a single brace-group is too long, split it across multiple imports
(rustfmt will not merge them back):

```rust
// Too crowded
crate_1::module_1::{
    AAA, BBBB, CCCC, DDDDD, EEEEEEE, FFFFFFF,
    GGGGGGGGG, HHHHHHHHHHHHHHHHH,
};

// Better — split over multiple lines
crate_1::module_1::{AAA, BBBB, CCCC, DDDDD, EEEEEEE};
crate_1::module_1::{FFFFFFF, GGGGGGGGG, HHHHHHHHHHHHHHHHH};
```

## Unsafe Code

Every `unsafe fn` must document its preconditions in a `# Safety` section:

```rust
/// # Safety
///
/// Callers must ensure that …
unsafe fn add() { /* ... */ }
```

For `unsafe {}` blocks, a `// SAFETY:` comment is required only when the safety
reasoning is non-obvious.  Keep it short — state which precondition is satisfied:

```rust
// SAFETY: the caller guarantees the index is within bounds.
unsafe { /* ... */ }
```

## Doc References

Rust’s doc comments support automatic linking for types and functions. For example,
[`None`] automatically links to `core::option::Option`.

However, when a path is not in scope or is ambiguous, an explicit link is required:

```rust
/// Uses [`Foo`](foo::Foo) to do something important.
pub fn do_something() { /* ... */ }
```

If the full path is long and clutters the doc comment, define the link reference at
the bottom of the comment block:

```rust
/// Uses [`Foo`] to do something important.
///
/// [`Foo`]: crate::very::deep::path::to::Foo
pub fn do_something() { /* ... */ }
```

This keeps the main doc text readable, while still providing a correct, unambiguous
link for the generated documentation.

---
