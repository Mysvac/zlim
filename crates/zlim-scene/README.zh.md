# zlim-scene

场景系统，提供编译期的静态场景宏，以及动态的场景资产格式。

## 静态场景

通过 `scn!` 和 `scn_list!` 直接描述场景：

```rust
# use zlim_core::derive::IntoTemplate;
# use zlim_core::prelude::*;
# use zlim_scene::{WorldSceneExt, Scene, SceneList};
#
# #[derive(Component, Clone, Default, Debug, PartialEq)]
# struct Scale(f32);
#
# #[derive(Component, Clone, Default, Debug, PartialEq)]
# struct Health { current: u32, max: u32 }
#
# #[derive(Component, Clone, IntoTemplate)]
# struct Target { to: EntityId }
/// 生成单个场景（根实体唯一，子实体任意）
fn scene() -> impl Scene {
    zlim_scene::scn! {
        // 通过 `#` 定义一个实体的引用
        #Root

        // 声明实体的组件
        Health { current: 10, max: 20 }

        // 通过 Children 关键字定义子实体
        Children [
            // 子实体 (1)
            #Child
            Health { current: 3, max: 3 }

            // 实体分隔符
            --
            // 子实体 (2)
            Target { to: #Root } // 引用实体 (EntityId)
        ]
    }
}

