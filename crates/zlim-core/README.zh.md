# zlim-core

Zlim Engine 的核心层：一个类 ECS 架构的游戏引擎实现，改自 Bevy 引擎。

## 游戏程序

本库主要提供游戏世界 World 的具体实现。

游戏程序 App 由多个 **World** 组成，每个 World 都驱动一套自身的 ECS 框架，
而游戏主循环就是依次驱动各个 World 的单帧逻辑；详细内容请参考 zlim_app 的实现。

## 核心概念

### 2.1 Component - 组件

组件是**标准的 Rust 结构体**，通过 `#[derive(Component)]` 与实体关联：

```rust
use zlim_core::prelude::*;

#[derive(TypePath, Component, Clone)]
struct Position { x: f32, y: f32 }

#[derive(TypePath, Component, Clone)]
struct Health { current: f32, max: f32 }
```

一个实体可以挂载任意多个**不同类型**的组件，但**同类型组件只能有一个**。

组件与实体一起生成（见 2.2），也可以在运行时通过实体句柄或 `Commands`（见 2.11）增删。

更多细节和示例请参考 `component` 模块的文档。

### 2.2 Entity - 实体

实体是一个**薄句柄**，类似传统游戏引擎中的 GameObject：它本身不携带数据，只是组件的一个"容器标识"。

```rust
use zlim_core::prelude::*;

#[derive(TypePath, Component, Clone)]
struct Position { x: f32, y: f32 }

let mut world = World::alloc();

// 生成一个根实体（第二个参数是父实体，`None` 表示没有父级）
let root = world.spawn(Position { x: 0.0, y: 0.0 }, None).id();

// 生成一个挂在 `root` 下的子实体
let child = world.spawn(Position { x: 1.0, y: 1.0 }, Some(root)).id();

// 通过句柄读取实体的组件
let entity = world.entity(child);
assert_eq!(entity.get::<Position>().unwrap().x, 1.0);
```

与标准 ECS 不同，zlim-core 的实体内置了**层级结构**——每个实体可以有一个父实体和若干子实体。

更多细节和示例请参考 `entity` 模块的文档。

### 2.3 Resource - 资源

资源用于存储**全局（世界）唯一**的数据，例如游戏时钟、消息队列、配置等。

资源**不属于任何实体**，由它的 Rust 类型唯一标识——同一个世界内一个资源类型最多只有一个值。

```rust
use zlim_core::prelude::*;

#[derive(TypePath, Resource)]
struct Score(u32);

let mut world = World::alloc();
world.insert_resource(Score(100));
assert_eq!(world.get_resource::<Score>().unwrap().0, 100);
```

在系统中通过 `Res<T>`（只读）与 `ResMut<T>`（可变）访问资源。

更多细节和示例请参考 `resource` 模块的文档。

### 2.4 World - 世界

**世界**是一个数据容器，存储了该世界的实体、组件、资源、调度表等内容，并对外暴露修改这些数据的操作。

实体与组件总是存在于某个世界中，资源与调度表同样按世界隔离。

```rust, ignore
use zlim_core::prelude::*;

let mut world = World::alloc(); // 分配一个新的世界

// 生成实体、插入资源、注册调度表……所有操作都发生在世界之上
world.spawn_empty(None);
world.insert_resource(Score(0));
```

更多细节和示例请参考 `world` 模块的文档。

### 2.5 System - 系统

系统 System 是一种满足特定要求的 **Rust 函数**，可以通过 `World::invoke` 调用。

`invoke` 能根据函数**签名**判断需要什么参数，然后从 World 中自动构造参数并运行。
系统实例默认会被**缓存**，以加速多次调用（缓存内部数据，例如 `Local` 参数与查询状态）。

```rust
use zlim_core::prelude::*;

#[derive(TypePath, Component, Clone)]
struct Position { x: f32, y: f32 }

#[derive(TypePath, Component, Clone)]
struct Velocity { x: f32, y: f32 }

// 系统参数完全由签名声明：`Query` 查询组件，`Res` 读取资源……
fn movement(mut query: Query<(&mut Position, &Velocity)>) {
    for (mut position, velocity) in query.iter_mut() {
        position.x += velocity.x;
        position.y += velocity.y;
    }
}

let mut world = World::alloc();
let bundle = (Position { x: 0.0, y: 0.0 }, Velocity { x: 1.0, y: 2.0 });
let player = world.spawn(bundle, None).id();

// invoke 根据签名自动构造参数并运行系统
world.invoke(movement, ()).unwrap();
// 也可以使用 invoke_once 避免缓存系统的数据

let entity = world.entity(player);
let position = entity.get::<Position>().unwrap();
assert_eq!((position.x, position.y), (1.0, 2.0));
```

更多细节和示例请参考 `system` 模块的文档。

### 2.6 Job

**Job 是一种特殊的系统**：

- 拥有**稳定的字符串标识**（`JobId` = `name` + `group`，均可在运行时按名字查找）；

