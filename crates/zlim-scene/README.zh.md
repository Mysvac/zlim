# zlim-scene

场景系统，提供编译期的静态场景宏，以及动态的场景资产格式。

`scn!` 描述一个实体，`scn_list!` 描述多个实体；这个 crate 的其余部分负责把描述变成实体——解析成
`ResolvedScene`、把它应用到世界，以及把描述存进资产系统，以便另一个场景在它之上继续描述。

## 静态场景

`scn!` 是一串*条目*，描述**一个**实体：它由哪些组件组成、可以用哪些名字指向它、以及哪些实体属于它之下。
任意规模的场景都这样写——子实体可以嵌套，名字贯穿整份描述：

```rust
use zlim_core::derive::{Component, IntoTemplate};
use zlim_core::entity::EntityId;
use zlim_core::world::World;
use zlim_scene::prelude::*;

#[derive(Component, Clone, Default, Debug, PartialEq)]
struct Scale(f32);

#[derive(Component, Clone, Default, Debug, PartialEq)]
struct Health {
    current: u32,
    max: u32,
}

/// 指向另一个实体的组件。`#[derive(IntoTemplate)]` 给了它一个模板，其中的字段是
/// `EntityTemplate`，也正是 `#Name` 解析出来的东西。
#[derive(Component, Clone, IntoTemplate)]
struct Target {
    to: EntityId,
}

