# zlim-reg

基于静态初始化的分布式数据收集器。

## 声明注册表

当你需要为某个类型收集数据时，使用 `collect!` 宏：

```rust, ignore
struct Plugin { name: &'static str }

zlim_reg::collect!(Plugin);
```

`collect!` 宏会为类型实现本库的 `Collect` trait。由于 Rust 的孤儿原则，
它只能在类型自身所属的 `crate` 内调用。

另外，输入的类型必须是确定的，不能存在未定的泛型参数。

```rust, ignore
struct Foo<T>(T);

zlim_reg::collect!(Foo<u32>); // 正常 ✅：Foo<u32> 是确定的类型

zlim_reg::collect!(Foo<T>);   // 编译错误 ❌：T 类型不确定
```

## 提交数据

提交数据则使用 `submit!` 宏，语法是 `$expr => Type`，例如：

```rust, ignore
zlim_reg::submit!(Plugin { name: "1" } => Plugin);
```

`submit!` 宏通常在模块作用域（函数体外）调用，只要能访问到注册表对应的类型。

传入的表达式必须是常量表达式，即需要在编译期求值。

## 访问数据

通过本库的 `iter` 函数创建迭代器，即可遍历所有通过 `submit!` 提交的数据：

```rust, ignore
for item in zlim_reg::iter::<Plugin>() {
    std::println!("{}", item.name);
}
```

迭代器返回的元素类型是 `&'static T`，即 `submit!` 宏内部定义的静态变量。

如果你需要收集只能在运行时创建的数据，可以通过本库收集“构造函数”的函数指针，然后在运行时迭代并调用它们。

## 内部实现

C 语言在 GCC 和 Clang 编译器中支持 `__attribute__((constructor))` 函数属性，
这些被标记的函数会在 `main` 之前执行，本库正是基于类似的方式实现数据收集。

1. `collect!` 宏会让类型实现 `Collect` trait，从而为该类型提供一个链表（即注册表）。

2. `submit!` 宏会用表达式创建一个静态变量，并声明一个在 `main` 之前执行的函数，
   将静态变量的引用注册到链表中。

3. `iter` 返回一个迭代器，在运行时迭代类型对应的链表。

---

本库代码改自 inventory 库。

本库在 wasm 目标上内置了调用逻辑，因此用户无需手动调用 `__wasm_call_ctors`。
