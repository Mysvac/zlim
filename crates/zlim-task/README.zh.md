一个轻量但高效的异步任务池，专为 zlim engine 设计。

提供了一套统一的任务池接口，以便后续扩展实现。

- 在常规平台上默认使用多线程模式（Linux、Windows、Android、macOS）。

- 在 WASM 目标使用浏览器特定的单线程模式。

- 其他未知平台或者显式启用 `single_thread` feature 时回退到标准单线程模式。

## 架构

```text
┌─────────────────────────────────────────────────────┐
│                      TaskPool                       │
│  ┌──────────┐  ┌─────────────┐  ┌────────────────┐  │
│  │  spawn   │  │ spawn_local │  │ spawn_to_main  │  │
│  │ (Send)   │  │  (!Send)    │  │  (Send → main) │  │
│  └────┬─────┘  └─────┬───────┘  └───────┬────────┘  │
│       │              │                  │           │
│  ┌────▼──────┐ ┌─────▼───────┐  ┌───────▼────────┐  │
│  │ Pool Exec.│ │ Local Exec. │  │  Main Executor │  │
│  │           │ │   Executor  │  │                │  │
│  │ working-  │ │             │  │                │  │
│  │ stealing  │ │ Background  │  │   Background   │  │
│  └───────────┘ └─────────────┘  └────────────────┘  │
│  ┌──────────────────────────────────────────────┐   │
│  │                  Scope                       │   │
│  │     spawn / spawn_local / spawn_to_main      │   │
│  │                                              │   │
│  │     Drives Executors + Collects Results      │   │
│  └──────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
```

- `spawn` 在语义上，表示生成一个任务，由任意线程执行，因此需要 `Send` 约束。

- `spawn_local` 在语义上，表示生成一个任务并在当前线程执行，因此不需要 `Send`。

- `spawn_to_main` 则是将任务送往主线程执行，同样需要 `Send` 约束。

这仅在多线程模式中有实际差异，单线程模式中三者是一致的。

上述函数都返回一个实现了 `Future` 的 `Task<T>` 句柄。可使用 `Task::cancel`
取消任务，或使用 `Task::detach` 丢弃句柄但不影响任务的执行（任务的返回值将被丢弃）。

Scope 系列同样有 `spawn`、`spawn_local` 和 `spawn_to_main` 三个函数。

不同之处在于 Scope 会阻塞等待内部的所有任务完成，并直接返回 `Vec<T>`。因此
scope 可以创建包含非 `'static` 参数的任务。

## 模式

### 多线程模式

Windows、Linux、macOS 和 Android 上的默认模式。

工作线程使用工作窃取调度器：每个工作线程拥有本地队列，另有共享的全局队列，
并通过随机选择进行跨线程窃取。

每个池至少创建一个工作线程；工作线程空闲时休眠，逐个唤醒以避免惊群效应。

默认情况下，多线程模式会在第一次创建 `TaskPool` 时启动一个虚假的主线程,
它独占全局 `MainExecutor` 并持续轮询直到进程结束。因此无论哪个线程创建了任务池，
`spawn_to_main` 的任务都会被送往此线程执行，这保证了测试环境的行为一致性。

但在真实应用中，推荐在程序代码的开头通过 [`designate_main_thread`] 将当前线程标记为主线程，
此时创建 `TaskPool` 将不会启动虚拟主线程。`zlim_app` 模块提供的 `#[zlim_main]` 宏
会自动在主函数开头调用它。

注意 `designate_main_thread` 不应在测试环境中使用。测试用例分散在各个子线程执行，将子
线程标记为 `designate_main_thread` 很容易导致程序出现长久睡眠（死锁）。

### 单线程模式

未知平台的回退模式。所有任务都在当前线程执行——没有后台线程。

必须通过 [`run_local`] 或 [`TaskPool::scope`] 显式驱动池才能取得进展。

### WASM 模式

