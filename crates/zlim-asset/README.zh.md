# zlim-asset

资产系统，定义了资产加载、缓存、处理等内容的基础架构。

## 资产

资产类型通过 `Asset` Trait 定义，通过 `VisitAssetDependencies` Trait 收集依赖项。

```rust, no_run
use zlim_asset::prelude::Asset;
use zlim_path::TypePath;

#[derive(Asset, TypePath)]
pub struct MyAsset { /* ... */ }
```

资产路径用于加载、保存和处理资产，由 `AssetPath` 类型定义。


```rust, no_run
use zlim_asset::prelude::AssetPath;

let _ = AssetPath::parse("images/example.png");
let _ = AssetPath::parse("https://example.png");
let _ = AssetPath::parse("embeded://config.obj#mesh");
```

资产通过三段式的路径进行标记：`source://` + `path` + `#label` ，其中 source 段和 label 段
是可选的。未指定时应当为空（包括段落符号），比如 `path#` 是非法路径，应当改为 `path` 。

## 资产源

`source://` 用于指定资产源，未指定时为默认源，这在 wasm 中是网络请求，在其他平台为文件系统。

每个源都可以设置 `reader` 、`writer` 、`watcher` 三个对象，用于读写文件和观察变更事件（以
进行热更新）。其中 `reader` 是必须的，后二者可选。

`AssetSourceBuilders` 是一个资源（zlim-core 的 Resource），位于主世界。如果想要添加自定义
源，则必须在 `AssetPlugin` 应用**之前**往 `builders` 资源中插入自定义的 `builder` ，而资产
插件在应用时会通过它们构建出所有资产服务的资产源。必须在资产插件之前插入，因为构建完成后源不可变。

如果 `AssetSourceBuilders` 不存在，资产插件会使用默认的 `builders` 。如果 `builders` 中
没有提供默认源的构造器，则会使用平台特定的默认源构造器：

- wasm 中使用网络请求
- android 中使用应用自身的资源文件夹
- 其他平台使用项目文件夹中的 `assets` 文件夹

除了默认源外，资产插件还会提供一个 `embedded` 源，这通常用于处理 `include_bytes!` 等宏
实现的内联资产。详情请看本库 `io::embedded` 模块的文档。  

## 路径与标记

`path` 段通常就是常规的路径。例如在文件系统源中，这就是文件的相等路径，例如 `image/example.png`
在常规默认源中会被映射为 `assets/image/example.png` 。

`#label` 用于标记子资产，这被用于那些单文件会产出多资产的对象。资产读取时取决于源和路径，
读取一个未加载的子资产通常需要加载完整文件。（当然，加载完整文件时通常会协同加载子资产。）

子资产和依赖资产请查看后面的 `加载器` 章节，或代码的相关实现。

## 资产存储

资产通过 `Assets<A>` 存储在主世界中，外部通常通过 `Handle<A>` 持有资产的引用并保持其生命周期。

`Handle` 可以分为 `Uuid` handle 和 `Strong` handle。

`Uuid` 句柄通常由用户自行指定（通过本库的 `uuid_handle` 宏）。这些资产通常没有资源路径，不从
源加载，而是由用户在程序启动时自行插入。同时，它不由本库的资产服务管理，生命周期完全由用户控制。

`Strong` 句柄则是常规的运行时资产，它们通常从资产路径中加载，也就是上面介绍的方式。这些资产将
由本库的资产服务进行管理，由 `Handle` 实现引用计数，引用为空时自动释放。

## 资产服务

`AssetServer` 是一个资源（zlim_core::Resource），位于主世界，用于加载和保持资产，维护资源源
和运行时资产信息等内容。

```rust, ignore
fn system(server: Res<AssetServer>) {
    let handle: Handle<Image> = server.load("images/example.png");
    // do something
    server.save("images/copied.png", handle);
}
```

`AssetServer` 中有四个重点内容：资产源列表，资产加载器列表，资产保存器列表，资产信息列表。

资产源已经在上面说明，此处不再赘述。

资产加载器用于控制如何从源中加载资产，详情请看本库的 `loader` 模块。当你通过资产服务发起一个加载
命令时，他会立即返回一个 `Handle` ，然后再后台发起一个异步任务，根据输入参数选择合适的加载器并
加载资产，然后将其插入到主世界的 `Assets` 中，并在资产服务中维护相关信息。

加载器的选择通常根据以下参数决定：

- 加载器名（如果提供）
- 资产类型（如果已知）
- 资产元数据（如果存在）
- 资产路径的扩展名

找不到合适的加载器则会加载失败；参数过于宽泛导致存在多个可用加载器，则警告并使用最后注册的加载器。

资源数据通常是与前缀与资产路径相同的 `.meta` 文件，内部可用指定此资产需要使用哪个加载器。

资产保存器用于控制如何将资产保存到源，详情请看本库的 `saver` 模块。保存同样是异步的，因为资产
存储在世界的 `Assets` 中，而资产服务本身不存储资产数据（仅存储资产信息）。保存命令会入队，并
通过一个每帧调用的 Job，让 World 处理这些保存命令。

用户需要输入一个资产路径和一个句柄，此时资产类型保证已知。选择方式和加载器类似：：

- 资产类型（已知）
- 保持器名（如果提供）
- 资产路径的扩展名

保存器还可以设置一些额外选项，比如是否保存 `.meta` 文件，`.meta` 文件中需要指定什么加载器。

