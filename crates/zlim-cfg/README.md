Compile-time helper macros that simplify compile-time control in your code.

## enabled! & disabled!

This crate defines and exports two very simple macros: `enabled!` and `disabled!`.

When invoked with no arguments, they expand to the boolean literals `true` and `false`:

```rust, ignore
assert!( zlim_cfg::enabled!() );
assert!( !zlim_cfg::disabled!() );
```

When invoked with statements, `enabled!` keeps the content as-is, while `disabled!` removes it entirely:

```rust, ignore
let mut x: u32 = 0;

zlim_cfg::enabled! {
    x |= 0b01;   // kept as-is
}

zlim_cfg::disabled! {
    x |= 0b10;  // removed by the macro
}

assert_eq!(x, 0b01);
```

You can also use the `if {} else {}` form to conditionally keep one side of the content:

```rust, ignore
let mut x: u32 = 0;

zlim_cfg::enabled! {
    if {
        x |= 0b0001;   // kept as-is
    } else {
        x |= 0b0010;   // not applied
    }
}

zlim_cfg::disabled! {
    if {
        x |= 0b0100;   // not applied
    } else {
        x |= 0b1000;   // kept as-is
    }
}

assert_eq!(x, 0b1001);
```

## define_alias!

On their own, the two macros above are of limited use. With `define_alias!`, however, you can give any compile-time option a name:

```rust, ignore
zlim_cfg::define_alias!{
    #[cfg(test)] => test,
    #[cfg(feature = "serde")] => serde,
}
```

The snippet above defines the `test` and `serde` aliases, which you can then use just like `enabled!` and `disabled!`:

```rust, ignore
test! {
    if {
        // This content only applies under #[cfg(test)]
    } else {
        // This content only applies under #[cfg(not(test))]
    }
}
```

When the `cfg` condition holds, the alias behaves exactly like `enabled!`; otherwise it behaves like `disabled!`.

## switch!

The last macro in this crate is `switch!`, which lets you activate specific code in the style of a `match` statement:

```rust, ignore
zlim_cfg::switch! {
    #[cfg(feature = "std")] => { /* content */ },
    enabled => { /* content */ },
    _ => { /* content */ },
}
```

It checks the branches from top to bottom and activates only the first one that matches, similar to the `cfg_select!` macro introduced in Rust 1.95.

A branch condition can be a `#[cfg(..)]` attribute, this crate's `enabled` and `disabled`, an alias defined by `define_alias!`, or a trailing `_` that serves as the default branch.

## Known Issues

When using this crate's macros to determine whether a module should be compiled,
skipped modules may not be selected by `rustfmt`.

Therefore, `cfg_select!` is still recommended for controlling whether a module is compiled.

This crate is primarily intended for controlling the compilation of internal code blocks,
which is more concise than `cfg_select`.

---

This crate's code is adapted from bevy_platform.
