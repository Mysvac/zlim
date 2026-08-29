# zlim-utils

zlim 引擎的基础工具库。

## num

- `NonMax*` 整型包装，用于 niche 值优化，类似于 `NonZero*`。

## str

- `SmolStr` —— 小缓冲区优化的不可变字符串；封装自 `smol_str` crate。
- `intern_str` —— 通过全局读优化的字符串池，将 `&str` 驻留为 `&'static str`。
- `format_smol!` —— 通过 `format_args!` 创建 `SmolStr` 的宏。

## vec

- 小缓冲区优化的向量；重新导出 `fastvec` crate。

## mem

- `Bump` —— 作用域化的 bump 分配器，用于临时数据（在 drop 时释放）。
- `Global` —— 互斥锁保护的静态 bump 分配器，用于 `'static` 生命周期数据。

## mpmc

- 无界异步多生产者、多消费者通道。
- 无锁，基于 `SegQueue` 和 `Event` 构建；比 `async-channel` 更快。
- 多个 `Receiver` 可以克隆并并发使用。

## mpsc

- 无界异步多生产者、单消费者通道。
- 无锁，基于 `SegQueue` 和 `Event` 构建；比 `async-channel` 更快。
- 单个 `Receiver`（不可 `Clone`），避免消费者端的竞争。

## event

- 异步事件监听；重新导出 `event_listener` crate。

## hash

- 基于 `hashbrown` 的确定性 `HashMap` 和 `HashSet`，默认使用固定种子（`FixedState`）。
- 自定义哈希器：`NoopState`（恒等哈希）、`SparseState`（用于 ECS 实体 ID 的斐波那契哈希）。

## sync

- `SpinLock` —— 带指数退避的自旋互斥锁。
- `SegQueue`、`ArrayQueue` —— 无锁并发队列（移植自 crossbeam）。
- `Backoff`、`OnceFlag`、`Parallel` —— 同步辅助工具。

## ext

- `ArrayDeque` —— 栈上的固定容量环形缓冲双端队列。
- `BlockList` —— 带空闲块池的块链表队列。
- `CachePadded` —— 缓存行对齐包装，用于减少伪共享。
- `ThreadLocal` —— 基于桶（bucket）分配的线程本地存储。
- `TypeMap` —— 以 `TypeId` 为键、使用无操作哈希器的映射。

## exp

- `SyncUnsafeCell` —— 不稳定特性 `core::cell::SyncUnsafeCell` 的临时实现。
- `SyncView` —— 不稳定特性 `core::sync::SyncView` 的临时实现。

## macros

- `range_invoke!` —— 基于范围的重复宏调用。
- `define_atomic_id!` —— 由原子计数器支撑的唯一 ID 生成器。
- `once_expr!` —— 单次执行表达式，比 `Once` 更快。

## debug

- `DebugName` —— 条件性捕获类型名，用于诊断消息。
  仅在 debug 构建或本库的 `debug` feature 启用时存储真实的类型名；
  在 release 构建且未启用 `debug` feature 时为零大小占位符，返回 `_unknown_`。

- `DebugLocation` —— 条件性捕获调用点的 `Location`，用于诊断消息。
  仅在 debug 构建或本库的 `debug` feature 启用时携带真实的调用者信息；
  在 release 构建且未启用 `debug` feature 时为零大小占位符。

---
