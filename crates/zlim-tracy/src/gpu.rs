use core::fmt::{Display, Formatter};
use core::marker::PhantomData;

#[cfg(feature = "tracy")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "tracy")]
use tracy_client_sys as sys;

use crate::SpanSource;

// -----------------------------------------------------------------------------
// GpuContextType

/// The API a GPU context belongs to.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GpuContextType {
    /// A context of an API the profiler does not know about.
    Invalid = 0,

    /// An OpenGL context.
    OpenGL = 1,

    /// A Vulkan context.
    Vulkan = 2,

    /// An OpenCL context.
    OpenCL = 3,

    /// A Direct3D 12 context.
    Direct3D12 = 4,

    /// A Direct3D 11 context.
    Direct3D11 = 5,
}

// -----------------------------------------------------------------------------
// GpuContext

#[cfg(feature = "tracy")]
static GPU_CONTEXT_INDEX: Mutex<u8> = Mutex::new(0);

/// A context for creating GPU spans.
///
/// A context generally corresponds to a single GPU queue. The flow of timing GPU work is:
///
/// 1. Create a context with [`GpuContext::new`], passing the current GPU timestamp and the period
///    of the GPU clock.
/// 2. Start a span with [`GpuSpan::new`] or with a [`GpuSpanBuilder`] right next to where the GPU
///    timestamp of the beginning of the work is recorded.
/// 3. Call [`GpuSpan::end_zone`] right next to where the GPU timestamp of the end of the work is
///    recorded.
/// 4. Once the timestamps are readable, upload them with [`GpuSpan::upload_timestamp_start`] and
///    [`GpuSpan::upload_timestamp_end`].
///
/// A caller that records the timestamps and owns the timestamp queries itself uses
/// [`SpanSource::begin_gpu`] instead, together with [`GpuContext::end_span`] and
/// [`GpuContext::upload_gpu_timestamp`].
#[derive(Clone)]
pub struct GpuContext {
    _seal: PhantomData<()>,
    #[cfg(feature = "tracy")]
    value: u8,
    #[cfg(feature = "tracy")]
    span_freelist: Arc<Mutex<Vec<u16>>>,
}

impl GpuContext {
    /// Creates a GPU context.
    ///
    /// - `name` is the name of the context, which is what the profiler UI lists it under.
    /// - `gpt_type` is the API the context belongs to.
    /// - `gpu_timestamp` is the GPU timestamp that corresponds, as closely as possible, to this
    ///   call.
    /// - `period` is the period of the GPU clock in nanoseconds, so `1.0` is a 1GHz clock, `1000.0`
    ///   is a 1MHz clock, and so on.
    ///
    /// # Errors
    ///
    /// Fails if more than 255 contexts were created over the lifetime of the program.
    pub fn new(
        name: Option<&str>,
        gpt_type: GpuContextType,
        gpu_timestamp: i64,
        period: f32,
    ) -> Result<Self, GpuContextCreationError> {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = (name, gpt_type, gpu_timestamp, period);
            Ok(Self { _seal: PhantomData })
        }

