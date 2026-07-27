# Pointer extension

Lightweight pointer wrappers for internal runtime code.

This crate provides small, type-erased pointer utilities used by ECS/reflection internals to reduce data movement and keep APIs explicit about safety boundaries.

## Modules

- `Ptr<'a>`: type-erased shared pointer, conceptually similar to `&'a T`.
- `PtrMut<'a>`: type-erased exclusive pointer, conceptually similar to `&'a mut T`.
- `OwningPtr<'a>`: type-erased ownership pointer for read/drop handoff patterns.
- `Slice<'a, T>`: thin shared slice pointer (stores pointer only, no length).
- `SliceMut<'a, T>`: thin mutable slice pointer (stores pointer only, no length).

## Safety

These types are intentionally low-level. The compiler enforces lifetime shape through `PhantomData`, but callers still own key runtime responsibilities:

1. Use the correct target type when casting from erased pointers.
2. Ensure pointer alignment for the target type.
3. Ensure pointee validity and initialization state.
4. Respect aliasing/exclusivity rules for mutable access.

In debug builds, prefer calling alignment checks before unsafe casts.

## Usage

Shared erased pointer:

```rust
use zlim_ptr::Ptr;

let x = 10_i32;
let ptr = Ptr::from_ref(&x);

let rx = unsafe { ptr.deref::<i32>() };
assert_eq!(*rx, 10);
```

Owning handoff:

```rust
use core::mem::ManuallyDrop;
use zlim_ptr::OwningPtr;

let mut value = ManuallyDrop::new(42_i32);
let ptr = OwningPtr::from_value(&mut value);

let out = unsafe { ptr.read::<i32>() };
assert_eq!(out, 42);
```

Thin mutable slice:

```rust
use zlim_ptr::SliceMut;

let mut data = [1, 2, 3];
let mut thin = SliceMut::from_mut(&mut data);

unsafe {
    *thin.get_mut(1) = 20;
    assert_eq!(thin.as_ref(3), &[1, 20, 3]);
}
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