任务通过 `web_task` 提交到浏览器的微任务队列。

`spawn` / `spawn_local` / `spawn_to_main` 都会路由到浏览器事件循环。

`scope` 使用 Rust 侧的执行器同步驱动任务。

## 语义

### 后台任务

`TaskPool` 本身的 `spawn` 系列函数用于创建“后台任务”。

语义上，这些函数将工作放入**不会**被自动驱动的后台队列，实际的执行点不尽相同：

- 多线程模式时，这些任务可能会被工作线程在后台完成，也可能等待主线程驱动本地任务。

- 标准单线程模式中，这些任务必须等到显式的 [`run_local`] 调用才会执行。

- WASM 中，可能要等 Rust 侧单帧逻辑结束，将执行器让给浏览器时才会执行。

因此，对 `spawn` 返回的 `Task` 进行阻塞等待很可能会死锁。
它几乎总是用于非阻塞等待的异步后台任务，并通过轮询或者消息传递的方式接收结果。

### 作用域任务

`TaskPool::scope` 用于创建作用域任务。与 `TaskPool::spawn` 不同，作用域本身
会驱动本地任务执行，保证任务得以完成。

单线程模式（包括 WASM）中 `scope` 创建的任务都会在本地队列，无论使用哪个 `spawn` 函数。

多线程模式中，`spawn` 会将任务送往任意工作线程，`spawn_local` 送入当前线程的
本地队列，而 `spawn_to_main` 则送往“主线程”。

## 性能说明

- **单线程模式**：任务分发速度大约是 `bevy_tasks` 的 2 倍。任务直接使用线程局部存储的块状链表（`BlockList`），
  避免了 `async_executor` 的诸多原子操作开销。任务执行速度理论上没有差别，但实际上由于分发速度和任务紧密存储，小任务会快上大约 10%。

- **多线程模式**：任务分发速度大约是 `bevy_tasks` 的 4 倍。计算密集型任务（计算开销远大于分发开销）耗时基本一致。

## 示例

### Spawn

```rust, ignore
use zlim_task::TaskPool;
let pool = TaskPool::new();

// Send + 'static —— 派发到工作线程
let task = pool.spawn(async { 1 + 1 });

// !Send + 'static —— 留在当前线程
let task = pool.spawn_local(async { 2 });

// Send + 'static —— 发送到主线程，唤醒主线程 waker
let task = pool.spawn_to_main(async { 3 - 1 });
```

### Scope

```rust
use zlim_task::TaskPool;
let pool = TaskPool::new();

let results: Vec<i32> = pool.scope(|scope| {
    scope.spawn(async { 1 + 1 });     // → 工作线程
    scope.spawn_local(async { 2 });   // → 当前线程
    scope.spawn_to_main(async { 3 - 1 }); // → 主线程
});

assert_eq!(&results, &[2, 2, 2]);
```

`scope` 会阻塞当前线程，驱动所有本地任务，并等待每个派生的任务完成后才收集结果。

`Scope` 句柄是 `Send` 的——可以跨线程移动——因此 scope 可以嵌套：

```rust
use zlim_task::TaskPool;
let pool = TaskPool::new();

let results: Vec<i32> = pool.scope(|scope| {
    scope.spawn(async {
        scope.spawn_local(async { 1 });
        2 - 1
    });
});

assert_eq!(&results, &[1, 1]);
```

## 静态任务池

三个针对不同工作负载的全局单例池：

| 池 | 用途 | 默认线程数（多线程模式） |
|------|---------|-------------------------|
| [`MainTaskPool`] | 并行算法后端，以及单帧计算 | 可用线程的 50%（至少 1） |
| [`AsyncTaskPool`] | 可能跨多帧的计算密集型任务 | 可用线程的 25%（至少 1） |
| [`IoTaskPool`] | 可能有长时间等待的 IO 密集型任务 | 可用线程的 25%（至少 1） |

