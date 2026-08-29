# zlim-ptr

为内部代码提供的轻量级指针包装。

本库包含两部分内容：类型擦除的引用，以及不含长度的切片。

## 类型擦除的引用

- `Ptr<'a>`：类型擦除的共享指针，概念上类似于 `&'a dyn Any`。

- `PtrMut<'a>`：类型擦除的独占指针，概念上类似于 `&'a mut dyn Any`。

- `OwningPtr<'a>`：类型擦除的所有权指针，用于读取/析构交接模式，类似 `&'a mut MaybeUninit<_>`。

与 `dyn Any` 不同，此处的三个指针完全擦除了类型，类似 `NonNull<()>`，无法通过 RTTI 进行检查。

其目的主要有以下三点：

1. 在 ECS 数据存储的实现中，函数参数尽量传递指针而非完整数据，仅在深层函数中，
    通过 `core::ptr::copy` 等方式转移数据，从而避免数据在栈上的无意义拷贝。

2. 通过类型擦除的方式，避免代码膨胀，让 ECS 底层存储共用一份代码。

3. 在指针的基础上添加生命周期参数，一定程度上保证安全性。

需要额外说明的是，`OwningPtr` 依然只是简单的指针，并**不**等价于 `Box<T>`。
也就是说，`OwningPtr` 并不会负责释放指向的内存，也不会自动调用数据的 `Drop`。

语义上，`Ptr` 表示只读，`PtrMut` 表示可变，而 `OwningPtr` 则允许“消耗”和“覆盖”。
`OwningPtr` 通常指向一个被 `ManuallyDrop` 包裹的值，可以将值的所有权转移到外部。

**使用示例：**

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
// ↑ 等价于 ↓
// let value = ManuallyDrop::new(value);
// let ptr = OwningPtr::from_value(&mut value);

let out = unsafe { ptr.read::<String>() };
// 如果不调用 `ptr.read()`，字符串的内存不会被释放，即发生内存泄漏。

assert_eq!(out, "42");
```

## 不含长度的切片

- `Slice<'a, T>`：瘦共享切片指针（只存储指针，不存储长度）。

- `SliceMut<'a, T>`：瘦可变切片指针（只存储指针，不存储长度）。

这两个类型用于在“长度已知”的情况下，削减存储所需的空间。

例如，ECS 的实现中可能有这样一个类型：

```rust, ignore
struct DataSlice<'a, T> {
    len: usize,
    data: ThinSlice<'a, T>,
    added_time: ThinSlice<'a, Tick>,
    changed_time: ThinSlice<'a, Tick>,
}
```

在上面的例子中，三个切片共用一个长度，因此结构体削减了两个 `usize` 的大小。

**使用示例：**

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

本库代码改自 bevy_ptr。