- **没有输入参数**（没有 `SystemInput`，`SystemParam` 照常支持）；

- 返回值可以转换成标准的 `ZlimResult<()>`（`()`、`bool`、`Result<(), E>`、`Result<bool, E>` 均可）；

- 常规系统需要通过 `World::invoke` 运行；而 Job 需要插入到 World 的 **Schedule** 中；

- Schedule 会**自动最大化地并行执行**内部 Job（根据函数签名检测数据访问冲突，互不冲突的 Job 并行运行），也允许你显式指定某些 Job 的先后顺序；

- 内置**循环依赖检查**：循环依赖会导致程序 Panic。

你可以用 `#[job_fn]` 属性标记一个可系统化的函数，或者使用 `job!` 标记一个可系统化的表达式：

```rust
use zlim_core::prelude::*;

#[job_fn(type = GreetJob)]
fn greet() { println!("Hello World!"); }

job! {
    type: PipeGreetJob,
    system: (|| true).pipe(|In(flag)| { let _ = flag; }),
}
```

`type` 用于指定一个类型名，这会生成一个用于标识的空类型。换言之，`job` 是强类型的。

另外，你可以自定义 `name`，还可以添加一些系统运行的前置条件：

```rust
use zlim_core::prelude::*;

// Local<T>: system local data, should support `Default`
fn once(mut flag: Local<bool>) -> bool {
    // When called for the first time, the value is `false` ( bool::default() ).
    if !*flag { *flag = true; true } else { false }
}

#[job_fn(type = GreetJob, name = "test::GreetJob", run_if = once)]
fn greet() { println!("Hello World!"); }
```

可以使用 JobGroup 批量高效地组织 Job。

```rust
use zlim_core::prelude::*;

#[job_fn(type = JobA, name = "group_job_a")]
fn job_a() {}

#[job_fn(type = JobB, name = "group_job_b")]
fn job_b() {}

job_group! {
    type: MyGroup,
    name: "my_group",
    jobs: [JobA, JobB],
    order: [[JobA, JobB]], // 声明执行顺序，保证 B 在 A 之后执行。
}
```

更多细节和示例请参考 `job` 模块的文档。

### 2.7 Schedule

**Schedule 是 Job 的容器**，用于规划内部 Job 的执行顺序。

通常 **Schedule 之间需要串行**（例如先 Update 后 Render），但 **Schedule 内部的 Job 可以并行**。

从层级关系上看：`Schedule > ScheduleStage > JobGroup > Job`：

- 每个 Job 都属于一个 **JobGroup**（未指定则为匿名组）；

- 每个 JobGroup 和 Job 也都属于一个 **Stage**（未指定则为匿名阶段）；

- 两个未指定先后顺序的 Stage 完全可以并行执行。

这种层级关系允许你高效地组织 Job 的执行顺序。

命名 Stage 会自动生成 `StageBegin` / `StageEnd` 标记 Job，从而运行用户插入顺序约束：

- `insert_order`: 强序，前一 Job 成功后运行，延迟命令保证可见

- `insert_weak_order`: 弱序，前一 Job 完成后运行，无论是否成功，延迟命令保证可见

- `insert_relaxed_order`: 宽松序，前一 Job 完成后运行，无论是否成功，不保证延迟命令可见

```rust
use zlim_core::prelude::*;

#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MainLoop {
    Update,
    Render,
}

#[derive(TypePath, ScheduleStage)]
enum FixedStage {
    PreUpdate,
    Update,
}

#[job_fn(type = PhysicsStep, name = "physics_step")]
fn physics_step() {}

#[job_fn(type = RenderFrame, name = "render_frame")]
fn render_frame() {}

let mut world = World::alloc();

// Update 调度表：PhysicsStep 属于 Update 阶段
world.schedules_mut()
    .entry(MainLoop::Update)
    .insert::<PhysicsStep>(FixedStage::Update);
    // ↑ 阶段不存在时或自动插入

world.schedules_mut()
    .entry(MainLoop::Update)
    .insert_stage(FixedStage::PreUpdate);
// 注意 insert_order 不会插入阶段，调用前需保证阶段已经存在。

// 设置 `FixedStage` 的执行顺序：
world.schedules_mut()
    .entry(MainLoop::Update)
    .insert_order(&[
        FixedStage::PreUpdate.stage_end(),
        FixedStage::Update.stage_begin(),
    ]);

// Render 调度表：与 Update 串行
world.schedules_mut()
    .entry(MainLoop::Render)
    .insert::<RenderFrame>(()); // 匿名阶段

// 执行调度表
world.run_schedule(MainLoop::Update);
world.run_schedule(MainLoop::Render);
```

### 2.8 Query - 查询

Query 是一个极其重要的系统参数，用于高效地访问**实体与组件**的数据。

组件的数据存储方式请查看 `table` 模块的模块文档，这里只展示 query 用法：

