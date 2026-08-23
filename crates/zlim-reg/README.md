A distributed data collector based on static initialization.

## Declaring a Registry

When you need to collect data of a type, use the `collect!` macro:

```rust, ignore
struct Plugin { name: &'static str }

zlim_reg::collect!(Plugin);
```

`collect!` implements this crate's `Collect` trait for the type. Due to Rust's
orphan rule, it can only be invoked in the crate that defines the type.

In addition, the type must be concrete — it cannot contain unbound generic
parameters:

```rust, ignore
struct Foo<T>(T);

zlim_reg::collect!(Foo<u32>); // OK ✅ — Foo<u32> is a concrete type

zlim_reg::collect!(Foo<T>);   // Compile error ❌ — T is unbound
```

## Submitting Data

To submit data, use the `submit!` macro with the syntax `$expr => Type`:

```rust, ignore
zlim_reg::submit!(Plugin { name: "1" } => Plugin);
```

`submit!` is typically invoked at module scope (outside function bodies), as
long as the registry's type is accessible.

The expression passed in must be a constant expression, i.e. it must be
evaluable at compile time.

## Accessing Data

Create an iterator with the `iter` function to traverse all data submitted via
`submit!`:

```rust, ignore
for item in zlim_reg::iter::<Plugin>() {
    std::println!("{}", item.name);
}
```

The iterator yields elements of type `&'static T`, which are the static
variables defined inside the `submit!` macro.

If you need to collect data that can only be created at runtime, consider
collecting function pointers to "constructors" through this crate, then
iterating and calling them at runtime.

## Internal Implementation

C supports the `__attribute__((constructor))` function attribute in GCC and
Clang; marked functions are executed before `main`. This crate collects data
in a similar way.

1. `collect!` makes the type implement the `Collect` trait, providing a linked
   list (the registry) for that type.

2. `submit!` creates a static variable from the expression and declares a
   function that runs before `main` to register the static variable's
   reference into the linked list.

3. `iter` returns an iterator that walks the linked list of the corresponding
   type at runtime.

---

This crate's code is adapted from the inventory crate.

This crate has built-in handling for wasm targets, so users don't need to call
`__wasm_call_ctors` manually.
