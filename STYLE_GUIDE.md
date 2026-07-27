# Zlim Coding Style Guide

Most formatting is handled by rustfmt — see [`rustfmt.toml`](./rustfmt.toml) for project-specific
tweaks. This guide covers conventions that rustfmt does *not* enforce, and that improve consistency
and readability across the codebase.

## Section Dividers

Comment dividers make it easier to scan large single-file modules. The project uses them heavily
to separate logical regions within a file.

Template:

```rust
// ----------------------------------------------------------------------------
// Description
```

Width is locked to **80 columns**: `//` + 1 space + 77 hyphens.

## Imports

Imports follow the rustfmt nightly `imports_granularity = "Module"` convention:
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

## TODO Comments

The `todo!()` macro marks unimplemented code.  It should generally not appear
in merged PRs, except for genuinely unsupported target platforms.

For deferred work that does **not** block compilation — missing documentation,
pending tests, planned optimizations, known limitations — use a comment with
uppercase `TODO!`. The format is flexible; the only requirement is clarity:

```rust
/* TODO! - Description */

// TODO! - Description

// TODO!
// - Item 1
// - Item 2
```

The uppercase `TODO!` convention makes every deferred item instantly
discoverable with an editor-wide text search.

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
