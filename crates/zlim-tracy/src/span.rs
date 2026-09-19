use core::ffi::CStr;
use core::marker::PhantomData;

#[cfg(feature = "tracy")]
use tracy_client_sys as sys;

// -----------------------------------------------------------------------------
// Span

/// A handle of an ongoing span of execution.
///
/// The span is reported, and its duration measured, when the handle is dropped.
///
/// Similar to `tracing` crate's `EnterredSpan`.
pub struct Span {
    #[cfg(feature = "tracy")]
    zone: sys::___tracy_c_zone_context,
    // Ensure it's `!Send + !Sync`
    _marker: PhantomData<*mut ()>,
}

#[cfg(feature = "tracy")]
impl Drop for Span {
    fn drop(&mut self) {
        unsafe {
            // SAFE: The only way to construct `Span` is by creating a valid
            // tracy zone context. We also still have an owned Client handle.
            let () = sys::___tracy_emit_zone_end(self.zone);
        }
    }
}

impl Span {
    /// Creates the no-op span of a build without the profiler.
    #[doc(hidden)]
    #[inline(always)]
    #[cfg(not(feature = "tracy"))]
    pub const fn __no_tracy() -> Self {
        Self {
            _marker: PhantomData,
        }
    }

    /// Starts a span with an explicit source location.
    ///
    /// - `name` is the name of the span, which is what the profiler UI lists it under.
    /// - `func` is the function the span belongs to.
    /// - `file` and `line` are the location the span is reported at.
    /// - `color` is the color of the span, as RGB, where `0` lets the profiler pick a color.
    /// - `callstack_depth` is how many call frames are collected for the span; zero collects none.
    ///   Collecting a callstack is expensive, so it is only worth it while looking for the caller
    ///   of the span.
    ///
    /// The source location is allocated on the heap, and the allocation is kept until the profiler
    /// has read it out, so entering a location that is known ahead of time from a loop allocates
    /// over and over. In that case create a [`SpanSource`] for the location instead and enter it:
    /// it is described once, and [`SpanSource::begin`] and [`SpanSource::begin_with`] then need no
    /// allocation at all.
    #[cfg_attr(not(feature = "tracy"), inline)]
    pub fn new(
        name: &str,
        func: &str,
        file: &str,
        line: u32,
        color: u32,
        callstack_depth: u16,
    ) -> Self {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = name;
            let _ = func;
            let _ = file;
            let _ = line;
            let _ = color;
            let _ = callstack_depth;
            Self {
                _marker: PhantomData,
            }
        }

        #[cfg(feature = "tracy")]
        unsafe {
            let loc = sys::___tracy_alloc_srcloc_name(
                line,
                file.as_ptr().cast(),
                file.len(),
                func.as_ptr().cast(),
                func.len(),
                name.as_ptr().cast(),
                name.len(),
                color,
            );
            let zone = if callstack_depth == 0 {
                sys::___tracy_emit_zone_begin_alloc(loc, 1)
            } else {
                let stack_depth: i32 = crate::internal::adjust_stack_depth(callstack_depth).into();
                sys::___tracy_emit_zone_begin_alloc_callstack(loc, stack_depth, 1)
            };
            Span {
                zone,
                _marker: PhantomData,
            }
        }
    }

    /// Reports a numeric value associated with this span.
    #[cfg_attr(not(feature = "tracy"), inline)]
    pub fn emit_value(&self, value: u64) {
        #[cfg(not(feature = "tracy"))]
        let _ = value;

        #[cfg(feature = "tracy")]
        unsafe {
            // SAFE: the zone is alive for as long as this span is, and only this span ends it.
            let () = sys::___tracy_emit_zone_value(self.zone, value);
        }
    }

    /// Reports a text associated with this span.
    #[cfg_attr(not(feature = "tracy"), inline)]
    pub fn emit_text(&self, text: &str) {
        #[cfg(not(feature = "tracy"))]
        let _ = text;

        #[cfg(feature = "tracy")]
        unsafe {
            // SAFE: the text outlives the call, which copies it into the profiler queue.
            let () = sys::___tracy_emit_zone_text(self.zone, text.as_ptr().cast(), text.len());
        }
    }

    /// Reports a color associated with this span, as RGB.
    ///
    /// The customary way to write the color is a hexadecimal literal,
    /// such as `0xFF0000` for red, `0x00FF00` for green or `0x0000FF` for blue.
    #[cfg_attr(not(feature = "tracy"), inline)]
    pub fn emit_color(&self, color: u32) {
        #[cfg(not(feature = "tracy"))]
        let _ = color;

        #[cfg(feature = "tracy")]
        unsafe {
            // SAFE: the zone is alive for as long as this span is, and only this span ends it.
            let () = sys::___tracy_emit_zone_color(self.zone, color);
        }
    }
}

