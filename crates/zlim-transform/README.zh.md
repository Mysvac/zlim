# zlim-transform

基于 `zlim-core` ECS 的变换 Transform 系统：

维护实体层级中 `Transform`（局部变换）与 `GlobalTransform`（世界变换）的一致性，并支持并行传播。

## 对外接口

| 类型 | 说明 |
|---|---|
| `Transform` | 局部变换（平移 / 旋转 / 缩放）。`require(GlobalTransform)`。 |
| `GlobalTransform` | 世界变换。**派生输出**，只由传播系统写入。 |
| `TransformPropagateStrategy` | 检测策略：`Default` / `PropagateUp` / `PropagateAll`。 |
| `TransformPlugin` | 插件：把两个系统注册到 `PostStartup` / `PostUpdate`。 |
| `EntityTransformExt` / `EntityCommandsTransformExt` | `reparent_in_place` 扩展。 |

## 示例

```rust
use zlim_app::{App, MainSchedulePlugin};
use zlim_transform::{GlobalTransform, Transform, TransformPlugin};

let mut app = App::new();
app.add_plugins(TransformPlugin::default());
app.build(); // 执行插件（build → apply → cleanup）

// 构造层级：root -> child
let world = app.main_world_mut();
let mut root = world.spawn(Transform::from_xyz(1.0, 0.0, 0.0), None);
root.with_child(Transform::from_xyz(0.0, 2.0, 0.0)).unwrap();
let child_id = root.children().unwrap()[0];
let _ = root;

// 每帧调用一次：`PostUpdate` 中运行
// `TransformChangeDetection` → `TransformPropagation`
app.update();

// child 的世界变换 = root 的世界变换 × child 的局部变换
let world = app.main_world_mut();
assert_eq!(
    world.entity(child_id).get::<GlobalTransform>().unwrap(),
    &GlobalTransform::from(Transform::from_xyz(1.0, 2.0, 0.0)),
);
```

## 算法实现

变换更新分为两个阶段，两个系统按顺序执行（`TransformChangeDetection` → `TransformPropagation`）。

### 阶段一：变更检测（`TransformChangeDetection`）

目标：找出所有"变更子树的根节点"（自身需要更新、但父不需要更新的实体），写入 `TransformChangeRoot` 资源。

#### 默认策略（`Default`）：

1. **处理 ReparentSignal 消息**：读取本帧的 `ReparentSignal` 消息（由 `zlim-core` 的 reparent
   操作发送）。对每个实体计算"父全局 × 自身局部"并与当前 `GlobalTransform` 比较，
   不同将 `Transform` 标记为 changed（浮点比较可能误判，影响很小）。

2. **收集 changed 实体**：顺序遍历查询（`Query::iter_slice_mut`），把每个
   `Transform.is_changed()` 的实体作为"根候选"（连同其父）写入临时缓冲，同时把它
   的直接子节点收集到待处理队列。此步是 **O(N) 顺序访问**，缓存命中率极佳。

3. **向下污染标记**：从待处理队列弹出节点，未标记则 `set_changed()` 并继续把它的
   子节点入队；**已标记则跳过**（它的子树由它自己的遍历负责，避免重复）。此步的最坏
   情况为 O(N) 随机访问；无变化时无操作。

4. **筛选真正的根**：过滤候选集 `(id, parent)`，上级实体无 `Transform` 组件或未
   改变时，才真正属于变更子树的根节点。

#### 完全传播（`PropagateAll`）：

完全跳过默认策略的 1-4 步，直接收集所有"根"（无上级实体、或上级实体无 `Transform` 组件）。

此时阶段二总是 O(N) 随机访问（全树传播）。适合高度动态的场景。

#### 向上传播（`PropagateUp`）：

用**向上查询**替代向下标记（跳过默认策略的 2-4 步）。

对每个 changed 节点沿祖先链向上遍历：若任一祖先也 changed，则该节点属于那个祖先的子树、不是根；
祖先链上无 changed 节点（或链断）的节点才是变更子树的根。

每个 changed 节点只需一次 O(深度) 的父链查询，而不用标记整棵子树，适合变化稀疏、层级较浅的场景。
父链查询在遇到 changed 节点是可以立即停止，因此场景的完全变更也能将复杂度控制在 O(N) 。

但此模式对于层级较深的树，在极端情况下可能出现恶化。例如大量深层节点进行父链查询，又应独立变更而
无法在查询时提前截断，可能退化成接近 O(N*深度) 的复杂度。

### 阶段二：传播（`TransformPropagation`）

遍历节点一收集的根列表，对于每个根执行 `this.GlobalTransform = super.GlobalTransform * this.Transform` 
（无上级实体则为 `IDENTITY × this.Transform`）。

然后递归对所有子实体执行相同操作。

## 并行性

是否并行取决于依赖库 `zlim_task` 的情况。如果 `zlim_task` 启用了多线程，
则本库同步开始多线程的支持。

多线程模式中，除了阶段 1-1 的“处理 ReparentSignal 消息”，其他步骤均可并行。

默认使用 `zlim_task` 的 MainTaskPool ，内部进行了许多调度上的优化。