每个池都是惰性初始化的：首次调用 `get()`（或任何 `Deref` 用法）会隐式创建一个
采用上表默认配置的 `TaskPool`。典型使用无需显式设置：

```rust, ignore
use zlim_task::{MainTaskPool, TaskPool};

// 首次访问时隐式初始化——直接用即可
let task = MainTaskPool::get().spawn(async { /* ... */ });
```

如需自定义配置，请在首次访问前调用 `try_init`：

```rust, ignore
use zlim_task::{MainTaskPool, TaskPoolBuilder};

// 自定义初始化——必须在首次 get() 之前调用
let did_init = MainTaskPool::try_init(|| {
    TaskPoolBuilder::new()
        .thread_count(8)
        .thread_name("CustomMain")
        .build()
});

assert!(did_init); // 首次调用返回 true，已初始化则返回 false
// 单线程模式中，可能始终返回 false 。
```

**多线程模式：** 当池尚未初始化时 `try_init` 返回 `true`（应用自定义配置），
已初始化则返回 `false`。

**单线程 / WASM 模式：** 三个池共享同一个全局 `TaskPool`。
`try_init` 始终返回 `false`——这些模式不支持自定义初始化。

### TaskPoolConfigs

[`TaskPoolConfigs`] 通过一次调用初始化全部三个全局池，按内部的 [`TaskPoolConfig`]
字段分配可用线程——默认 `IoTaskPool` 占 `25%`、`AsyncTaskPool` 占 `25%`、
其余线程归 `MainTaskPool`：

```rust
use zlim_task::TaskPoolConfigs;

TaskPoolConfigs::default().apply();
```

请在启动时、任何池被首次访问（例如 `MainTaskPool::get()`）之前调用一次。
在单线程 / WASM 模式下它是 no-op；在测试环境中，如果某个池已被初始化，
`apply` 会直接返回。

注意：如果通过隐式初始化使用 `MainTaskPool`、`AsyncTaskPool` 等全局池，
请保证它们的"第一次获取"先于其他任务池——除非先调用 [`designate_main_thread`]，
虚假主线程由第一个创建的 `TaskPool` 启动，`spawn_to_main` 的任务会路由到该线程。

## ParallelSlice

对切片进行并行批量操作的扩展 trait：

```rust
use zlim_task::ParallelSlice;

let data = [3, 7, 9, 12];

let found = data.par_contains(&7);           // true
let pos   = data.par_position(|v| *v > 5);   // Some(1)
let doubled: Vec<_> = data.par_map(|v| *v * 2);
```

当启用 `multi_thread` cfg 路径时，工作会通过 [`MainTaskPool`] 分发到各工作线程。
当多线程被禁用（单线程或 WASM 构建）时，方法退化为顺序迭代。

## block_on

```rust
let result = zlim_task::block_on(async { 42 });
assert_eq!(result, 42);
```

在当前线程阻塞等待一个 future，同时驱动异步执行器以避免死锁。

- **主线程**上（多线程模式）：阻塞期间同时驱动 `MainExecutor` 与
  `LocalExecutor`。
- **工作线程**上：阻塞期间同时驱动池执行器与 `LocalExecutor`。
- 单线程 / WASM 模式下：阻塞期间驱动执行器。

底层通过 `futures_lite::future::block_on` 停放线程；启用 `async_io` feature
时改用 `async_io::block_on`。

## invoke_on_main

将单个闭包送往主线程执行，并阻塞等待结果。

如果在工作线程上调用，等待期间**不会**驱动该工作线程自身的任务——极端情况下
可能导致所有线程休眠（死锁）。因此它主要由程序顶层入口调用，通常仅用于
`App::run` 的内部实现。

## Cargo Features

| 标志 | 效果 |
|------|--------|
| `single_thread` | 无论平台如何，强制使用单线程模式 |
| `async_io` | 使用 `async_io::block_on` 代替 `futures_lite` |

---