        #[cfg(feature = "tracy")]
        {
            let index = {
                let mut index = GPU_CONTEXT_INDEX
                    .lock()
                    .expect("the GPU context index is never poisoned");

                if *index == u8::MAX {
                    return Err(GpuContextCreationError::TooManyContextsCreated);
                }

                let value = *index;
                *index += 1;
                value
            };

            unsafe {
                // SAFETY: `index` is handed out exactly once, so it is not reused. The period and
                // the timestamp are plain values, and the flags are zero.
                let () =
                    sys::___tracy_emit_gpu_new_context_serial(sys::___tracy_gpu_new_context_data {
                        gpuTime: gpu_timestamp,
                        period,
                        context: index,
                        flags: 0,
                        type_: gpt_type as u8,
                    });
            }

            if let Some(name) = name {
                unsafe {
                    // SAFETY: the name outlives the call, which copies it into the profiler queue.
                    let () = sys::___tracy_emit_gpu_context_name_serial(
                        sys::___tracy_gpu_context_name_data {
                            context: index,
                            name: name.as_ptr().cast(),
                            len: name.len().try_into().unwrap_or(u16::MAX),
                        },
                    );
                }
            }

            Ok(Self {
                value: index,
                span_freelist: Arc::new(Mutex::new((0..=u16::MAX).collect())),
                _seal: PhantomData,
            })
        }
    }

    /// Takes a pair of query ids that are not waiting for a GPU timestamp anymore.
    #[cfg(feature = "tracy")]
    fn alloc_span_ids(&self) -> Result<(u16, u16), GpuSpanCreationError> {
        let mut freelist = self
            .span_freelist
            .lock()
            .expect("the GPU span freelist is never poisoned");
        if freelist.len() < 2 {
            ::core::hint::cold_path();
            return Err(GpuSpanCreationError::TooManyPendingSpans);
        }
        // The length was checked, so both pops succeed.
        let start = freelist.pop().unwrap();
        let end = freelist.pop().unwrap();
        Ok((start, end))
    }

    /// Ends a manually tracked GPU span.
    ///
    /// This ends a span that was started with [`SpanSource::begin_gpu`], or a
    /// [`GpuSpanBuilder::begin`] that the caller does not drop a [`GpuSpan`] for. `query_id` is the
    /// id of the GPU timestamp query that was created for the end of the span; when the GPU
    /// timestamp becomes available, upload it with [`GpuContext::upload_gpu_timestamp`].
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn end_span(&self, query_id: u16) {
        #[cfg(not(feature = "tracy"))]
        let _ = query_id;

        #[cfg(feature = "tracy")]
        unsafe {
            // SAFETY: the caller owns the query, so nothing else is using its id.
            let () = sys::___tracy_emit_gpu_zone_end_serial(sys::___tracy_gpu_zone_end_data {
                queryId: query_id,
                context: self.value,
            });
        }
    }

    /// Uploads the GPU timestamp of a query.
    ///
    /// Upload the timestamps of a span in monotonically increasing order. For two nested spans
    /// *outer* and *inner*, that order is *outer* start, *inner* start, *inner* end, *outer* end.
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn upload_gpu_timestamp(&self, query_id: u16, gpu_timestamp: i64) {
        #[cfg(not(feature = "tracy"))]
        let _ = (query_id, gpu_timestamp);

        #[cfg(feature = "tracy")]
        unsafe {
            // SAFETY: the caller owns the query, so nothing else is using its id.
            let () = sys::___tracy_emit_gpu_time_serial(sys::___tracy_gpu_time_data {
                gpuTime: gpu_timestamp,
                queryId: query_id,
                context: self.value,
            });
        }
    }

    /// Reports the current GPU timestamp to the profiler.
    ///
    /// Some GPUs aggressively reset their clock when they enter a lower power state, which
    /// desynchronizes the CPU and the GPU timelines of the profiler. Fetch the current GPU
    /// timestamp and pass it here to resynchronize them.
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn sync_gpu_time(&self, gpu_timestamp: i64) {
        #[cfg(not(feature = "tracy"))]
        let _ = gpu_timestamp;

        #[cfg(feature = "tracy")]
        unsafe {
            // SAFETY: the timestamp is a plain value, and the context is still alive.
            let () = sys::___tracy_emit_gpu_time_sync_serial(sys::___tracy_gpu_time_sync_data {
                gpuTime: gpu_timestamp,
                context: self.value,
            });
        }
    }
}

// -----------------------------------------------------------------------------
// GpuContextCreationError

/// The reason creating a GPU context failed.
#[derive(Debug)]
pub enum GpuContextCreationError {
    /// More than [`u8::MAX`] contexts were created over the lifetime of the program.
    TooManyContextsCreated,
}

impl Display for GpuContextCreationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str("more than 255 GPU contexts were created over the lifetime of the program")
    }
}

impl core::error::Error for GpuContextCreationError {}

// -----------------------------------------------------------------------------
// GpuSpanState

/// The state of a GPU span, which decides whether ending it emits anything.
#[cfg(feature = "tracy")]
enum GpuSpanState {
    /// The span has been started.
    Started,

    /// The span has been ended, either waiting for its timestamps or after uploading them.
    Ended,
}

// -----------------------------------------------------------------------------
// GpuSpan

/// A span timing GPU work.
///
/// The span owns a pair of GPU timestamp queries: it records their ids when it starts, and the
/// timestamps the GPU writes for them are uploaded later, once they are readable. Dropping the span
/// ends its zone if that has not happened yet and gives its query ids back to the context; a span
/// that is dropped before its timestamps are uploaded therefore leaves a gap in the GPU timeline of
/// the profiler.
#[must_use]
pub struct GpuSpan {
    _seal: PhantomData<()>,
    #[cfg(feature = "tracy")]
    context: GpuContext,
    #[cfg(feature = "tracy")]
    start_query_id: u16,
    #[cfg(feature = "tracy")]
    end_query_id: u16,
    #[cfg(feature = "tracy")]
    state: GpuSpanState,
}