/// 单个场景（根实体唯一，子实体任意）
fn scene() -> impl Scene {
    scn! {
        // 通过 `#` 为这个实体定义一个引用
        #Root

        // 条目描述它的组件
        Health { current: 10, max: 20 }

        // `Children` 描述它之下的实体
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

/// 场景列表（多个实体，用 `--` 分隔，共享一个名称作用域）
fn scene_list() -> impl SceneList {
    scn_list! {
        #Left
        Scale(1.0)
        --
        Scale(2.0)
        Parent(#Left) // 可以通过 Parent 关键字设置层级关系
        // Children [ .. ] // 同样支持 Children
    }
}

let mut world = World::alloc();

world.spawn_scene(scene(), None).unwrap();
world.spawn_scene_list(scene_list(), None).unwrap();
```

名称由声明它的实体绑定，而且要等场景中所有实体都存在之后才绑定——所以子实体可以指回父实体，兄弟实体之间
也可以互相指向，先后顺序无所谓：

```rust
use zlim_core::derive::Component;
use zlim_core::world::World;
use zlim_scene::prelude::*;

#[derive(Component, Clone, Default, Debug, PartialEq)]
struct Health {
    current: u32,
    max: u32,
}

let mut world = World::alloc();

let roots = world
    .spawn_scene_list(
        scn_list! {
            #A
            Children [ #B ]
            --
            #C
            Parent(#A)
        },
        None,
    )
    .unwrap();

// `Parent(#A)` 在所有实体都存在之后才应用，所以 `A` 最终同时拥有 `B` 与 `C`。
assert_eq!(world.entity_owned(roots[0]).children().unwrap().len(), 2);
```

### 条目

场景宏中的一个条目可以是：

| 写法 | 含义 |
|---|---|
| `#Name` | 为这个实体定义别名；它不生成任何组件 |
| `Type` | 确保 `Type` 的规范模板存在（缺失时用 `Default` 创建） |
| `Type { field: value, ... }` | 编辑规范模板的命名字段，没写到的字段保持原值 |
| `Type(value, ...)` | 同上，用于元组结构体（字段为 `0`、`1` …） |
| `Type::function(args)` | 用调用的结果替换规范模板 |
| `~expression` | 把表达式本身作为模板存入，例如 `~{ B(6) }` |
| `~template { field: value, ... }` / `~template(args)` | 与上两种写法相同，但路径本身就**是**模板，不经过 `IntoTemplate` 转换 |
| `@ expression` | 在写下的位置引入一个场景，不进行缓存 |
| `: expression` | 通过资产路径引用一个缓存的场景，并在它之上继续描述 |
| `--` | 实体的分隔符，用在 `scn_list!` 或 `Children` 中分隔实体 |
| `Parent(value)` | 指定当前实体的父边，通常写成 `#` 别名 |
| `Children [ entry* (-- entry*)* ]` | 指定当前实体的子实体 |

### 名称引用

可以使用 `#` 定义实体的别名，在需要 `EntityId` 的地方使用别名进行引用：

```rust,ignore
#Root
Sprite { color: RED }
Target { to: #Root } // #Root 解析为 EntityId
```

名称的作用域是单次宏调用：不同的 `scn!`、`scn_list!` 之间互不影响；同一个 `scn!` 被多次调用时，每次调用
也各自指向不同的实体。

引用一个未声明的名称是解析期的错误，例如：

```rust,ignore
scn! { Target { to: #Root } } // 错误：#Root 没有声明
```

但是，仅声明却不使用是可行的：

```rust,ignore
scn! { #Root Health } // #Root 未使用，可行
```

### 默认模板与显式替换

通过 `Type { … }`、`Type(…)`、`Type` 声明的模板是「规范模板」，允许字段缺失：缺失的字段保持规范模板里
已有的值（第一次创建时来自 `Default`）。如果对应的规范模板已经存在，那么就在它上面编辑，不会重新创建。

通过 `Type::function(…)` 与 `~…` 声明的模板则需要给出完整的值，始终替换规范模板，无论它是否已经存在。

`~` 的意思是「这个路径**就是**模板，不是由它构建的类型」；不带 `~` 时，路径会先经过 `IntoTemplate`
转换。对于模板就是类型本身的类型（大多数 `Clone + Default` 的类型），两种写法没有区别，只有手动实现
`IntoTemplate` 的类型才需要区分。

### 层级关系

场景可以通过 `Parent` 和 `Children` 描述层级关系。

对于 `scn!` 宏，根实体唯一，通常使用 `Children` 自上而下地描述场景：

```rust,ignore
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

对于 `scn_list!`，定义的是扁平的实体列表，两种写法都可以用：

```rust,ignore
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

`Parent(value)` 在场景里所有实体都存在之后才应用，因此可以指向稍后才声明的名字。例如，下面的写法保证效果
符合预期：

```rust,ignore
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

```rust,ignore
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

这里的 `Parent` 和 `Children` 冲突了：`#C` 被声明为 `#B` 的子实体，又用 `Parent(#A)` 指定了另一条父边。
解析不会报错，两处都会生效，最终 `#C` 落在 `#A` 之下（显式父边在实体生成之后才应用）。这种写法应当避免。

`Children` 和 `Parent` 都可以写多次：多个 `Children [ … ]` 按书写顺序把子实体接在一起（等价于写成一个
列表、用 `--` 分隔）；多个 `Parent(…)` 则后者胜，只有最后一条生效。

### 缓存场景

`@ expression` 在它写下的位置引入一个场景：表达式本身就是一个 `Scene`，就地解析，不缓存。

引入的场景会和当前实体已有的描述合并，而不是新增实体（两个宏都一样）：它描述的模板、子实体和父边都进入
当前实体。在 `scn_list!` 里想新增实体，用 `--` 分隔即可。

`: expression` 和它类似，但引用的是资产：表达式需要能转换成资产路径，通常就是字面量，例如
`: "scenes/player.scene"`。它依赖资产系统。

`: …` 必须写在所有描述模板或子实体的条目之前（`#` 名称可以写在它前面，因为它两者都不描述），否则是编译
错误。这一限制来自它的语义：缓存场景先被应用，写在它后面的条目是在它贡献的模板上继续编辑。不想受此限制，
就改用 `@`。

## 场景是怎么工作的

### 模板

场景中的一个条目通常称为模板（template），由 `zlim-core` 定义：描述模板的是 `Template` 与
`IntoTemplate` 两个 Trait，而场景接收的是实现了 `Template` 的类型的值。

`zlim-core` 通过伪特化为大多数类型实现了这两个 Trait，并且默认实现中模板往往就是类型本身。

前面提到，`Type { .. }` 这类写法得到的是 `IntoTemplate` 的值，再转换成它的模板。

模板按书写顺序组成部件（part），部件再组成场景。部件按顺序解析，因此写在后面的条目可以编辑
前面的条目留下的模板：

```rust
# use zlim_core::derive::Component;
# use zlim_core::world::World;
# use zlim_scene::prelude::*;
# use zlim_scene::ResolveContext;
# 
#[derive(Component, Clone, Default, Debug, PartialEq)]
struct Health {
    current: u32,
    max: u32,
}

let mut context = ResolveContext::new();
let mut scene = ResolvedScene::new();

// `Health { current: 3 }` 编辑的是规范模板——这里是 `Default` 产生的那个 `Health`——所以 `max`
// 保留它原有的值。
scn! { Health { current: 3 } }
    .resolve(&mut context, &mut scene)
    .unwrap();

scn! { Health { current: 5, max: 9 } }
    .resolve(&mut context, &mut scene)
    .unwrap();

// 无论如何，场景里只有一份模板：`Health` 的规范槽位。
assert_eq!(scene.templates().len(), 1);
```

对于一些特殊的组件，需要使用 `IntoTemplate` 宏显式标记才能用于场景描述
（比如字段中包含 `EntityId` 等特殊类型），请参考 zlim-core 中关于 template 的文档。

### 解析

模板的列表构成了场景，而在场景应用前，还需要解析出正确的场景树。

解析通过 `Scene::resolve` 函数实现，将结构存储在 `ResolvedScene` 类型中。

整个解析过程完全不碰世界，因此一份场景可以「解析一次、应用多次」，从而实现缓存。

### 应用

把一个解析好的场景应用到一个实体，按顺序分四步：

1. 树里的每个实体都被生成：根就是被应用的那个实体，每个子场景生成在描述它的实体之下；

2. 树声明的每个 `#Name` 绑定到声明它的实体；

3. 每个实体拿到它的模板，通过 `BundleWriter` 一次性写入：组件模板压入行里的一列，输出是整个 bundle 的
   模板则把它携带的所有组件都交出来（包括被 require 的组件，由 writer 负责初始化）；

4. 每条显式父边被应用，层级至此确定。

第 1 步与第 3 步的先后顺序正是前向引用能工作的原因：模板解析实体之前，名称已经绑定、实体已经存在。

```rust
# use zlim_core::world::World;
# use zlim_scene::prelude::*;
# use zlim_scene::ResolveContext;
# 
let mut world = World::alloc();

let mut resolved = ResolvedScene::new();
().resolve(&mut ResolveContext::new(), &mut resolved).unwrap();

let entity = resolved.spawn(&mut world, None).unwrap();
assert!(world.get_entity_ref(entity).is_ok());
```

场景只携带数据与结构，因此组件写完之后，不会再执行任何属于场景自身的操作，和 `EntityOwned::insert` 完全
一致：跑的是组件自己的钩子，不是场景的任何效果。

### 层级关系

zlim 把层级放在世界里而不是组件里：`Parent` 与 `Children` 是实体树上的查询数据，树通过「带父实体生成」或
「重新指定父实体」来维护。因此这里没有一个需要插入的关系*组件*、没有需要靠钩子同步的东西、也没有关系钩子
模式要选，所以本 crate 没有 Bevy 的 `Relationship` / `RelationshipTarget` / `RelationshipHookMode` 的
对应物：

- `SceneChildren` 描述场景作为父实体的那些实体。它们带着父实体一起生成，所以之后不需要再补一条边——也不
  需要广播什么，因为「出生就在那个位置」的实体从未在别处。列表的顺序就是 `Children` 的顺序。

- `SceneParent` 描述实体自身的一条显式父边。应用到一个已有实体的场景用的就是它；它在场景所有实体都存在
  之后才应用，因此可以指向同一场景中稍后才声明的名字。带不带它总是对层级的一个答复：没有父边的场景让实体
  留在原地，而父边不指向任何实体的场景把实体移到根。

场景创建的实体从不发生变换传播意义上的*重新指定父实体*：边是在实体构建过程中记下的，因此用
`EntityOwned::reparent_without_signal` 应用——实体是新的，它自己的 tick 已经说明了这一点。只有应用到一
个在世界里已有位置的实体上的场景，才会移动别的部分需要知道的东西，走 `EntityOwned::reparent`。

### 世界扩展

生成一个场景是两步——先解析描述，再应用解析结果——`WorldSceneExt` 是同时做这两步的简写：

| 调用 | 作用 |
|---|---|
| `world.spawn_scene(scene, parent)` | 解析它，并为它生成一个新的根 |
| `world.spawn_scene_list(list, parent)` | 解析一个列表，为每个实体生成一个根 |
| `world.apply_scene(scene, target)` | 解析它，并应用到一个已经存在的实体上 |

也可以把解析结果留下来，用 `ResolvedScene::spawn` 与 `ResolvedScene::apply` 反复应用——`ScenePatch`
就是这么做的，也是场景值得留下来的原因。

这个 trait 的其余入口把同样的两步延后，也就是后面的「场景队列」一节。

### 命令扩展

`Commands` 是延时版本：在系统里写下描述，等调度让命令队列跑起来才构建。

| 调用 | 作用 |
|---|---|
| `commands.spawn_scene(scene, parent)` | 排队一个场景，返回它将描述的实体的 `EntityCommands` |
| `commands.spawn_scene_list(list, parent)` | 排队一个列表；不返回东西，因为列表描述多个实体 |
| `entity_commands.apply_scene(scene)` | 排队一个场景，应用到已经存在的实体上 |

`spawn_scene` 交回的 `EntityCommands` 所代表的实体此时还不存在——但对应的 id 已经分配好了。

```rust
# use zlim_app::App;
# use zlim_asset::plugin::AssetPlugin;
# use zlim_core::derive::Component;
# use zlim_scene::prelude::*;
# use zlim_scene::ScenePlugin;
# 
#[derive(Component, Clone, Default)]
struct Scale(f32);

let mut app = App::new();
app.add_plugins(AssetPlugin::default());
app.add_plugins(ScenePlugin);
app.build();

app.main_world_mut()
    .commands()
    .spawn_scene(scn! { Scale(2.5) });

// 命令队列在调度末尾跑，场景则在那之后的 `SpawnScene` 调度里构建。
app.update();
app.update();
```

两个 trait 都有 `try_` 形式：命令执行时实体已经不存在的情况不会报告——实体是执行时才查的，不是现在查的。

## 缓存一个场景

场景也可以通过资产系统缓存，通过 `ScenePatch` 资产存储解析结果：

```rust
# use zlim_asset::assets::Assets;
# use zlim_core::world::World;
# use zlim_scene::ScenePatch;
# 
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
assert!(world.get_entity_ref(entity).is_ok());
```

`CachedSceneAsset` 是「一个场景建立在另一个之上」的方式：它按路径指名一个补丁，包含它的场景会先应用那个
缓存场景。缓存场景描述的模板可以在 `ResolvedScene::get_or_init_template` 里**接管**：第一次索取时会把
缓存的那份克隆出来，之后应用时缓存的那份就被跳过。这种写时复制，让一个补丁能改「自己并不拥有的场景」的
某个字段，而完全不改动补丁资源本身：

```rust,ignore
scn! {
    : "scenes/player.scene"     // 先应用
    Health { current: 5 }       // `Health` 从它那里克隆而来，所以 `max` 保留
}
```

`@ expression` 是不缓存的对应写法：那个表达式本身就是一个场景，就地解析，既没有补丁也没有写时复制。

解析结果通过 `Arc` 共享，包含缓存场景的场景持有那个 `Arc`，所以应用时不需要任何资源查找；而「还没解析的
补丁」是在**解析期**被拒绝，不是在应用期。

### 依赖

`ScenePatch::load` 会启动「描述所依赖的东西」的加载，`Scene::register_dependencies` 则是描述声明这些
依赖的地方（`: "path"` 会把它指名的补丁登记为依赖）。解析一个补丁时，会先解析它依赖的补丁，深度优先
（因为「加载完了」不等于「解析完了」）；排队生成场景的那个调度也是这么做的。

### 场景列表

`SceneListPatch` 是列表形式：它持有一个 `SceneList`，解析成「每个实体一个 `ResolvedScene`」，然后作为
一个整体生成在同一个父实体之下。

## 插件

场景资产住在资产系统里，所以这个 crate 需要先注册好几样东西：

- `AssetPlugin`（来自 `zlim-asset`）：构建资产源、插入 `AssetServer`、跑加载流水线。它必须**先**加，
  因为注册一种资产类型需要 server。

- `ScenePlugin`（本 crate 的）：通过 `init_asset` 注册 `Assets<ScenePatch>` 与
  `Assets<SceneListPatch>`，注册 `SceneQueue` 资源，并把「构建排队场景」的作业插进 `SpawnScene` 调度。
  它自己声明在 `AssetPlugin` 与 `MainSchedulePlugin` 之后。

```rust
use zlim_app::App;
use zlim_asset::plugin::AssetPlugin;
use zlim_scene::ScenePlugin;

let mut app = App::new();
app.add_plugins(AssetPlugin::default());
app.add_plugins(ScenePlugin);
app.build();
```

### 场景队列

插件加好之后，场景还可以稍后再建：`queue_*` 系列入口把补丁加进资源系统，并在 `SceneQueue` 资源里记下一条
请求。随后的 `SpawnScene` 调度（位于 `Update` 与 `PostUpdate` 之间）会在补丁及其依赖都加载完成之后解析
它、构建它，并取走该请求。

等的是**描述**。`queue_spawn_scene` 会**立刻**把实体建出来并且留空，所以在场景的资源还在加载的这段时间里
就有东西可以指向，组件稍后才到——这正是流式加载关卡所需要的。场景本身此刻被指名、稍后解析；它不返回 id，
因为场景要指名自己的实体走的是 `#Name`。

`queue_spawn_scene_list` 是例外：列表描述的是它自己的一批实体，所以它指名的实体是这些实体的**父**，而不是
「放列表的地方」，列表就绪之前什么都不会建。请求也不是组件——排队的描述可能是关于一个尚不存在的实体的——
所以请求放在资源里，永远不往原型里加表。

```rust
# use zlim_app::App;
# use zlim_asset::plugin::AssetPlugin;
# use zlim_core::derive::Component;
# use zlim_scene::prelude::*;
# use zlim_scene::ScenePlugin;
#
# #[derive(Component, Clone, Default)]
# struct Scale(f32);
#
let mut app = App::new();
app.add_plugins(AssetPlugin::default());
app.add_plugins(ScenePlugin);
app.build();

app.main_world_mut()
    .queue_spawn_scene(scn! { Scale(1.5) }, None)
    .unwrap();

// 场景由同一帧稍后的 `SpawnScene` 调度构建。
app.update();
```

`queue_apply_scene` 是第三种形式：把场景**应用到一个已经存在的实体**上，而不是新建一个。请求所指的实体或
父实体在兑现时已经不存在，会通过 `zlim_log` 报告并丢弃该请求——只有「父实体已消失的列表」是例外，它会改为
作为根生成；而解析不出来的补丁会保留请求，留待下一次调度。
