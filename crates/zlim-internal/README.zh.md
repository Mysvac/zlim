# zlim-internal

zlim 引擎的核心实现 crate。

作为中央 feature 管理枢纽——公共的 `zlim` facade 将其 feature 转发到此 crate，
再由它启用所有引擎子系统的对应行为。

## 内部 crate

- **`cfg`** —— 重新导出 `zlim_cfg`：编译期控制宏（`enabled!`、`disabled!`、`switch!`、`define_alias!`）。
- **`ptr`** —— 重新导出 `zlim_ptr`：类型擦除的指针抽象（`Ptr`、`PtrMut`、`OwningPtr`、`Slice`、`SliceMut`）。
- **`reg`** —— 重新导出 `zlim_reg`：基于 CTOR 的元数据收集器（`collect!`、`submit!`、`iter`）。
- **`log`** —— 重新导出 `zlim_log`：日志库以及基于日志的性能分析工具。
- **`os`** —— 重新导出 `zlim_os`：平台抽象层（标准目录、时间、系统 crate 重新导出）。
- **`utils`** —— 重新导出 `zlim_utils`：基础工具（哈希容器、同步原语、内存池、集合）。
- **`reflect`** —— 重新导出 `zlim_reflect`：运行时反射（`Reflect`、`TypePath`、`TypeDB`、动态类型）。
- **`task`** —— 重新导出 `zlim_task`：异步任务池（工作窃取）（`TaskPool`、`Scope`、`block_on`、`Task`）。
- **`core`** —— 重新导出 `zlim_core`：ECS 核心（Entity、Component、World、Schedule、Tick 等）。
- **`app`** —— 重新导出 `zlim_app`：App / 插件框架（`App`、`Plugin`、主调度）。
- **`math`** —— 重新导出 `zlim_math`：基于 `glam` 的数学库（向量、矩阵、运算）。
- **`shape`** —— 重新导出 `zlim_shape`：几何图元（2D/3D）、射线、包围体。
- **`curve`** —— 重新导出 `zlim_curve`：`Curve` trait、曲线类型与适配器。
- **`color`** —— 重新导出 `zlim_color`：颜色空间、转换与调色板。
- **`transform`** —— 重新导出 `zlim_transform`：`Transform`/`GlobalTransform` + 层级传播。
- **`diagnostic`** —— 重新导出 `zlim_diagnostic`：诊断存储与内置测量插件。
- **`sysinfo`** —— 重新导出 `zlim_sysinfo`：主机系统信息插件（在启用 `zlim_sysinfo` feature 时可用）。
- **`derive`** —— 重新导出各个库的派生宏。