// -----------------------------------------------------------------------------
// SpanBuilder

/// A builder for spans whose source location is only known piecewise.
///
/// Create it with the [`span_builder!`](crate::span_builder) macro, describe the span with the
/// `with_*` methods, and start it with [`SpanBuilder::build`].
#[derive(Debug, Default, Clone)]
pub struct SpanBuilder<'a> {
    _seal: PhantomData<&'a ()>,
    #[cfg(feature = "tracy")]
    name: &'a str,
    #[cfg(feature = "tracy")]
    func: &'a str,
    #[cfg(feature = "tracy")]
    file: &'a str,
    #[cfg(feature = "tracy")]
    line: u32,
    #[cfg(feature = "tracy")]
    color: u32,
    #[cfg(feature = "tracy")]
    callstack_depth: u16,
}

impl<'a> SpanBuilder<'a> {
    /// Creates a builder without any source location.
    #[inline]
    pub const fn empty() -> Self {
        Self {
            _seal: PhantomData,
            #[cfg(feature = "tracy")]
            name: "",
            #[cfg(feature = "tracy")]
            func: "",
            #[cfg(feature = "tracy")]
            file: "",
            #[cfg(feature = "tracy")]
            line: 0,
            #[cfg(feature = "tracy")]
            color: 0,
            #[cfg(feature = "tracy")]
            callstack_depth: 0,
        }
    }

    /// Creates a builder that reports `function` as the function name.
    #[inline]
    pub const fn new(function: &'a str) -> Self {
        #[cfg(not(feature = "tracy"))]
        let _ = function;
        Self {
            _seal: PhantomData,
            #[cfg(feature = "tracy")]
            name: "",
            #[cfg(feature = "tracy")]
            func: function,
            #[cfg(feature = "tracy")]
            file: "",
            #[cfg(feature = "tracy")]
            line: 0,
            #[cfg(feature = "tracy")]
            color: 0,
            #[cfg(feature = "tracy")]
            callstack_depth: 0,
        }
    }

    /// Creates a builder with an explicit function, file and line.
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
            func: function,
            #[cfg(feature = "tracy")]
            file,
            #[cfg(feature = "tracy")]
            line,
            #[cfg(feature = "tracy")]
            color: 0,
            #[cfg(feature = "tracy")]
            callstack_depth: 0,
        }
    }

    /// Sets the name of the span.
    #[inline]
    pub const fn with_name(self, name: &'a str) -> Self {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = name;
            Self { _seal: PhantomData }
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
            Self { _seal: PhantomData }
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
            Self { _seal: PhantomData }
        }

        #[cfg(feature = "tracy")]
        return Self { line, ..self };
    }

    /// Sets the color of the span, as RGB (0x00RRGGBB).
    #[inline]
    pub const fn with_color(self, color: u32) -> Self {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = color;
            Self { _seal: PhantomData }
        }

        #[cfg(feature = "tracy")]
        return Self { color, ..self };
    }

    /// Collects at most `callstack_depth` call frames for the span.
    #[inline]
    pub const fn with_callstack(self, callstack_depth: u16) -> Self {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = callstack_depth;
            Self { _seal: PhantomData }
        }

        #[cfg(feature = "tracy")]
        return Self {
            callstack_depth,
            ..self
        };
    }

    /// Starts the span.
    #[inline]
    pub fn build(self) -> Span {
        #[cfg(not(feature = "tracy"))]
        return Span::__no_tracy();

        #[cfg(feature = "tracy")]
        return Span::new(
            self.name,
            self.func,
            self.file,
            self.line,
            self.color,
            self.callstack_depth,
        );
    }
}