impl GpuSpan {
    /// Starts a GPU span, describing its source location on the spot.
    ///
    /// - `context` is the context the work is submitted to.
    /// - `name` is the name of the span, which is what the profiler UI lists it under.
    /// - `function`, `file` and `line` describe where the span comes from.
    ///
    /// Call this right next to where the GPU timestamp of the beginning of the work is recorded, so
    /// that the profiler can pair the CPU time of the call with the GPU timestamp. The description
    /// is allocated on the heap; a location that is started over and over is better described once
    /// with a [`SpanSource`] and started with [`SpanSource::begin_gpu`], or built with a
    /// [`GpuSpanBuilder`].
    ///
    /// # Errors
    ///
    /// Fails if more than 32767 spans are waiting for their GPU timestamps.
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    #[must_use = "a Span ends the zone as soon as it is dropped; bind it to a variable"]
    pub fn new(
        context: &GpuContext,
        name: &str,
        function: &str,
        file: &str,
        line: u32,
    ) -> Result<Self, GpuSpanCreationError> {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = (context, name, function, file, line);
            Ok(Self { _seal: PhantomData })
        }

        #[cfg(feature = "tracy")]
        return Self::begin_zone(context, alloc_srcloc(name, function, file, line));
    }

    /// Takes a pair of query ids from `context` and reports the beginning of a zone at `srcloc`.
    #[cfg(feature = "tracy")]
    fn begin_zone(context: &GpuContext, srcloc: u64) -> Result<Self, GpuSpanCreationError> {
        let (start_query_id, end_query_id) = context.alloc_span_ids()?;

        unsafe {
            // SAFETY: the query ids are taken from the context, so nothing else is using them.
            let () = sys::___tracy_emit_gpu_zone_begin_serial(sys::___tracy_gpu_zone_begin_data {
                srcloc,
                queryId: start_query_id,
                context: context.value,
            });
        }

        Ok(Self {
            context: context.clone(),
            start_query_id,
            end_query_id,
            state: GpuSpanState::Started,
            _seal: PhantomData,
        })
    }

    /// Marks the end of the GPU span.
    ///
    /// Call this right next to where the GPU timestamp of the end of the work is recorded. Only the
    /// first call emits anything; later calls are ignored.
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn end_zone(&mut self) {
        #[cfg(feature = "tracy")]
        {
            if !matches!(self.state, GpuSpanState::Started) {
                return;
            }

            unsafe {
                // SAFETY: the span holds the end query, so nothing else is using its id.
                let () = sys::___tracy_emit_gpu_zone_end_serial(sys::___tracy_gpu_zone_end_data {
                    queryId: self.end_query_id,
                    context: self.context.value,
                });
            }
            self.state = GpuSpanState::Ended;
        }
    }

    /// Supplies the GPU timestamp of the beginning of the span.
    ///
    /// Upload the start and the end timestamps of nested spans in monotonically increasing order,
    /// as described by [`GpuContext::upload_gpu_timestamp`].
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn upload_timestamp_start(&self, start_timestamp: i64) {
        #[cfg(not(feature = "tracy"))]
        let _ = start_timestamp;

        #[cfg(feature = "tracy")]
        unsafe {
            // SAFETY: the span holds the start query, so nothing else is using its id.
            let () = sys::___tracy_emit_gpu_time_serial(sys::___tracy_gpu_time_data {
                gpuTime: start_timestamp,
                queryId: self.start_query_id,
                context: self.context.value,
            });
        }
    }

    /// Supplies the GPU timestamp of the end of the span.
    ///
    /// Upload the start and the end timestamps of nested spans in monotonically increasing order,
    /// as described by [`GpuContext::upload_gpu_timestamp`].
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn upload_timestamp_end(&self, end_timestamp: i64) {
        #[cfg(not(feature = "tracy"))]
        let _ = end_timestamp;

        #[cfg(feature = "tracy")]
        unsafe {
            // SAFETY: the span holds the end query, so nothing else is using its id.
            let () = sys::___tracy_emit_gpu_time_serial(sys::___tracy_gpu_time_data {
                gpuTime: end_timestamp,
                queryId: self.end_query_id,
                context: self.context.value,
            });
        }
    }
}

#[cfg(feature = "tracy")]
impl Drop for GpuSpan {
    fn drop(&mut self) {
        if matches!(self.state, GpuSpanState::Started) {
            self.end_zone();
        }

        // The span is done with its queries, so other spans may use them again.
        let mut freelist = self
            .context
            .span_freelist
            .lock()
            .expect("the GPU span freelist is never poisoned");
        freelist.push(self.start_query_id);
        freelist.push(self.end_query_id);
    }
}

