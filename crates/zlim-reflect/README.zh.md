专为 Zlim Engine 设计的运行时反射系统，整体可以分成四个部分：

1. 运行时类型信息

2. 类型擦除时的数据操作

3. 类型信息注册表

4. 代码生成

## 模块列表

| 模块 | 内容 |
|--------|---------|
| `path` | 编译期确定的类型路径，提供稳定的类型唯一标识 |
| `info` | 编译期类型数据，例如字段列表、自定义属性、泛型参数等 |
| `ops` | 核心的反射 Trait，以及类型特定的数据操作子 Trait  |
| `db` | 全局类型注册表 |
| `dynamic` | 动态构造的类型擦除容器，通常用于反射序列化。 |
| `impls` | 提供一些用于实现反射的通用函数，并为常见类型实现反射 |
| `derive` | 提供反射相关的宏 |

## 类型路径

类型路径由 `TypePath` Trait 定义，用于提供稳定的全局类型路径，作为类型的唯一标识。

> 官方文档指出，`core::any::type_name` 并不保证稳定，通常只推荐用于 debug。

`TypePath` 提供的路径默认由 `module_path!()` + `type_ident` 两部分组成。

你还可以通过 `type_path = "..."` 属性显式指定类型路径，这样即使相关代码被移动，
类型路径也能保持稳定。

```rust
use zlim_reflect::TypePath;

#[derive(TypePath)]
#[type_path = "example::Foo"]
struct Foo;

assert_eq!(Foo::type_path(), "example::Foo");
assert_eq!(Foo::type_name(), "Foo");
```

## 类型信息

通过 `Reflect` 宏为类型生成完整反射所需的代码，这包含 `TypePath` 的内容：

```rust
use zlim_reflect::Reflect;
use zlim_reflect::info::Typed;

#[derive(Reflect)]
struct Position {
    x: f32,
    y: f32,
}

// 访问运行时类型信息
let info = Position::type_info();
println!("Type: {}", info.type_path()); // "Position"

// 类型擦除。
let pos = Position { x: 1.0, y: 2.0 };
let r: &dyn Reflect = &pos;
println!("{:?}", r); // 类似：Struct<Position>({x: 1.0, y: 2.0})
```

## 通用操作

`Reflect` Trait 定义了一系列反射的通用操作，大致如下：

- `reflect_assign`：强类型的赋值，类型相等时成功，类型不相等时失败。

- `reflect_apply`：弱类型的拷贝赋值，结构相似即可进行赋值。

- `reflect_clone`：类型擦除状态下的数据拷贝，保证返回值的类型与本身一致。
  此函数通常始终成功，仅有少量不支持 `Clone` 的类型可能失败。

- `reflect_eq`：类型擦除状态下的比较，强类型比较（类型不等时直接返回 `false`）。
  对于字符串等特殊类型，会使用字符串化的宽松比较。对于 `HashSet` 等特殊类型，由于迭代顺序的
  不确定性，几乎只有完全相同时（迭代出的元素顺序也一致）才会返回 `true`。

- `reflect_hash`：类型擦除状态下的 Hash。

- `from_reflect`：尝试从反射值构造自身。非完全的弱类型。
  如果类型相等，必然成功。如果类型数据库提供了转换函数，也必然成功。
  否则，对于基本数据类型，尝试转换成字符串，然后从字符串反序列化自身。
  对于复合数据类型，尝试逐个字段构造自身，但字段需要直接兼容（类型相等或提供了转换函数），不会再递归构造。

`reflect_apply` 通常比 `from_reflect` 更加宽松：前者是完全的弱类型，结构相似就能赋值；
后者只允许类型本身不同，字段等子类型必须直接兼容（类型相等或提供了转换函数）。

反过来，`from_reflect` 通常比 `reflect_apply` 更高效，因为它直接转换类型，无需拷贝值。

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

## 反射种类

本库定义了八种常见的反射子类型：

| 类型 | Trait | 示例 |
|------|-------|----------|
| Opaque | `Opaque` | `i32`, `f64`, `String`, `bool` |
| Struct | `Struct` | `struct Pos { x: f32, y: f32 }` |
| Tuple | `Tuple` | `(i32, f32)`, tuple-structs |
| Array | `Array` | `[i32; 5]` |
| List | `List` | `Vec<T>`, `VecDeque<T>` |
| Map | `Map` | `HashMap<K, V>` |
| Set | `Set` | `HashSet<T>` |
| Enum | `Enum` | `enum Option<T> { None, Some(T) }` |

注意，`struct T` 属于 Opaque，`struct T{}` 属于 Struct，而 `struct T()` 属于 Tuple。

可以将反射对象转换为子类型，以实现更多的动态操作，比如字段访问。

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

## 类型数据库

`TypeDB` 结构体是所有反射类型的"类型数据库"：保存每种类型的类型信息，
并附带可选的构造函数、转换函数，以及序列化/反序列化函数指针。

