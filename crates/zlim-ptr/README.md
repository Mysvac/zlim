Lightweight pointer wrappers for internal code.

This crate consists of two parts: type-erased references, and slices that store no length.

## Type-Erased References

- `Ptr<'a>`: type-erased shared pointer, conceptually similar to `&'a dyn Any`.

- `PtrMut<'a>`: type-erased exclusive pointer, conceptually similar to `&'a mut dyn Any`.

- `OwningPtr<'a>`: type-erased ownership pointer for read/drop handoff patterns, similar to `&'a mut MaybeUninit<_>`.

Unlike `dyn Any`, these three pointers erase the type completely — like `NonNull<()>` — and cannot be inspected through RTTI.

They serve three main purposes:

1. In ECS data storage implementations, function parameters prefer passing pointers over the full data; data is only transferred in deep functions (e.g. via `core::ptr::copy`), avoiding meaningless copies on the stack.

2. Type erasure avoids code bloat, letting the ECS low-level storage share a single implementation.

3. Adding a lifetime parameter on top of the pointer provides a degree of safety.

One extra note: `OwningPtr` is still just a plain pointer — it is **not** equivalent to `Box<T>`. That is, `OwningPtr` does not free the memory it points to, nor does it automatically call the data's `Drop`.

Semantically, `Ptr` is read-only, `PtrMut` is mutable, while `OwningPtr` permits "consuming" and "overwriting". `OwningPtr` usually points to a value wrapped in `ManuallyDrop`, so ownership of the value can be transferred out.

**Example:**

```rust, ignore
use zlim_ptr::Ptr;

let x = 10_i32;
let ptr = Ptr::from_ref(&x);

let rx = unsafe { ptr.deref::<i32>() };
assert_eq!(*rx, 10);
```

```rust, ignore
use zlim_ptr::OwningPtr;

let value = String::from("42");

zlim_ptr::into_owning!(value as ptr);
// ↑ equivalent to ↓
// let value = ManuallyDrop::new(value);
// let ptr = OwningPtr::from_value(&mut value);

let out = unsafe { ptr.read::<String>() };
// If `ptr.read()` is not called, the string's memory is never freed — a memory leak.

assert_eq!(out, "42");
```

## Slices Without Length

- `Slice<'a, T>`: thin shared slice pointer (stores only a pointer, no length).

- `SliceMut<'a, T>`: thin mutable slice pointer (stores only a pointer, no length).

These two types are used to reduce storage when the length is already known.

For example, the ECS implementation may have a type like this:

```rust, ignore
struct DataSlice<'a, T> {
    len: usize,
    data: Slice<'a, T>,
    added_time: Slice<'a, Tick>,
    changed_time: Slice<'a, Tick>,
}
```

In the example above, the three slices share a single length, so the struct saves the size of two `usize`.

**Example:**

```rust
use zlim_ptr::SliceMut;

let mut data = [1, 2, 3];
let mut thin = SliceMut::from_mut(&mut data);

unsafe {
    *thin.get_mut(1) = 20;
    assert_eq!(thin.as_ref(3), &[1, 20, 3]);
}
```

---

This crate's code is adapted from bevy_ptr.