// -----------------------------------------------------------------------------
// Macros

/// Implementation detail of the [`span!`](crate::span) and
/// [`span_builder!`](crate::span_builder) macros.
#[doc(hidden)]
#[macro_export]
#[cfg(feature = "tracy")]
macro_rules! span_help {
    (@func) => {{
        struct S;
        // A Hack, `type_name` returns `module::function::type` in function scope.
        // So we define a new type and parse function name from it.
        let fn_ty: &'static str = ::core::any::type_name::<S>();
        let len: usize = <str>::len(fn_ty);
        if len > 3 {
            // `3` -> `::S`, optional, usually can be optimized by compiler
            &fn_ty[..len - 3]
        } else {
            fn_ty
        }
    }};
    (@new, $name:expr, $func:expr, true) => {
        $crate::Span::new($name, $func, ::core::file!(), ::core::line!(), 0, 0)
    };
    (@new, $name:expr, $func:expr, false) => {
        $crate::Span::new($name, $func, "", 0, 0, 0)
    };
    (@builder_0, $func:expr, true) => {
        $crate::SpanBuilder::location($func, ::core::file!(), ::core::line!())
    };
    (@builder_0, $func:expr, false) => {
        $crate::SpanBuilder::new($func)
    };
    (@builder_1, $name:expr, $func:expr, true) => {
        $crate::SpanBuilder::location($func, ::core::file!(), ::core::line!()).with_name($name)
    };
    (@builder_1, $name:expr, $func:expr, false) => {
        $crate::SpanBuilder::new($func).with_name($name)
    };
}

/// Implementation detail of the [`span!`](crate::span) and
/// [`span_builder!`](crate::span_builder) macros.
#[doc(hidden)]
#[macro_export]
#[cfg(not(feature = "tracy"))]
macro_rules! span_help {
    (@func) => {
        ""
    };
    (@new, $name:expr, $func:expr, true) => {{
        if cfg!(false) {
            let _ = $name;
            let _ = $func;
        }
        $crate::Span::__no_tracy()
    }};
    (@new, $name:expr, $func:expr, false) => {{
        if cfg!(false) {
            let _ = $name;
            let _ = $func;
        }
        $crate::Span::__no_tracy()
    }};
    (@builder_0, $func:expr, true) => {{
        if cfg!(false) {
            let _ = $func;
        }
        $crate::SpanBuilder::empty()
    }};
    (@builder_0, $func:expr, false) => {{
        if cfg!(false) {
            let _ = $func;
        }
        $crate::SpanBuilder::empty()
    }};
    (@builder_1, $name:expr, $func:expr, true) => {{
        if cfg!(false) {
            let _ = $name;
            let _ = $func;
        }
        $crate::SpanBuilder::empty()
    }};
    (@builder_1, $name:expr, $func:expr, false) => {{
        if cfg!(false) {
            let _ = $name;
            let _ = $func;
        }
        $crate::SpanBuilder::empty()
    }};
}