复合类型在注册时会自动注册子类型，比如 `struct A(Vec<i32>)` 在注册时会自动注册 `Vec<i32>`。

所有实现了反射（通过 `Reflect` 宏）的非泛型类型，都会通过 `zlim_reg` 库自动注册类型数据。
而整数、浮点数、字符串等基本数据类型已由本库注册，调用者可以放心使用。

泛型类型不会自动注册，因为 `zlim_reg` 只能收集确定的类型。你可以使用本库提供的 `register_reflect!` 宏显式为类型注册，
重复注册是安全的，可以放心使用：

```rust
use zlim_reflect::{Reflect, register_reflect};

#[derive(Reflect)]
struct Foo<T>(T);

register_reflect!(Foo<u32>, Foo<i32>);
```

## 基于反射的序列化

反射系统基于 serde 提供序列化与反序列化支持，集成在 `TypeDB` 中，分为两种格式：

- `reflect_serialize` / `reflect_deserialize`：**自描述**格式。
  序列化出的数据会包一层以类型路径为键的映射，比如 `{"my_crate::A":{"x":3,"y":4}}`
  ——外面的 `my_crate::A` 是类型路径。反序列化时无需预先知道目标类型，输入自带类型信息。

- `serialize` / `deserialize`：**裸数据**格式，只包含内容本身，
  比如 `{"x":3,"y":4}`，与标准 serde 输出一致。反序列化时调用方需要自己持有目标类型的
  `TypeDB`；序列化嵌套字段时，内部使用的也是这一套。

处理优先级：

1. **已注册的函数优先**：如果类型在 `TypeDB` 中注册了
   `SerdFunc`/`DeseFunc`（通过 `insert_serializer`/`insert_deserializer` 设置），
   则直接调用——这是拥有 serde 实现类型的快路径。

2. **反射兜底**：未注册时，按反射种类（Opaque、Struct、Tuple、Array、
   List、Map、Set、Enum）分派到对应的序列化函数或反序列化 visitor。

另外，Opaque 类型即使没有注册任何序列化函数，也会通过 `Opaque::stringify`
将值字符串化后再序列化。因此**任何反射类型都可以序列化**，只是效率不同。

反序列化采用两阶段策略：类型有默认构造函数时，先构造空值再就地修改字段（快）；
否则先构造 `Dynamic*` 值，再通过 `TypeDB::from_reflect` 转换为目标类型（总能成功，但更慢）。

## 代码生成

反射相关的实现基本不需要手写，`derive` 宏会代劳。相关的宏位于
`zlim-reflect/derive`，主要有两个：

- `#[derive(TypePath)]`：生成 `TypePath` 实现。
  默认使用 `module_path!()` + 类型名拼接路径；也可以通过 `#[type_path = "..."]`
  自行指定。泛型参数会自动拼接到路径中（经 `PathCell` 缓存）。

- `#[derive(Reflect)]`：一次生成反射所需的全部代码：
  - `Reflect` 核心 trait（`reflect_clone`、`reflect_apply`、`reflect_eq`、
    `reflect_hash`、`reflect_debug`、`from_reflect`）
  - `TypePath` 与 `Typed`（提供运行时类型信息）
  - 类型对应的子 trait（结构体生成 `Struct`，枚举生成 `Enum`，依此类推）
  - `TypeDatabase`（类型注册、转换与自动发现）

对于非泛型类型，宏还会额外生成 `register_reflect!` 调用，
程序启动时由 `TypeDB::collect` 自动发现并注册。

`#[derive(Reflect)]` 支持的属性：

| 属性 | 作用 |
|------|------|
| `#[type_path = "..."]` | 自定义类型路径，覆盖默认值 |
| `#[reflect(Opaque)]` | 将类型标记为 Opaque（`Opaque` trait 需自行实现） |
| `#[reflect(Clone)]` | `reflect_clone` 直接复用 `Clone::clone` |
| `#[reflect(Eq)]` | `reflect_eq` 直接复用 `PartialEq::eq` |
| `#[reflect(Hash)]` | `reflect_hash` 直接复用 `Hash::hash` |
| `#[reflect(Debug)]` | `reflect_debug` 直接复用 `Debug::fmt` |
| `#[reflect(Default)]` | 注册默认构造函数，运行时可通过 `TypeDB::default` 构造 |
| `#[reflect(@expr)]` | 附加自定义元数据（存入 `Attributes`，可运行时读取） |

完整内容请查看 `zlim_reflect::derive` 的文档。

## Cargo Features

| Feature | 说明 |
|---------|------|
| `debug` | 用于记录序列化和反序列化栈，提供更清晰的序列化和反序列化失败信息 |
| `glam` | 序列化 `glam` crate 的数据结构 |
| `uuid` | 序列化 `uuid` crate 的 `Uuid` 和 `NonNilUuid` |

---
