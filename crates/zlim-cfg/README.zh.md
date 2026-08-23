一些编译辅助宏，用于简化代码中的编译控制。

## enabled! & disabled!

本库定义并导出了两个非常简单的宏：`enabled!` 和 `disabled!` 。

当宏内部没有任何参数时，它们会被替换为布尔字面量 `true` 或 `false` ：

```rust, ignore
assert!( zlim_cfg::enabled!() );
assert!( !zlim_cfg::disabled!() );
```

当内部存在语句时，`enabled!` 会原样保留内容，而 `disabled!` 则会完全移除内容：

```rust, ignore
let mut x: u32 = 0;

zlim_cfg::enabled! {
    x |= 0b01;   // 正常保留
}

zlim_cfg::disabled! {
    x |= 0b10;  // 直接被宏消除
}

assert_eq!(x, 0b01);
```

还可以使用 `if {} else {}` 语法，条件性地保留内容：

```rust, ignore
let mut x: u32 = 0;

zlim_cfg::enabled! {
    if {
        x |= 0b0001;   // 正常保留
    } else {
        x |= 0b0010;   // 不生效
    }
}

zlim_cfg::disabled! {
    if {
        x |= 0b0100;   // 不生效
    } else {
        x |= 0b1000;   // 正常保留
    }
}

assert_eq!(x, 0b1001);
```

## define_alias!

上面两个宏本身的作用不大，但你可以使用 `define_alias!` 宏为编译选项定义别名：

```rust, ignore
zlim_cfg::define_alias!{
    #[cfg(test)] => test,
    #[cfg(feature = "serde")] => serde,
}
```

上面的语句定义了 `test` 和 `serde` 别名，因此你可以像 `enabled!` 和 `disabled!` 一样使用它们：

```rust, ignore
test! {
    if {
        // 这段内容仅在 #[cfg(test)] 时生效
    } else {
        // 这段内容仅在 #[cfg(not(test))] 时生效
    }
}
```

当 `cfg` 成立时，定义的别名等效于上面提到的 `enabled!` ；当 `cfg` 不成立时，别名等效于 `disabled!` 。

## switch!

本库提供的最后一个宏是 `switch!`，它允许你像 `match` 语句一样让指定的代码生效。

```rust, ignore
zlim_cfg::switch! {
    #[cfg(feature = "std")] => { /* content */ },
    enabled => { /* content */ },
    _ => { /* content */ },
}
```

它会从上至下依次判断，仅激活最早生效的分支，类似 Rust 1.95 内置的 `cfg_select!` 宏。

分支条件可以使用 `#[cfg(..)]`、本库的 `enabled` 和 `disabled`、`define_alias!` 定义的别名，
以及放在最后表示默认分支的 `_` 。

## 遗留问题

使用本库的宏决定模块是否被编译时，跳过的模块可能不会被 `rustfmt` 选中。

因此，控制模块是否编译时依然推荐使用 `cfg_select` 。

本库主要用于控制内部代码块的编译情况，这比 `cfg_select` 更简洁。

---

本库代码改自 bevy_platform 。