/// Starts a span at the call site.
///
/// The span is described from the call site itself: the enclosing function is parsed out of a type
/// that the macro declares, and the file and the line come from `file!()` and `line!()`. The
/// accepted forms are:
///
/// - `span!()` reports the enclosing function as both the function and the name of the span.
/// - `span!(name)` uses `name` as the name of the span.
/// - `span!(function = func)` and `span!(function = func, name)` report `func` instead of the
///   enclosing function.
/// - `span!(file = false, ...)` reports neither the file nor the line, which keeps the span out of
///   the source view of the profiler. This is the one argument that combines with the others, in
///   either order.
///
/// The macro calls [`Span::new`], so the span allocates its source location on the heap. A location
/// that is entered over and over should be described once with a [`SpanSource`] instead.
#[macro_export]
macro_rules! span {
    () => {{
        $crate::span_help!(@new, "", $crate::span_help!(@func), true)
    }};
    (file = false, $(,)?) => {
        $crate::span_help!(@new, "", $crate::span_help!(@func), false)
    };
    (function = $func:expr $(,)?) => {
        $crate::span_help!(@new, "", $func, true)
    };
    (file = false, function = $func:expr $(,)?) => {
        $crate::span_help!(@new, "", $func, false)
    };
    (file = false, function = $func:expr, $name:expr $(,)?) => {
        $crate::span_help!(@new, $name, $func, false)
    };
    (file = false, $name:expr $(,)?) => {
        $crate::span_help!(@new, $name, $crate::span_help!(@func), false)
    };
    (function = $func:expr, $name:expr $(,)?) => {
        $crate::span_help!(@new, $name, $func, true)
    };
    ($name:expr $(,)?) => {
        $crate::span_help!(@new, $name, $crate::span_help!(@func), true)
    };
}

/// Creates a [`SpanBuilder`] at the call site.
///
/// The builder is created with the enclosing function, the file and the line already filled in, so
/// a span that needs more than a name — a color, a callstack, or a name that is only known at
/// runtime — does not have to repeat them by hand:
///
/// ```
/// let span = zlim_tracy::span_builder!()
///     .with_name("loading")
///     .with_color(0x00FF00)
///     .build();
/// ```
///
/// The accepted forms are:
///
/// - `span_builder!()` reports the enclosing function, the file and the line.
/// - `span_builder!(file = false)` reports the enclosing function only.
///
/// The name is set with [`SpanBuilder::with_name`], and the span is started with
/// [`SpanBuilder::build`].
#[macro_export]
macro_rules! span_builder {
    () => {
        $crate::span_help!(@builder_0, $crate::span_help!(@func), true)
    };
    (file = false $(,)?) => {
        $crate::span_help!(@builder_0, $crate::span_help!(@func), false)
    };
}

// -----------------------------------------------------------------------------
// SpanSource

/// The source location of a span, shared by every span that reports it.
///
/// A `SpanSource` is meant to be created once, leaked with [`SpanSource::leak`] and entered any
/// number of times, so that a location that is entered in a loop is described only once.
pub struct SpanSource {
    #[cfg(feature = "tracy")]
    pub(crate) data: sys::___tracy_source_location_data,
    _marker: PhantomData<()>,
}

// SAFETY: the source location holds nothing but pointers to `'static` strings, so it can be shared
// and sent between threads.
#[cfg(feature = "tracy")]
unsafe impl Sync for SpanSource {}
#[cfg(feature = "tracy")]
unsafe impl Send for SpanSource {}

// `SpanSource` is shared between threads, so the two impls above must stay.
const _ASSERT_: () = const {
    const fn type_assert<T: Send + Send>() {}
    type_assert::<SpanSource>();
    type_assert::<&'static SpanSource>();
};