/// 生成场景列表（多个实体）
fn scene_list() -> impl SceneList {
    zlim_scene::scn_list! {
        #Left
        Scale(1.0)
        -- // 直接通过分隔符划分实体
        Scale(2.0)
        Parent(#Left) // 可以通过 Parent 关键字设置层级关系
        // Children [ .. ] // 同样支持 Children
    }
}

// 生成场景：
let mut world = World::alloc();

world.spawn_scene(scene()).unwrap();
world.spawn_scene_list(scene_list()).unwrap();
```

### 条目

场景宏中的一个条目可以是：

| 写法                         | 含义    |
|------------------------------|----------------|
| `#Name` | 定义别名，不生成任何组件，但可以在场景中作为 `EntityId` 引用 |
| `Type` | 确保 `Type` 的规范模板存在（缺失时用 `Default` 创建） |
| `Type { field: value, ... }` | 编辑规范模板的命名字段，没写到的字段保持原值 |
| `Type(value, ...)` | 同上，用于元组结构体（字段为 `0`、`1` …） |
| `Type::function(args)` | 用调用的结果替换规范模板 |
| `~expression` | 把表达式本身作为模板存入，例如 `~{ B(6) }` |
| `~template { field: value, ... }` / `~template(args)` | 与上两种写法相同，但路径本身就**是**模板，不经过 [`IntoTemplate`] 转换 |
| `@ expression` | 在写下的位置引入一个场景，不进行缓存 |
| `: expression` | 通过资产路径引用一个缓存的场景，并在它之上继续描述 |
| `--` | 实体的分隔符，用在 `scn_list!` 或 `Children` 中分隔实体 |
| `Parent(value)` | 指定当前实体的父边，通常写成 `#` 别名 |
| `Children [ entry* (-- entry*)* ]` | 指定当前实体的子实体 |

### 名称引用

可以使用 `#` 定义实体的别名，在需要 `EntityId` 的地方使用别名进行引用：

```rust, ignore
#Root
Sprite { color: RED }
Target { to: #Root } // #Root 解析为 EntityId
```

名称的作用域是单次宏调用：不同的 `scn!` 、`scn_list!` 之间互不影响；同一个 `scn!` 被多次调用时，每次调用也各自指向不同的实体。

引用一个未声明的名称是解析期的错误，例如：

```rust, ignore
scn! { Target { to: #Root } } // 错误：#Root 没有声明
```

但是，仅声明却不使用是可行的：

```rust, ignore
scn! { #Root Health } // #Root 未使用，可行
```

### 默认模板与显式替换

通过 `Type { … }`、`Type(…)`、`Type` 声明的模板是「规范模板」，允许字段缺失：缺失的字段保持规范模板里已有的值（第一次创建时来自 `Default`）。
如果对应的规范模板已经存在，那么就在它上面编辑，不会重新创建。

通过 `Type::function(…)` 与 `~…` 声明的模板则需要给出完整的值，始终替换规范模板，无论它是否已经存在。

`~` 的意思是「这个路径**就是**模板，不是由它构建的类型」；不带 `~` 时，路径会先经过 [`IntoTemplate`] 转换。

对于模板就是类型本身的类型（大多数 `Clone + Default` 的类型），两种写法没有区别，只有手动实现 `IntoTemplate` 的类型才需要区分。

### 层级关系

场景可以通过 `Parent` 和 `Children` 描述层级关系。

对于 `scn!` 宏，根实体唯一，通常使用 `Children` 自上而下地描述场景：

```rust, ignore
scn! {
    #A
    Children [
        #B
        --
        #C
        Children [ #D ]
    ]
}
```

对于 `scn_list!`，定义的是扁平的实体列表，此时可以选择使用 `Parent` 指定父边，但同样可以选择 `Children` 向下描述：

```rust, ignore
scn_list! {
    #A
    Children [ #B ]
    --
    #C
    --
    #D
    Parent(#C)
}
```

`Parent(value)` 在场景里所有实体都存在之后才应用，因此可以指向稍后才声明的名字。

例如，下面的写法保证效果符合预期：

```rust, ignore
scn_list! {
    #A
    Children [ #B ]
    --
    #C
    Parent(#A)
}
```

实体 `A` 将具有 `B` 和 `C` 两个子实体。

但是，下面的写法是自相矛盾的：

```rust, ignore
scn_list! {
    #A
    --
    #B
    Children [
        #C // ❌️
        Parent(#A)
    ]
}
```

这里的 `Parent` 和 `Children` 冲突了：`#C` 被声明为 `#B` 的子实体，又用 `Parent(#A)` 指定了另一条父边。解析不会报错，两处都会生效，最终 `#C` 落在 `#A` 之下（显式父边在实体生成之后才应用）。这种写法应当避免。

`Children` 和 `Parent` 都可以写多次：多个 `Children [ … ]` 按书写顺序把子实体接在一起（等价于写成一个列表、用 `--` 分隔）；多个 `Parent(…)` 则后者胜，只有最后一条生效。

### 缓存场景

`@ expression` 在它写下的位置引入一个场景：表达式本身就是一个 `Scene`，就地解析，不缓存。

引入的场景会和当前实体已有的描述合并，而不是新增实体（两个宏都一样）：它描述的模板、子实体和父边都进入当前实体。在 `scn_list!` 里想新增实体，用 `--` 分隔即可。

`: expression` 和它类似，但引用的是资产：表达式需要能转换成资产路径，通常就是字面量，例如 `: "scenes/player.scene"` 。它依赖资产系统。

`: …` 必须写在所有描述模板或子实体的条目之前（`#` 名称可以写在它前面，因为它两者都不描述），否则是编译错误。这一限制来自它的语义：缓存场景先被应用，写在它后面的条目是在它贡献的模板上继续编辑。不想受此限制，就改用 `@` 。

### 数量的限制

场景最终是一个元组，而元组最多只有 12 个元素，所以一份描述最多只能有 12 个**部件**（部件见[场景是怎么工作的](#场景是怎么工作的)）。

把条目分成「语句」和「场景」两类：连续的语句合并成 1 个部件，连续的场景也合并成 1 个部件；而嵌在 `Children [ ]` 里的实体各自只算 1 个部件，不管它内部写了多少条目。只要每次合并后都不超过 12 个即可：

```rust, ignore
scn! {
    #A          // 与 Text 同属部件 1
    Text        // 部件 1（连续的语句）
    Children [  // 部件 2（连续的场景）
        #B      // 子实体 1，内部无论多少条目都只算 1 个
        --
        #C      // 子实体 2
    ]
    Health      // 部件 3（语句）
}
```

`scn_list!` 的限制同理，只不过它的部件就是实体：一个实体算一个部件，同样不能超过 12 个。

超出限制是编译期错误，`rust_analyzer` 在写代码时就会报错，通常不需要特别关注（没报错就是正常的）。

## 场景是怎么工作的

### 模板

场景中的一个条目通常称为模板（template），由 `zlim-core` 定义：
描述模板的是 `Template` 与 `IntoTemplate` 两个 Trait，而场景接收的是实现了 `Template` 的类型的值。

`zlim-core` 通过伪特化为大多数类型实现了这两个 Trait，并且默认实现中模板往往就是类型本身。

前面提到，`Type { .. }` 这类写法得到的是 `IntoTemplate` 的值，再通过 `from_template` 转换成 `Template`。

模板按书写顺序组成部件（part），部件再组成场景。部件按顺序解析，因此写在后面的条目可以编辑前面的条目留下的模板。

### 解析

模板的列表构成了场景，而在场景应用前，还需要解析出正确的场景树。

解析通过 [`Scene::resolve`] 函数实现，将结构存储在 [`ResolvedScene`] 类型中。

整个解析过程完全不碰世界，因此一份场景可以「解析一次、应用多次」，从而实现缓存。

### 应用

把一个解析好的场景应用到一个实体，按顺序分四步：

1. 树里的每个实体都被生成：根就是被应用的那个实体，每个子场景生成在描述它的实体之下；

2. 树声明的每个 `#Name` 绑定到声明它的实体；

3. 每个实体拿到它的模板，通过 `BundleWriter` 一次性写入：
   组件模板压入行里的一列，输出是整个 bundle 的模板则把它携带的所有组件都交出来（包括被 require 的组件，由 writer 负责初始化）；

4. 每条显式父边被应用，层级至此确定。

第 1 步与第 3 步的先后顺序正是前向引用能工作的原因：模板解析实体之前，名称已经绑定、实体已经存在。

```rust
use zlim_core::world::World;
use zlim_scene::{ResolveContext, ResolvedScene, Scene};

let mut world = World::alloc();

let mut resolved = ResolvedScene::new();
().resolve(&mut ResolveContext::new(), &mut resolved).unwrap();

let entity = resolved.spawn(&mut world, None).unwrap();
assert!(entity.is_spawned());
```

场景只携带数据与结构，因此组件写完之后，不会再执行任何属于场景自身的操作，和 `EntityOwned::insert` 完全一致。

## 缓存一个场景

场景可以通过资产系统进行缓存，通过 [`ScenePatch`] 资源存储解析结果。

```rust
use zlim_asset::assets::Assets;
use zlim_core::world::World;
use zlim_scene::ScenePatch;

let mut patches = Assets::<ScenePatch>::default();

// 补丁解析进它自己的集合……
let mut patch = ScenePatch::new(());
patch.resolve(None, &mut patches).unwrap();
let handle = patches.add(patch);

// ……解析结果之后可以应用任意多次。
let mut world = World::alloc();
let entity = patches
    .get(&handle)
    .unwrap()
    .spawn(&mut world, None)
    .unwrap();
assert!(entity.is_spawned());
```

`CachedSceneAsset` 是「一个场景建立在另一个之上」的方式：它按路径指名一个补丁，包含它的场景会先应用那个缓存场景。缓存场景描述的模板可以在 [`ResolvedScene::get_or_insert_template`] 里**接管**：第一次索取时会把缓存的那份克隆出来，之后应用时缓存的那份就被跳过。这种写时复制，让一个补丁能改「自己并不拥有的场景」的某个字段，而完全不改动补丁资源本身：

```rust,ignore
scn! {
    : "scenes/player.scene"     // 先应用
    Health { current: 5 }       // `Health` 从它那里克隆而来，所以 `max` 保留
}
```

`@ expression` 是不缓存的对应写法：那个表达式本身就是一个场景，就地解析，既没有补丁也没有写时复制。

解析结果通过 `Arc` 共享，包含缓存场景的场景持有那个 `Arc`，所以应用时不需要任何资源查找；而「还没解析的补丁」是在**解析期**被拒绝，不是在应用期。

### 依赖

`ScenePatch::load` 会启动「描述所依赖的东西」的加载，[`Scene::register_dependencies`] 则是描述声明这些依赖的地方（`: "path"` 会把它指名的补丁登记为依赖）。解析一个补丁时，会先解析它依赖的补丁，深度优先（因为「加载完了」不等于「解析完了」）；排队生成场景的那个调度也是这么做的。

### 场景列表

[`SceneListPatch`] 是列表形式：它持有一个 `SceneList`，解析成「每个实体一个 `ResolvedScene`」，然后作为一个整体生成在同一个父实体之下。

## 插件

场景资产住在资产系统里，所以这个 crate 需要先注册好几样东西：

- [`AssetPlugin`]（来自 `zlim-asset`）：构建资产源、插入 `AssetServer`、跑加载流水线。它必须**先**加，因为注册一种资产类型需要 server。
- [`ScenePlugin`]（本 crate 的）：通过 `init_asset` 注册 `Assets<ScenePatch>` 与 `Assets<SceneListPatch>`，并把「构建排队场景」的作业插进 `SpawnScene` 调度。它自己声明在 `AssetPlugin` 与 `MainSchedulePlugin` 之后。

```rust
use zlim_app::App;
use zlim_asset::plugin::AssetPlugin;
use zlim_scene::ScenePlugin;

let mut app = App::new();
app.add_plugins(AssetPlugin {
    // 示例从不读文件系统，而监视会一直留着一个线程。
    watch_for_changes_override: Some(false),
    ..AssetPlugin::default()
});
app.add_plugins(ScenePlugin);
app.build();
```

### 排队构建场景

插件加好之后，场景还可以稍后再建：[`WorldSceneQueueExt`] 把补丁加进资源系统，并在场景所属的实体上放一个 [`ScenePatchInstance`]。随后的 `SpawnScene` 调度（位于 `Update` 与 `PostUpdate` 之间）会在补丁及其依赖都加载完成之后解析它、应用它，并取走该请求。这正是流式加载关卡所需要的：实体在排队的那一刻就存在，可以被指向，组件则随资源一起到达。

```rust
# use zlim_app::App;
# use zlim_asset::plugin::AssetPlugin;
# use zlim_core::derive::Component;
# use zlim_scene::{ScenePlugin, WorldSceneQueueExt, scn};
# #[derive(Component, Clone, Default)]
# struct Scale(f32);
let mut app = App::new();
app.add_plugins(AssetPlugin {
    watch_for_changes_override: Some(false),
    ..AssetPlugin::default()
});
app.add_plugins(ScenePlugin);
app.build();

let entity = app
    .main_world_mut()
    .queue_spawn_scene(scn! { Scale(1.5) }, None)
    .unwrap();

// 场景由同一帧稍后的 `SpawnScene` 调度构建。
app.update();
```

[`SceneListPatchInstance`] 是列表形式；而在已有实体上的 [`ScenePatchInstance`] 是把场景**应用到这个实体**，而不是新建一个。实体已经不存在的排队请求会被丢弃；解析不出来的补丁会保留请求，并通过 `zlim_log` 报告失败。

## 源自 Bevy

设计参考 `bevy_scene`，并按 zlim 内置层级以及 zlim 的 `Template`、`BundleWriter`、任务 API 做了调整。感谢 Bevy 作者们。

[`AssetPlugin`]: zlim_asset::plugin::AssetPlugin
[`EntityScene`]: crate::EntityScene
[`IntoTemplate`]: zlim_core::template::IntoTemplate
[`InitTemplate`]: crate::InitTemplate
[`InsertTemplate`]: crate::InsertTemplate
[`PatchIntoTemplate`]: crate::PatchIntoTemplate
[`PatchTemplate`]: crate::PatchTemplate
[`ResolveContext`]: crate::ResolveContext
[`ResolvedScene::apply`]: crate::ResolvedScene::apply
[`ResolvedScene::get_or_insert_template`]: crate::ResolvedScene::get_or_insert_template
[`ResolvedScene::spawn`]: crate::ResolvedScene::spawn
[`ResolvedScene`]: crate::ResolvedScene
[`Scene::register_dependencies`]: crate::Scene::register_dependencies
[`Scene::resolve`]: crate::Scene::resolve
[`SceneChildren`]: crate::SceneChildren
[`SceneFunction`]: crate::SceneFunction
[`SceneListPatchInstance`]: crate::SceneListPatchInstance
[`SceneListPatch`]: crate::SceneListPatch
[`SceneListScope`]: crate::SceneListScope
[`SceneList`]: crate::SceneList
[`SceneParent`]: crate::SceneParent
[`ScenePatchInstance`]: crate::ScenePatchInstance
[`ScenePatch`]: crate::ScenePatch
[`ScenePlugin`]: crate::ScenePlugin
[`SceneScope`]: crate::SceneScope
[`Scene`]: crate::Scene
[`TemplatePatch`]: crate::TemplatePatch
[`Template`]: zlim_core::template::Template
[`WorldSceneExt`]: crate::WorldSceneExt
[`WorldSceneQueueExt`]: crate::WorldSceneQueueExt