当前的资产保存器仅能直接处理无 `#label` 的简单资产，未来可能完善相关实现。

## 热更新

资产源可以注册 `watcher` ，用于监听资产的变化。本库提供了 `watch` feature，在启用时，
在 windows、linux 和 macos 可以提供默认的资产监听器，监听文件变更。

World 同样通过一遍每帧调用的 Job 处理这些变更事件，并重新加载相关路径对应的资产。

另外，`embedded` 资产也支持热更新，但实现略有不同。
`embedded` 是内存中的虚拟文件系统，常规使用 `embedded_asset!` 添加的数据，默认直接
使用 `&'static [u8]` 存储在虚拟文件系统中。但当热更新启用时，`embedded` 会维护这些
嵌入资源对应的文件映射，当文件发生改变时，或重新读取文件并在运行时通过 `Arc[u8]` 的方
式存储对应的数据，以实现热更新。

## 资产处理

如果你观察 AssetSource 的实现，可以发现每个源都有两种读/写/观察器，普通版本和 `processed` 版本。

资产服务可以选择运行在处理模式还是非处理模式，非处理模式中使用普通的读写器，与上文内容相同。
处理模式中则会使用 `processed` 版本的读写器，它和读写器可能映射到不同的路径。

例如在文件系统版本的默认源中，普通路径对应 `assets` 文件夹，而处理后路径对应 `imported_assets` 文件夹。
选择什么模式，决定了资产服务从各个源的哪个路径中读和观察数据。资产服务的保存器则始终指向普通路径。

资产处理通常需要让资产插件启用 `AssetProcessServer`，它和 `AssetServer` 一样是主世界的资源。

在非处理模式下，只有 `AssetServer`，它直接读写普通路径（比如 `assets` ）的内容，简单且直接。

在处理模式写，则通常由 `AssetProcessServer` 读取普通路径（比如 `assets` ）的内容，然后通过内置
的处理器进行资产处理，并将处理后的资产保存到处理后路径（比如 `imported_assets` 文件夹）。而资产
服务 `AssetServer` 则直接读取 `imported_assets` 文件夹的内容。这也是为什么资产服务始终将文件保存
到普通路径。

并不是所有源都有两种路径，比如 `embedded` 和 `http` 等的两个路径都是一致的。当前，我们通过源是否
存在 `processed_writer` 判断此源在处理模式中使用要进行资产处理。如果不需要进行资产处理，请不要设
置它（否则，当处理前后的读取器指向相同文件夹时，可能会发生严重错误，比如文件全部被删除）。

## 资产插件

本库目前提供了三个插件：`AssetPlugin`，`WebAssetPlugin`，`AssetDiagnosticPlugin` 。

`AssetPlugin` 资产插件，用于初始化资产源、创建资产服务、配置是否处于处理模式等。

```rust, ignore
App::new()
  .add_plugins(AssetPlugin::default());
  .build();
  .init_asset::<MyAsset>()
  .register_asset_loader(MyAssetLoader)
  .run();
```

如果需要自定义资产源，请保持 `AssetSourceBuilder` 在此插件 `apply` 之前被添加，否则会被忽视。
而资产类型的注册，以及加载器/保存器的注册则需要在 `AssetPlugin` 之后添加，它们需要资产服务的存在。

用户通常总是应该使用 `AssetPlugin` 初始化资产系统，因为内部有许多队列依赖资产插件保证相关逻辑。
错误的手动初始化可能导致某些事件队列无人读取，内存占用不断增加。

`WebAssetPlugin` 用于初始化网络资产源。它本身不需要 feature，始终可用，但默认没有任何效果。
需要启用 `http` 或 `https` cargo feature，此时它才会在 `apply` 阶段添加这两个源对应的构造器。
它保持会在资产插件之前 `apply` 。

`AssetDiagnosticPlugin` 用于提供诊断信息，这基于 `zlim_diagnostic` 。目前，我们仅提供了一个
资产任务的计数器，用于记录当前资产服务产生了多少资产任务（保存或加载）。这是非精确的仅用于诊断
的信息，且可能存在溢出环绕的问题（虽然 `usize` 通常不太可能溢出）。严格意义上，它不依赖与资产插件，
因为当资产服务不存在时，相关的诊断 Job 会被跳过，程序安全运行。

## Cargo Features

- `debug` —— 进行很多的数据检查和调试信息输出。

- `watch` —— 启动后在可用平台默认开启资产事件的观测（可以通过 AssetPlugin 的参数覆盖）。
  导入 `notify-debouncer-full` 库并实现基于文件系统的资产监视，当前仅在 Windows、Linux 和
  macos 目标下生效，其他平台（比如 `wasm`）中此 feature 没有任何效果。

- `http` / `https` —— 为 `WebAssetPlugin` 添加 `http` 和 `https` 源的实现，基本支持所有平台。
  不启用此 feature 时 `WebAssetPlugin` 是空操作。

- `web_asset_cache` —— 为 web 资产进行缓存，保存在 `.web-asset-cache` 文件夹。
  这仅在 windows、linux 和 macos 目标下生效。
  注意这是个用于开发阶段的 feature，因为它的缓存策略是非常简单的，仅验证 `url` ，且没有过期策略，
  使用固定的哈希算法以保证缓存文件名稳定，因此绝不应该被用于发布阶段。