impl SpanSource {
    /// Returns the empty source location of a build without the profiler.
    #[doc(hidden)]
    #[cfg(not(feature = "tracy"))]
    pub const fn __no_tracy() -> &'static Self {
        const EMPTY: &SpanSource = &SpanSource {
            _marker: PhantomData,
        };
        EMPTY
    }

    /// Creates a source location from static strings.
    ///
    /// - `name` is the name of the span, which is what the profiler UI lists it under.
    /// - `func` is the function the span belongs to.
    /// - `file` and `line` are the location the span is reported at.
    /// - `color` is the color of the span, as RGB (0xRRGGBB), where `0` lets the profiler pick a color.
    /// - `callstack_depth` is how many call frames are collected for the span; zero collects none.
    ///   Collecting a callstack is expensive, so it is only worth it while looking for the caller
    ///   of the span.
    #[inline]
    pub const fn new(
        name: &'static CStr,
        func: &'static CStr,
        file: &'static CStr,
        line: u32,
        color: u32,
    ) -> Self {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = name;
            let _ = func;
            let _ = file;
            let _ = line;
            let _ = color;

            Self {
                _marker: PhantomData,
            }
        }

        #[cfg(feature = "tracy")]
        return SpanSource {
            data: sys::___tracy_source_location_data {
                // See `tracy-client` or `tracy`'s implementation
                // For empty names, use nullptr instead of `\0`.
                // I don't know what the difference is either. (T_T)
                name: if name.is_empty() {
                    ::core::ptr::null()
                } else {
                    name.as_ptr()
                },
                function: func.as_ptr(),
                file: file.as_ptr(),
                line,
                color,
            },
            _marker: PhantomData,
        };
    }

    /// Creates a source location from strings and leak it.
    #[must_use]
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn new_leak(
        name: String,
        func: String,
        file: &'static CStr,
        line: u32,
        color: u32,
    ) -> &'static Self {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = name;
            let _ = func;
            let _ = file;
            let _ = line;
            let _ = color;
            SpanSource::__no_tracy()
        }

        #[cfg(feature = "tracy")]
        fn alloc_or_empty(mut s: String) -> &'static CStr {
            if s.is_empty() {
                return c"";
            }

            if s.as_bytes().last().copied() != Some(b'\0') {
                s.push('\0');
            }

            let bytes = zlim_utils::mem::Global::alloc_str(&s).as_bytes();
            CStr::from_bytes_until_nul(bytes).expect("the last char is nul")
        }

        #[cfg(feature = "tracy")]
        {
            let name = alloc_or_empty(name);
            let func = alloc_or_empty(func);
            Self::new(name, func, file, line, color).leak()
        }
    }

    /// Copies the source location into the global memory pool, which makes it `'static`.
    #[must_use]
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn leak(self) -> &'static Self {
        #[cfg(not(feature = "tracy"))]
        return SpanSource::__no_tracy();

        #[cfg(feature = "tracy")]
        return zlim_utils::mem::Global::alloc_static(self);
    }

    /// Starts a span at this source location.
    #[doc(alias = "enter")]
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    #[must_use = "a Span ends the zone as soon as it is dropped; bind it to a variable"]
    pub fn begin(&'static self) -> Span {
        #[cfg(not(feature = "tracy"))]
        return Span {
            _marker: PhantomData,
        };

        #[cfg(feature = "tracy")]
        unsafe {
            Span {
                zone: sys::___tracy_emit_zone_begin(&self.data, 1),
                _marker: PhantomData,
            }
        }
    }

    /// Starts a span at this source location, collecting at most `callstack_depth` call frames.
    #[doc(alias = "enter_with")]
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    #[must_use = "a Span ends the zone as soon as it is dropped; bind it to a variable"]
    pub fn begin_with(&'static self, callstack_depth: u16) -> Span {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = callstack_depth;
            Span {
                _marker: PhantomData,
            }
        }

        #[cfg(feature = "tracy")]
        unsafe {
            let zone = if callstack_depth == 0 {
                sys::___tracy_emit_zone_begin(&self.data, 1)
            } else {
                let stack_depth = crate::internal::adjust_stack_depth(callstack_depth).into();
                sys::___tracy_emit_zone_begin_callstack(&self.data, stack_depth, 1)
            };

            Span {
                zone,
                _marker: PhantomData,
            }
        }
    }
}