```rust
use zlim_core::prelude::*;

#[derive(TypePath, Component, Clone)]
struct Position { x: f32, y: f32 }

#[derive(TypePath, Component, Clone)]
struct Player;

// 遍历所有拥有 `Player` 组件的实体上的 `Position`
fn total_x(query: Query<&Position, With<Player>>) -> f32 {
    query.iter().map(|p| p.x).sum()
}
```

Query 由两部分组成：

- **QueryData** 取什么数据：`&T` / `&mut T` / `EntityId` / 元组组合等
- **QueryFilter**哪些实体匹配：`With<T>` / `Without<T>` / `Changed<T>` / `Added<T>` / `And` / `Or` 等

更多细节和示例请参考 `query` 模块的文档。

### 2.9 Message - 消息

引擎内置了 **MessageQueue** 的实现。

MessageQueue 是一个**双缓冲区轮转**的消息队列：
消息写入写缓冲区，轮转（`update`）后切换到读缓冲区供读取，旧读缓冲区被清空。

**单一消息可以被多消费者读取**，多个消费者各自维护独立的游标，互不干扰。

```rust
use zlim_core::prelude::*;

#[derive(TypePath, Message)]
struct Ping;

let mut world = World::alloc();
world.register_message::<Ping>(); // 启动时注册，创建队列资源

// 生产者系统
fn producer(mut writer: MessageWriter<Ping>) {
    writer.write(Ping);
}

// 消费者系统
fn consumer(mut reader: MessageReader<Ping>) {
    for _ in reader.read() {
        println!("received!");
    }
}
```

缓冲区通过内置的资源系统实现，并提供 `MessageWriter` / `MessageReader` / `MessageMutator` 等系统参数工具。

缓冲区的轮转与消息丢弃策略见 `message` 模块的文档——简单地说，它**通常保留两帧的数据**，所以每帧都运行的 Job 不会遗漏消息。

### 2.10 变更检测

zlim-core 对**所有资源**和**组件**数据都追踪了变更，包含 **Added** 与 **Changed** 两个时刻：

- **Added**：在新增时设置（不包含 overwrite）；
- **Changed**：当你获取一个资源 / 组件的**可变引用**时更新；

变更检测是**基于时间段**的：
通过 World 进行的变更检测只会检测**当前帧**发生的变更；
通过 Job 进行的变更检测只会检测**此 Job 上一次运行之后（不包括上一次运行时）**发生的变更。

```rust
use zlim_core::prelude::*;

#[derive(TypePath, Component, Clone)]
struct Health { current: f32, max: f32 }

// 查询过滤器：只处理新增 / 自上次运行以来变更过的组件
fn on_created(query: Query<&Health, Added<Health>>) {
    for health in query.iter() {
        println!("new entity with max {}", health.max);
    }
}

fn on_damaged(query: Query<&Health, Changed<Health>>) {
    for health in query.iter() {
        println!("health changed to {}", health.current);
    }
}

// 资源同样暴露变更状态
fn check_time(time: Res<Time>) {
    if time.is_changed() {
        println!("time changed!");
    }
}
```

### 2.11 Commands- 延迟命令

实体生成、资源增删等操作需要世界的**独占引用**，这会导致对应的 System / Job 无法并行执行。

但通常这些操作都是条件执行的，不应使整个系统独占世界。
此时可以使用 **Commands**，将相关命令推送到**延迟队列**——不需要世界的独占访问，从而提高并行度。

**Schedule 会自动插入同步点**，在需要的时候执行这些延迟命令，以保证可见性。

```rust
use zlim_core::prelude::*;

#[derive(TypePath, Component, Clone)]
struct Position { x: f32, y: f32 }

// 收集命令，稍后统一应用
fn spawn_units(mut commands: Commands, query: Query<EntityId>) {
    for entity in query.iter() {
        let _ = commands.with_entity(entity).insert(Position { x: 0.0, y: 0.0 });
    }
    let _ = commands.spawn_empty(None);
}

// 基于时间的**延时**命令（见 time 模块）
fn delayed_spawn(mut commands: Commands) {
    let _ = commands.delayed().secs(1.0).spawn_empty(None);
}
```

更多细节和示例请参考 `command` 模块的文档。

### 2.12 Time - 时间

zlim-core 内置了一套**时钟系统**，包含真实时钟、虚拟时钟、固定步长时钟等，都以资源形式存储。

时钟通常通过 `refresh_metadata` 函数更新（App 主循环每帧自动调用一次）。

```rust
use zlim_core::prelude::*;

// 系统里直接读取时钟资源
fn game_time(time: Res<Time>) {
    let delta = time.delta();     // 上一帧流逝的时间
    let elapsed = time.elapsed(); // 时钟启动以来经过的总时间
}

// 引擎循环中：推进时钟，然后消费固定步长
let mut world = World::alloc();
World::refresh_metadata(&mut world);

if World::step_fixed(&mut world) {
    // 恰好累积了一个固定步长，执行一次固定步长逻辑（物理等）
}
```

更多细节和示例请参考 `time` 模块的文档。