// -----------------------------------------------------------------------------
// GpuSpanCreationError

/// The reason creating a GPU span failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum GpuSpanCreationError {
    /// More than 32767 spans are waiting for their GPU timestamps.
    TooManyPendingSpans,
}

impl Display for GpuSpanCreationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str("more than 32767 GPU spans are waiting for their GPU timestamps")
    }
}

impl core::error::Error for GpuSpanCreationError {}

// -----------------------------------------------------------------------------
// GpuSpanBuilder

/// Describes a source location out of the given strings.
///
/// The profiler reads the description out of the queue that the beginning of the zone is sent to,
/// so the strings only have to outlive this call.
#[cfg(feature = "tracy")]
fn alloc_srcloc(name: &str, function: &str, file: &str, line: u32) -> u64 {
    unsafe {
        // SAFETY: the strings outlive the call, which copies them into the profiler queue.
        sys::___tracy_alloc_srcloc_name(
            line,
            file.as_ptr().cast(),
            file.len(),
            function.as_ptr().cast(),
            function.len(),
            name.as_ptr().cast(),
            name.len(),
            0,
        )
    }
}

/// A builder for GPU spans, the counterpart of [`SpanBuilder`](crate::SpanBuilder).
///
/// Start with the description of the span, configure it with the `with_*` methods, and start it
/// with [`GpuSpanBuilder::build`], which is given the context to report the span to:
///
/// ```
/// use zlim_tracy::{GpuContext, GpuContextType, GpuSpanBuilder};
///
/// # let context = GpuContext::new(None, GpuContextType::Vulkan, 0, 1.0).unwrap();
/// let mut span = GpuSpanBuilder::location("my_function", "my_file.rs", 42)
///     .with_name("my_work")
///     .build(&context)
///     .unwrap();
///
/// // Record the GPU timestamps of the work around this call. The span keeps the ids of the
/// // queries they are written to.
/// span.end_zone();
/// span.upload_timestamp_start(0);
/// span.upload_timestamp_end(1);
/// ```
///
/// The description of a span is a name, a function, a file and a line, and it is allocated on the
/// heap when the span is started. A location that is started over and over is better described once
/// with a [`SpanSource`] and started with [`SpanSource::begin_gpu`], which also carries a color. A
/// GPU span never collects a callstack.
#[derive(Clone)]
pub struct GpuSpanBuilder<'a> {
    _seal: PhantomData<&'a ()>,
    #[cfg(feature = "tracy")]
    name: &'a str,
    #[cfg(feature = "tracy")]
    function: &'a str,
    #[cfg(feature = "tracy")]
    file: &'a str,
    #[cfg(feature = "tracy")]
    line: u32,
}

impl<'a> GpuSpanBuilder<'a> {
    /// Creates a builder for a span that only describes the function it belongs to.
    #[inline]
    pub const fn new(function: &'a str) -> Self {
        #[cfg(not(feature = "tracy"))]
        let _ = function;
        Self {
            _seal: PhantomData,
            #[cfg(feature = "tracy")]
            name: "",
            #[cfg(feature = "tracy")]
            function,
            #[cfg(feature = "tracy")]
            file: "",
            #[cfg(feature = "tracy")]
            line: 0,
        }
    }

    /// Creates a builder for a span with an explicit function, file and line.
    #[inline]
    pub const fn location(function: &'a str, file: &'a str, line: u32) -> Self {
        #[cfg(not(feature = "tracy"))]
        let _ = function;
        #[cfg(not(feature = "tracy"))]
        let _ = file;
        #[cfg(not(feature = "tracy"))]
        let _ = line;
        Self {
            _seal: PhantomData,
            #[cfg(feature = "tracy")]
            name: "",
            #[cfg(feature = "tracy")]
            function,
            #[cfg(feature = "tracy")]
            file,
            #[cfg(feature = "tracy")]
            line,
        }
    }

    /// Creates a builder for a span that only reports the name it is given.
    #[inline]
    pub const fn empty() -> Self {
        Self::new("")
    }

    /// Sets the name of the span.
    #[inline]
    pub const fn with_name(self, name: &'a str) -> Self {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = name;
            self
        }

