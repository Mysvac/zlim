# zlim-tracy

Tracy-Client 的绑定，移植自 [`tracy-client`](https://crates.io/crates/tracy-client)，并进行了调整。

本库提供了 `tracy` feature，用于激活 tracy-client。

当 feature 未启用时，本库的所有函数、类型都切换到廉价的空操作模式。这意味着你可以直接导入本库并
在代码编写时无需声明大量的 `#[cfg(feature)]` 语句。当 `tracy` 未启用时，大部分本库的操作都可以
被编译器消除。

当 feature 启用时，`tracy-client` 会自动在 `main` 之前运行，直到程序结束。
于 `tracy-client` 库本身不同，本库不提供手动的 client 声明周期管理。如果你
不想启用，直接关闭 `feature` 即可。

## 快速开始

```rust
use zlim_tracy::{Client, PlotConfiguration, plot_name, span, span_builder};

fn work() {
    // span 在被 drop 时上报，同时带上它持续的时间。
    {
        let _span = span!("work"); // 通过宏创建一个运行期的 span
        Client::message("starting", 0);
    }

    // 通过 span_builder 手动构造,以支持更多参数
    let _span = span_builder!()
        .with_name("loading")
        .with_color(0x00FF00)
        .build();

    // plot 配置一次，之后持续送值即可。
    Client::plot_config(plot_name!("memory"), PlotConfiguration::default());
    Client::plot(plot_name!("memory"), 42.0);

    // frame mark 把剖析器的时间轴切成帧。
    Client::frame_mark();
}

fn main() {
    work();
}
```

## span

| 条目 | 用途 |
|------|------|
| [`Span`] | 一段执行区间，在 drop 时上报。 |
| [`Span::new`] / [`span!`] | 上报在调用点描述的 span。 |
| [`SpanBuilder`] / [`span_builder!`] | 上报逐项拼出来的 span。 |
| [`SpanSource`] | 静态的 span 源，可用于反复构建 span，复用时无需额外分配内存 |

span 还可以携带数值、文本与颜色：
[`Span::emit_value`]、[`Span::emit_text`]、[`Span::emit_color`]。
为 span 或 message 采集调用栈的开销明显高于上报本身，因此只有显式给出深度时才会采集。

## message

[`Client::message`] 上报一条消息，[`Client::color_message`] 上报一条带颜色的消息，两者都可带
调用栈深度。[`Client::is_running`] 表示剖析器是否被编译进来，[`Client::is_connected`] 表示是否
有剖析器程序接了上来。

## plot

[`PlotName`] 表示一个 plot 的名字：可以用 [`PlotName::new`] 从静态 `CStr` 创建，用
[`PlotName::new_leak`] 从运行时字符串创建，或者用 [`plot_name!`] 宏在编译期创建。

[`Client::plot`] 往 plot 里加点，[`Client::plot_config`] 描述剖析器 UI 如何显示它，即
[`PlotConfiguration`] 搭配 [`PlotFormat`] 与 [`PlotLineStyle`]。

## frame

[`Client::frame_mark`] 标记一个连续帧的结束。[`Client::secondary_frame_mark`] 用于具名帧，
[`Client::non_continuous_frame`] 返回一个 [`Frame`]，drop 时结束一个不重复的帧。

[`Client::frame_image`] 附带一张帧的图像。名字来自 [`frame_name!`] 宏、[`FrameName::new`] 或
[`FrameName::new_leak`]。

## GPU span

[`GpuContext::new`] 用所属 API（[`GpuContextType`]）、对应当前时刻的 GPU 时间戳、以及 GPU 时钟
周期，为一个 GPU 队列创建上下文。

GPU 工作的 span 用 [`GpuSpan::new`] 或 [`GpuSpanBuilder`] 在记录工作起始 GPU 时间戳的地方开启，
用 [`GpuSpan::end_zone`] 结束，等 GPU 写好时间戳后再上传：

```rust
use zlim_tracy::{GpuContext, GpuContextType, gpu_span_builder};

let context = GpuContext::new(None, GpuContextType::Vulkan, 0, 1.0).unwrap();
let mut span = gpu_span_builder!()
    .with_name("my_work")
    .build(&context)
    .unwrap();

// 在这里记录工作前后的 GPU 时间戳，等可读之后回传。
span.end_zone();
span.upload_timestamp_start(0);
span.upload_timestamp_end(1);
```

如果 GPU 时间戳查询由调用方自己管理，则使用 [`SpanSource::begin_gpu`]，配合
[`GpuContext::end_span`] 与 [`GpuContext::upload_gpu_timestamp`]。

## 复用源信息

Span 和 GpuSpan 本身的创建函数将在运行期分配内存，存储相关信息直到被 tracy 客户端读取。

如果想要进一步优化性能，则可以提前创建 `SpanSource` ，使用它的 `begin` 函数创建 Span，
此时无需再次分配内存。

```rust
use zlim_tracy::SpanSource;

static WORK: SpanSource = SpanSource::new(c"work", c"my_crate::work", c"src/lib.rs", 42, 0);

fn work() {
    let _span = WORK.begin();
}
```

这要求 `SpanSource` 是 `&'static` 的。如果需要运行期创建，则可以使用 `SpanSource::leak` 将他长期
驻留到全局内存池。注意 `leak` 不会去重，因此用户需要复用创建的 `SpanSource` 而非频繁 `leak` （这将
导致内存消耗持续增长）。

## Cargo features

- `tracy` : 导入 `tracy-client`，提供真正的 `tracy` 实现。
- `tracy_memory` : 使用 `tracy` 的全局内存分配器，以监控内存信息。
- `tracy_demangle` : 解析 Rust 符号，以在栈捕获时更好地显示函数名，对应 `tracy-client/demangle` 。
- `tracy_system` : 采集系统跟踪数据，对应 `tracy-client/system-tracing` 。
- `tracy_broadcast` : 广播发现报文，以支持局域网客户端连接，对应 `tracy-client/broadcast` 。
- `tracy_only_localhost` : 把客户端限制在本机，而不是整个局域网，对应 `tracy-client/only-localhost` 。
- `tracy_context_switch` : 采集上下文切换，对应 `tracy-client/context-switch-tracing` 。
- `tracy_callstack_inlines` : 解析调用栈中被内联的帧，对应 `tracy-client/callstack-inlines` 。

`tracy` 之外的 feature 在启用时，必须保证 `tracy` feature 本身已经开启，否则会编译错误。

## Note

视配置而定，Tracy 可能向局域网广播发现报文，并把采集到的数据（可能包含机器码与源码）暴露给该
网络。请只在开发构建中启用 `tracy` feature。

---

更多内容请参考 Simonas Kazlauskas 的 [`tracy-client`](https://github.com/nagisa/rust_tracy_client) 。
