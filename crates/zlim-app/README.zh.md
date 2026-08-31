# zlim-app

zlim 引擎的应用层：`App` 的创建、插件系统、主循环（runner）与调度驱动。

## App 的结构

`App` 由一个**主应用（main sub-app）**与多个**子应用（sub-app）**组成：

- **主世界（main world）**：`App` 自身就是一个 `SubApp`（其 "main" 子应用），持有
  游戏逻辑所在的 `World`。

- **子世界（sub world）**：通过 `App::insert_sub_app(label, sub_app)` 添加，每个
  `SubApp` 有自己独立的 `World`、插件列表、更新调度与可选的 **Extract 函数**。
  渲染世界就是一个典型的子应用：每帧从主世界抓取数据快照并渲染，而不阻塞主世界。

```rust, ignore
let mut app = App::new();
let mut render_sub_app = SubApp::new();
// 每个子应用拥有自己的世界：
render_sub_app.world_mut().insert_resource(/* ... */);
// Extract 步骤：每帧把主世界数据拷贝到子世界：
render_sub_app.set_extract(|main, sub| { /* main: &mut World, sub: &mut World */ });
app.insert_sub_app(Render, render_sub_app);
```

## App 的运行流程

1. **应用插件**：`App::run` 首先调用 `App::build`（如果已被调用，则跳过）。
   按 `build → apply → cleanup` 的顺序执行所有已添加的插件。
2. **启动 runner**：build 完成后，`App` 使用 `set_runner` 设置的 runner（或默认
   runner）驱动帧循环，最后返回 `AppExit`。

## 常规主循环流程

每个世界（主世界与子世界）都需要定义一个**默认的调度表（default schedule）**；没有则跳过。

单帧流程（`App::update`）如下：

1. **主世界**：
   - `World::refresh_metadata`刷新变更检测、时间等元数据。
   - 运行其默认调度（通常是 `Main`，由 `MainSchedulePlugin` 提供）。

2. **子世界**（依次执行）：
   - 若注册了 **Extract 函数**，则优先执行 `extract(main_world, sub_world)` 。
    这允许你从主世界抓取数据，或者修改主世界。
   - `World::refresh_metadata` 刷新子世界的变更检测、时间等元数据。
   - 运行其默认调度，需要自行提供。
   - 清理子世界的变更追踪器（`clear_trackers`）。

3. 最后清理主世界的变更跟踪器（`clear_trackers`）。

## 默认 runner

未设置 runner 时，默认 runner 按上述流程**运行一帧然后结束**：

```rust, ignore
fn run_once(mut app: App) -> AppExit {
    app.update();                       // 跑一帧（主世界 + 子世界）
    app.should_exit().unwrap_or(AppExit::Success)   // 检查退出事件
}
```

需要持续运行（游戏循环、按帧步进等）时，请使用 `ScheduleRunnerPlugin`（`RunMode::Loop` 等）或自定义 runner。

`AppExit` 是一个消息 （`zlim_core::Message`），用于标识程序需要退出。
循环应用通常会在每帧检查是否收到此消息，如果收到则完成当前帧后退出执行。

## 插件

插件允许你以模块化方式为 `App` 添加功能。

插件是**延迟生效**的：`add_plugins` 只是暂存插件，不执行任何操作，直到 `App::build`（或 `App::run`）时才会真正生效。

插件分三个阶段，：

| 阶段 | 时机与用途 |
|---|---|
| `build` | 初始化插件自身；在 App 中添加**依赖插件**；设定各插件 `apply` 的执行顺序（依赖图）。此阶段可继续添加插件。 |
| `apply` | 此时插件的列表已**不可变**（不能再添加）。通常在此运行插件逻辑、修改世界（注册系统/资源/消息等）。执行顺序由 build 阶段建立的依赖图决定（拓扑序，环会 panic）。 |
| `cleanup` | 清理插件自身的数据。**cleanup 阶段完成后，所有插件都会被移除**，因此插件添加的游戏逻辑不应依赖插件自身的存在。 |

`apply` 是必须的，`build` 和 `cleanup` 提供了默认空实现。

## 日志插件

日志配置是**独立的**：虽然名为 `LogPlugin`（来自 `zlim-log`），但它没有实现
`Plugin` trait，也不属于插件生命周期（`build`/`apply`/`cleanup`）。直接在
`App` 上初始化日志：

- `App::init_logger()` — 使用默认配置初始化全局日志。
- `App::with_logger(config)` — 使用给定的 `LogPlugin` 配置初始化（等效于
  `LogPlugin::apply`）。

与插件不同，这两个调用是**立即生效**的：在调用点即安装全局 subscriber，此后所有
`App` 操作都可见于日志。建议在 `App::new()` 之后立即调用其中一个，以避免各种
日志不可见导致的奇怪问题；尤其是启用 `trace` feature 时，若在日志启用前就创建
Job、Schedule 等内容，内部的 span 将始终处于 disable 状态。

若不初始化，则不会启用日志——程序中的各种警告、错误信息都不会正常显示。

日志是进程级共享的：只能初始化**一次**，只有第一个配置生效。后续调用会失败并在
日志输出 `error`，但不会 panic。

## 任务池配置

异步任务池（`zlim_task` 的 `MainTaskPool`）同样是**内置配置而非插件**。

与 `LogPlugin` 不同，任务池配置若未显式提供，则使用**默认配置**（自动设定合适的线程数）：

```rust, ignore
app.with_task_pool_configs(TaskPoolConfigs::default());
```

任务池是进程级共享的；多个 App 配置时只有第一个生效。

## MainSchedulePlugin

定义了一套**预制的主世界执行逻辑**，是 zlim 引擎其他插件通常所依赖的基础插件：

- `Main` — 每帧执行 First → PreUpdate → RunFixedMainLoop → Update → SpawnScene →
  PostUpdate → Last（顺序由 `MainScheduleOrder` 资源决定）；首帧还会先执行启动
  调度 PreStartup → Startup → PostStartup。
  
- `FixedMain` — 执行 FixedFirst → FixedPreUpdate → FixedUpdate → FixedPostUpdate →
  FixedLast。

- `RunFixedMainLoop` — 消费累积的固定步长时间（`World::step_fixed`），每步运行一次
  `FixedMain`（每帧最多 50 步，避免饿死主循环）。

- 内置维护：`Last` 中的 `OptimizeDelayedCommands`（帧末应用延迟命令）、
  `FixedPostUpdate` 中的 `UpdateMessagesSignal`（双缓冲消息队列每固定步旋转一次）。

`App::new()` / `App::default()` 创建的应用自动添加此插件，并把主世界的默认调度设为 `Main`。

## 其他内置插件

- `ScheduleRunnerPlugin` — 按给定 `RunMode`（`Loop`/`Once` 等）驱动 `App` 的
  runner。适用于非图形应用；请勿与其它同类插件同时使用（只有一个生效）。

- `ShutdownPlugin` — 优雅退出：首次 `Ctrl+C`/`gracefully_exit()` 发送 `AppExit`
  干净退出（`on_exit` 注册的处理器执行一次），再次调用则强制 `std::process::exit`。
  此插件依赖全局变量，不适合常规测试等多 App 并发环境。

- `PanicHandlerPlugin` — 为 `App` 设置合理的 panic hook（wasm 上输出到浏览器控制台）。