        #[cfg(feature = "tracy")]
        return Self { name, ..self };
    }

    /// Sets the file of the source location.
    #[inline]
    pub const fn with_file(self, file: &'a str) -> Self {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = file;
            self
        }

        #[cfg(feature = "tracy")]
        return Self { file, ..self };
    }

    /// Sets the line of the source location.
    #[inline]
    pub const fn with_line(self, line: u32) -> Self {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = line;
            self
        }

        #[cfg(feature = "tracy")]
        return Self { line, ..self };
    }

    /// Starts the span in `context`.
    ///
    /// The returned span owns a pair of query ids taken from `context`: it ends its zone and returns
    /// them when it is dropped, and its timestamps still have to be uploaded with
    /// [`GpuSpan::upload_timestamp_start`] and [`GpuSpan::upload_timestamp_end`] once the GPU has
    /// written them.
    ///
    /// # Errors
    ///
    /// Fails if more than 32767 spans are waiting for their GPU timestamps.
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    #[must_use = "a Span ends the zone as soon as it is dropped; bind it to a variable"]
    pub fn build(self, context: &GpuContext) -> Result<GpuSpan, GpuSpanCreationError> {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = self;
            GpuSpan::new(context, "", "", "", 0)
        }

        #[cfg(feature = "tracy")]
        GpuSpan::new(context, self.name, self.function, self.file, self.line)
    }

    /// Starts the span in `context` with a query that the caller manages.
    ///
    /// `query_id` is the id of the GPU timestamp query that was created for the beginning of the
    /// span. Nothing is returned, because the caller is the one that ends the zone with
    /// [`GpuContext::end_span`] and uploads the timestamps with
    /// [`GpuContext::upload_gpu_timestamp`]. Prefer [`GpuSpanBuilder::build`], which keeps the
    /// queries of the span in one place.
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn begin(self, context: &GpuContext, query_id: u16) {
        #[cfg(not(feature = "tracy"))]
        let _ = (self, context, query_id);

        #[cfg(feature = "tracy")]
        unsafe {
            // SAFETY: the caller owns the query, so nothing else is using its id.
            let () = sys::___tracy_emit_gpu_zone_begin_serial(sys::___tracy_gpu_zone_begin_data {
                srcloc: alloc_srcloc(self.name, self.function, self.file, self.line),
                queryId: query_id,
                context: context.value,
            });
        }
    }
}

// -----------------------------------------------------------------------------
// SpanSource

/// Begins a GPU span at a source location that is described once and entered many times.
impl SpanSource {
    /// Begins a GPU span with a query that the caller manages.
    ///
    /// This is the GPU counterpart of [`SpanSource::begin`]: the source location is reported as it
    /// is, so nothing is allocated. `query_id` is the id of the GPU timestamp query that was
    /// created for the beginning of the span; end the zone with [`GpuContext::end_span`] and upload
    /// the timestamps with [`GpuContext::upload_gpu_timestamp`] once they are readable.
    ///
    /// [`SpanSource::begin`]: crate::SpanSource::begin
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn begin_gpu(&'static self, context: &GpuContext, query_id: u16) {
        #[cfg(not(feature = "tracy"))]
        let _ = (context, query_id);

        #[cfg(feature = "tracy")]
        unsafe {
            // SAFETY: the source location is `'static`, so the profiler can read it at any later
            // point in time, and the caller owns the query.
            let () = sys::___tracy_emit_gpu_zone_begin_serial(sys::___tracy_gpu_zone_begin_data {
                srcloc: core::ptr::addr_of!(self.data) as u64,
                queryId: query_id,
                context: context.value,
            });
        }
    }
}

// -----------------------------------------------------------------------------
// gpu_span_builder

/// Creates a [`GpuSpanBuilder`] at the call site.
///
/// The builder is created with the enclosing function, the file and the line already filled in, so
/// a span that needs more than that — a name, or a line that is only known at runtime — does not
/// have to repeat them by hand:
///
/// ```no_run
/// use zlim_tracy::{GpuContext, GpuContextType, gpu_span_builder};
/// # let context: GpuContext  = todo!();
///
/// let span = gpu_span_builder!().with_name("my_work").build(&context).unwrap();
/// ```
///
/// The accepted forms are:
///
/// - `gpu_span_builder!()` reports the enclosing function, the file and the line.
/// - `gpu_span_builder!(file = false)` reports the enclosing function only.
///
/// The name is set with [`GpuSpanBuilder::with_name`], and the span is started with
/// [`GpuSpanBuilder::build`], which is given the context to report the span to.
#[macro_export]
macro_rules! gpu_span_builder {
    () => {
        $crate::GpuSpanBuilder::location(
            $crate::span_help!(@func),
            ::core::file!(),
            ::core::line!(),
        )
    };
    (file = false $(,)?) => {
        $crate::GpuSpanBuilder::new($crate::span_help!(@func))
    };
}
