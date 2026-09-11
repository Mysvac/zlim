# zlim-path

稳定、不受重构影响的**类型路径**标识符。

[`TypePath`] 是 [`core::any::type_name`] 的确定性替代方案：
返回**确定**的类型路径，允许显示指定，不随编译器版本或私有重构变化，
因此可以作为反射、序列化、编辑器以及运行时类型查找的稳定键。

```rust
use zlim_path::TypePath;

#[derive(TypePath)]
struct Foo;

assert_eq!(Foo::type_name(), "Foo");
```

## `TypePath` trait

| 关联项 | 类型 | 以 `Option<Vec<u8>>` 为例 |
|------|------|-------------------|
| `type_path()` | fn | `"core::option::Option<alloc::vec::Vec<u8>>"` |
| `type_name()` | fn | `"Option<Vec<u8>>"` |
| `IDENT` | const | `"Option"` |
| `CRATE` | const | `Some("core")` |
| `MODULE` | const | `Some("core::option")` |

- `type_path()` 是完整且唯一的标识符，会递归包含泛型参数，不允许与其他类型重复。

- `type_name()` 是简短、便于阅读的形式，可能重复（不同模块下的同名类型），用于诊断与展示。

- `IDENT` 是不含泛型参数的短类型名。必然包含泛型的路径则被映射为 `_` ，比如 `&_` 和 `(_,)` 。

- `IDENT`、`CRATE`、`MODULE` 是编译期常量，内建原生类型的 `CRATE`/`MODULE` 为 `None`。

- 所有返回的名称都不带前导 `::`。

## 使用方式

### derive 派生

```rust
use zlim_path::TypePath;

#[derive(TypePath)]
struct Foo;

// 由 `module_path!()` 与标识符生成：
//   type_path() → "{module}::Foo"
//   type_name() → "Foo"
//   IDENT       = "Foo"
//   MODULE      = Some("{module}")
//   CRATE       = Some({module} 的首段)
```

### 自定义路径

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

`#[type_path = "..."]` 会覆盖整条路径前缀，注意不能带前导 `::`。

### 泛型类型

类型与 const 泛型参数会自动纳入生成的路径，并按实例由 [`PathCell`] 缓存：

```rust
use zlim_path::TypePath;

#[derive(TypePath)]
struct MyVec<T>(Vec<T>);

// 当 `T = u8` 时：
//   type_path() → "{module}::MyVec<u8>"
//   type_name() → "MyVec<u8>"
//   IDENT       = "MyVec"
```

自定义路径形式同样支持泛型，最后一段将成为 `IDENT` 与 `type_name()` 的名称部分。

```rust
use zlim_path::TypePath;

#[derive(TypePath)]
#[type_path = "my_crate::vec::MyVec"] // 不需要指定泛型参数
struct MyVec<T>(Vec<T>);
```

### 手动实现

请参考 `impls` 模块中的实现示例。

## 覆盖范围

### primitive

- 基本类型：`i*/u*`、`f32/f64`、`bool`、`char`、`str`、`()`
- 引用：`&T`、`&mut T`
- 数组：`[T]`、`[T; N]`
- 元组：`(T,..)`

### core

- 原子变量：`Ordering` 与 `AtomicI*`、`AtomicU*`

- 枚举：`Option`、`Result`

- 时间：`Duration`

- 范围：`core::ops` 和 `core::range` 的公开结构体 

- 数值：`NonZero*`、`Wrapping` 和 `Saturating`

- 标记：`PhantomData` `PhantomPinned`

- 其他：`TypeId`、`&Location`、`BuildHasherDefault` 、`Cell` 、`RefCell`

### alloc

- 指针：`Box`、`Arc`、`Cow`
- 容器：`String`、`Vec`、`VecDeque`、`LinkedList`、`BTreeSet`、`BTreeMap`、`BinaryHeap`

### std

- 哈希：`RandomState`、`HashSet`、`HashMap`
- 文件系统：`Path`、`PathBuf`、`OsString`、`OsStr`

### zlim_utils

- 哈希：`FixedState`、`NoopState`、`SparseState`、`HashSet`、`HashMap`

- 其他：`NonMax*`、`SmolStr`、`SmallVec`、`ArrayVec`、`TypeMap`、`BlockList`

### `uuid`（feature）

`Uuid`、`NonNilUuid`

### `glam`（feature）

"float-types" 和 "integer-types" 两个 feature 对应的类型。

（即除了 `usize/isize` 系列之外的大部分类型。）

## Cargo Features

| Feature | 作用 | 默认 |
|------|--------|---------|
| `uuid` | 为 `uuid::Uuid` 与 `uuid::NonNilUuid` 提供 `TypePath` | 关闭 |
| `glam` | 为 `glam` 数学类型提供 `TypePath` | 关闭 |
