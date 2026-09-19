# zlim-tracy

Bindings for the Tracy client, ported from [`tracy-client`](https://crates.io/crates/tracy-client)
with a few adjustments.

The crate provides the `tracy` feature, which is what activates the client.

With the feature disabled, every function and type of the crate falls back to a cheap no-op mode.
This means the crate can be imported, and instrumentation can be written, without a crowd of
`#[cfg(feature = ...)]` statements; with `tracy` disabled, most of what the crate does is removed by
the compiler.

With the feature enabled, `tracy-client` starts up before `main` and runs until the program ends.
Unlike `tracy-client` itself, this crate offers no manual control over the lifetime of the client:
if you do not want it, turn the feature off.

## Quick start

```rust
use zlim_tracy::{Client, PlotConfiguration, plot_name, span, span_builder};

fn work() {
    // A span is reported, together with the time it took, when it is dropped.
    {
        let _span = span!("work"); // A span at runtime, created by the macro.
        Client::message("starting", 0);
    }

    // An explicit construction, for the spans that need more parameters.
    let _span = span_builder!()
        .with_name("loading")
        .with_color(0x00FF00)
        .build();

    // A plot is configured once and then fed with values.
    Client::plot_config(plot_name!("memory"), PlotConfiguration::default());
    Client::plot(plot_name!("memory"), 42.0);

    // Frame marks cut the timeline of the profiler into frames.
    Client::frame_mark();
}

fn main() {
    work();
}
```

## span

| Item | Purpose |
|------|---------|
| [`Span`] | A region of execution, reported when it is dropped. |
| [`Span::new`] / [`span!`] | Reports a span described at the call site. |
| [`SpanBuilder`] / [`span_builder!`] | Reports a span assembled piece by piece. |
| [`SpanSource`] | A static source of spans, which builds spans over and over without allocating again. |

A span also carries values, texts and colors:
[`Span::emit_value`], [`Span::emit_text`], [`Span::emit_color`].
Collecting a callstack for a span or a message costs noticeably more than reporting it, so it only
happens where a depth is given.

## message

[`Client::message`] reports a message and [`Client::color_message`] reports one with a color, both
taking a callstack depth. [`Client::is_running`] says whether the profiler is compiled in, and
[`Client::is_connected`] whether a profiler application is attached.

## plot

[`PlotName`] names a plot: it can be created from a static `CStr` with [`PlotName::new`], from a
runtime string with [`PlotName::new_leak`], or at compile time with the [`plot_name!`] macro.

[`Client::plot`] adds a point to the plot, and [`Client::plot_config`] describes how the profiler UI
displays it, which is [`PlotConfiguration`] over [`PlotFormat`] and [`PlotLineStyle`].

## frame

[`Client::frame_mark`] marks the end of a continuous frame. [`Client::secondary_frame_mark`] does
the same for a named frame, and [`Client::non_continuous_frame`] returns a [`Frame`] whose drop ends
a frame that does not repeat.

[`Client::frame_image`] attaches an image of a frame. Names come from the [`frame_name!`] macro,
[`FrameName::new`] or [`FrameName::new_leak`].

## GPU span

[`GpuContext::new`] creates a context for a GPU queue out of the API it belongs to
([`GpuContextType`]), the GPU timestamp that corresponds to the call, and the period of the GPU
clock.

A span of GPU work is started with [`GpuSpan::new`] or [`GpuSpanBuilder`] right next to where the
GPU timestamp of the beginning of the work is recorded, ended with [`GpuSpan::end_zone`], and its
timestamps are uploaded once the GPU has written them:

```rust
use zlim_tracy::{GpuContext, GpuContextType, gpu_span_builder};

let context = GpuContext::new(None, GpuContextType::Vulkan, 0, 1.0).unwrap();
let mut span = gpu_span_builder!()
    .with_name("my_work")
    .build(&context)
    .unwrap();

// The GPU timestamps around the work are recorded here, and uploaded once they are readable.
span.end_zone();
span.upload_timestamp_start(0);
span.upload_timestamp_end(1);
```

A caller that manages the GPU timestamp queries itself uses [`SpanSource::begin_gpu`], together with
[`GpuContext::end_span`] and [`GpuContext::upload_gpu_timestamp`].

## Reusing source information

The constructors of `Span` and `GpuSpan` allocate memory at runtime, which holds the description of
the span until the client reads it.

To optimize further, the `SpanSource` can be created up front, and spans can be built with its
`begin`; no further allocation is needed then.

```rust
use zlim_tracy::SpanSource;

static WORK: SpanSource = SpanSource::new(c"work", c"my_crate::work", c"src/lib.rs", 42, 0);

fn work() {
    let _span = WORK.begin();
}
```

This requires the `SpanSource` to be `&'static`. One that is created at runtime can be kept resident
for the rest of the program with `SpanSource::leak`, which copies it into the global memory pool.
Note that `leak` does not deduplicate, so the leaked `SpanSource` is meant to be reused instead of
being leaked over and over, which would keep growing the memory it uses.

## Cargo features

- `tracy`: links the client library, which provides the actual implementation.
- `tracy_memory`: uses the global allocator of the client, to monitor memory.
- `tracy_demangle`: demangles Rust symbols, so that a captured callstack shows better names;
  corresponds to `tracy-client/demangle`.
- `tracy_system`: collects system tracing data; corresponds to `tracy-client/system-tracing`.
- `tracy_broadcast`: broadcasts discovery packets, so that clients of the local network can connect;
  corresponds to `tracy-client/broadcast`.
- `tracy_only_localhost`: limits the client to the local host instead of the whole local network;
  corresponds to `tracy-client/only-localhost`.
- `tracy_context_switch`: collects context switches; corresponds to
  `tracy-client/context-switch-tracing`.
- `tracy_callstack_inlines`: resolves the frames that were inlined into a captured callstack;
  corresponds to `tracy-client/callstack-inlines`.

Every feature other than `tracy` requires `tracy` to be enabled as well, and enabling one without it
is a compile error.

## Note

Depending on its configuration, Tracy may broadcast discovery packets to the local network and
expose the data it collects, which can include machine code and source code, to that network. Enable
the `tracy` feature in development builds only.

---

See Simonas Kazlauskas' [`tracy-client`](https://github.com/nagisa/rust_tracy_client) for more.
