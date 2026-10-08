# zlim-error

引擎的错误类型，以及 `#[derive(Error)]` 宏。

## `ZlimError` —— 核心错误类型

`ZlimError` 把任意 `Error + Send + Sync + 'static` 分配到堆上，
然后附带 `Severity` 和一些额外信息。

```rust
use zlim_error::{Severity, ZlimError};

let err = ZlimError::warning("disk is nearly full".to_string());
assert_eq!(err.severity(), Severity::Warning);

// Severity metadata can be adjusted without touching the payload.
let err = err.with_severity(Severity::Error);
assert_eq!(err.severity(), Severity::Error);

// Unbox the payload when handing it to foreign code.
let boxed: Box<dyn std::error::Error + Send + Sync> = err.take();
assert_eq!(boxed.to_string(), "disk is nearly full");
```

开启 `backtrace` feature 后，`Severity::Error` 与 `Severity::Panic` 会捕获栈回溯信息，
并随错误一起打印(默认行为)。捕获的阈值可以用过 `ZlimError::set_backtrace_threshold` 覆写。

与默认的 panic hook 不同，ZlimError 的回溯显示默认会跳过一些
冗余(无用)行，可以通过设置 `ZLIM_BACKTRACE=full` 以关闭过滤。

如果启用了 zlim-app 的 `PanicHandlerPlugin`，且 panic 由 ZlimError 引起、
并且回溯已被捕获，它会直接显示 ZlimError 的内容并跳过默认 hook，错误输出会更清晰。

## 严重级别

| 级别 | 含义 |
|-------|---------|
| `Severity::Ignore` | 可以安全地完全丢弃。 |
| `Severity::Debug` | 无害，但可能有助于调试。 |
| `Severity::Info` | 没有出错，但仍值得上报。 |
| `Severity::Warning` | 意料之外，但可恢复。 |
| `Severity::Error` | 真正的错误;程序可以继续运行。 |
| `Severity::Panic` | 致命;执行无法继续。 |

`ZlimError::merge_severity` / `ZlimError::map_severity` 在保留载荷的前提下
提升或变换级别，例如把内层错误的级别提升为失败操作的级别。

## 错误处理

可能失败的函数返回 `ZlimResult<T>`,并通过 `IntoZlimResult` 转换而来;
后者的输出类型表明了调用者想知道什么。

`IntoZlimResult<()>` 是命令(command)的返回值:

- `()` —— `Ok(())`;
- `Result<(), E>` —— 该值,或经由 `E: Into<ZlimError>` 转换的错误。

`IntoZlimResult<bool>` 是任务(job)的返回值:

- `()` —— `Ok(true)`;运行成功;
- `bool` —— `Ok(self)`;`true` 对应运行成功,`false` 表示自身运行完成
  但是不允许后继任务;
- `Result<(), E>` / `Result<bool, E>` —— `Ok` 时转换成上述值,
  `Err` 时运行失败。

当 `ZlimResult` 返回 `Err` 时,通常由 `zlim-core` 的默认错误处理器处理错误。

## `#[derive(Error)]` 宏

在结构体或枚举上派生 `Error` 会生成:

- **总是** —— `core::error::Error` 的实现(这也要求类型实现 `Debug`，
  所以要一并派生 `Debug`)。

- **`#[error("...")]`** —— `Display` 的实现。该字符串的用法类似
  `format!`:具名字段按名字进入作用域，元组字段按 `_0`、`_1`、…，
  并且允许额外的参数。

- **`#[zlim_error(severity)]`** —— 生成 `From<Self> for ZlimError` 实现。

对于枚举，写在类型上的属性会充当默认值；而变体属性可以覆写它们:

```rust
use zlim_error::derive::Error;
use zlim_error::{Severity, ZlimError, ZlimResult};

#[derive(Debug, Error)]
#[error("validation failed")]
#[zlim_error(warning)]
enum ValidationError {
    #[error("age {_0} is negative")]
    NegativeAge(i32), 
    #[error("limit {limit} exceeded")]
    #[zlim_error(error)] // override the default severity
    LimitExceeded { limit: i32 }, 
}

fn validate(age: i32, limit: i32) -> ZlimResult<()> {
    if age < 0 {
        return Err(ValidationError::NegativeAge(age).into());
    }
    if limit > 100 {
        return Err(ValidationError::LimitExceeded { limit }.into());
    }
    Ok(())
}

let _ = ZlimError::from(ValidationError::NegativeAge(-1)); // `From` was derived
```
