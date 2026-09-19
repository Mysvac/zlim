use core::ffi::CStr;
use core::marker::PhantomData;

use crate::Client;

#[cfg(feature = "tracy")]
use tracy_client_sys as sys;

// -----------------------------------------------------------------------------
// PlotName

/// The name of a plot.
///
/// Normally constructed with the [`plot_name!`] macro, which is a compile-time
/// operation without memory allocation.
///
/// Use [`PlotName::new_leak`] only for names that are not known at compile
/// time. It leaks the string into the memory pool **without** deduplication, so
/// the name should be created once and reused.
///
/// If there is already a static [`CStr`], consider using [`PlotName::new`] to
/// a [`PlotName`] without any additional allocation.
///
/// # Without the profiler
///
/// When the `tracy` cargo feature is disabled, a name carries nothing: the type
/// is a zero-sized marker, and every value of it is equal to every other one, no
/// matter which name each of them was built from. An implementation that relies
/// on names being distinct, such as a plot that is only reported again when its
/// name changes, therefore needs to be based on something else in that case.
///
/// [`plot_name!`]: crate::plot_name
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlotName {
    _seal: PhantomData<()>,
    #[cfg(feature = "tracy")]
    name: &'static str,
}

impl PlotName {
    /// Creates a `PlotName` from a static [`CStr`].
    ///
    /// The name is used as it is, so the caller keeps the string alive and nothing is allocated.
    ///
    /// # Panics
    ///
    /// Panics if the name is not valid UTF-8.
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn new(name: &'static CStr) -> Self {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = name;
            Self { _seal: PhantomData }
        }

        #[cfg(feature = "tracy")]
        {
            let name = name.to_str().expect("a plot name must be valid UTF-8");
            Self {
                _seal: PhantomData,
                name,
            }
        }
    }

    /// Constructs a `PlotName` from a runtime string.
    ///
    /// The name is copied into the global memory pool, so it stays valid for
    /// the rest of the program. Call this once per name and keep the result:
    /// calling it in a loop grows memory without bound.
    ///
    /// The [`plot_name!`](crate::plot_name) macro is preferable, because a
    /// literal name needs no allocation at all.
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    #[must_use]
    pub fn new_leak(name: &str) -> Self {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = name;
            Self { _seal: PhantomData }
        }

        #[cfg(feature = "tracy")]
        {
            // The profiler reads the name as a C string, so the terminator has to be part of the
            // copy whenever the caller did not provide one.
            let name = if name.bytes().last() == Some(b'\0') {
                zlim_utils::mem::Global::alloc_str(name)
            } else {
                let mut buf = String::with_capacity(name.len() + 1);
                buf.push_str(name);
                buf.push('\0');
                zlim_utils::mem::Global::alloc_str(&buf)
            };
            Self {
                _seal: PhantomData,
                name,
            }
        }
    }

    /// Constructs a `PlotName` from a null-terminated literal.
    ///
    /// Only [`crate::internal::create_plot`], which the [`plot_name!`](crate::plot_name) macro
    /// expands to, calls this function, and it checks that the literal is null-terminated.
    #[cfg(feature = "tracy")]
    #[inline(always)]
    pub(crate) const fn from_lit(name: &'static str) -> Self {
        Self {
            _seal: PhantomData,
            name,
        }
    }

    /// Creates the nameless plot name of a build without the profiler.
    #[doc(hidden)]
    #[inline(always)]
    #[cfg(not(feature = "tracy"))]
    pub const fn __no_tracy() -> Self {
        Self { _seal: PhantomData }
    }
}

// -----------------------------------------------------------------------------
// PlotConfiguration

/// The format of the values of a plot in the profiler UI.
#[derive(Debug, Hash, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
#[non_exhaustive]
pub enum PlotFormat {
    /// Values are shown as plain numbers.
    #[default]
    Number,

    /// Values are shown as byte counts, in kilobytes, megabytes and so on.
    Memory,

    /// Values are shown as percentages, where 100 is 100%.
    Percentage,

    /// Values are shown as watts.
    Watts,
}

/// The style of the lines of a plot in the profiler UI.
#[derive(Debug, Hash, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
#[non_exhaustive]
pub enum PlotLineStyle {
    /// Lines are stepped, so they look like a staircase.
    Stepped,

    /// Lines are smooth, interpolating between two values.
    #[default]
    Smooth,
}

/// How a plot is displayed by the profiler UI.
#[derive(Clone, PartialEq, Debug)]
pub struct PlotConfiguration {
    _seal: PhantomData<()>,
    #[cfg(feature = "tracy")]
    format: PlotFormat,
    #[cfg(feature = "tracy")]
    line_style: PlotLineStyle,
    #[cfg(feature = "tracy")]
    fill: bool,
    #[cfg(feature = "tracy")]
    color: Option<u32>,
}

impl PlotConfiguration {
    /// Sets the format of the values of the plot.
    #[inline]
    #[must_use]
    pub const fn format(self, format: PlotFormat) -> Self {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = format;
            self
        }

        #[cfg(feature = "tracy")]
        return Self { format, ..self };
    }

    /// Sets the style of the lines of the plot.
    #[inline]
    #[must_use]
    pub const fn line_style(self, line_style: PlotLineStyle) -> Self {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = line_style;
            self
        }

        #[cfg(feature = "tracy")]
        return Self { line_style, ..self };
    }

    /// Sets whether the area below the line is filled with a solid color.
    #[inline]
    #[must_use]
    pub const fn fill(self, fill: bool) -> Self {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = fill;
            self
        }

        #[cfg(feature = "tracy")]
        return Self { fill, ..self };
    }

    /// Sets a custom color of the plot, as RGB.
    ///
    /// [`None`] lets the profiler pick a color of its own.
    #[inline]
    #[must_use]
    pub const fn color(self, color: Option<u32>) -> Self {
        #[cfg(not(feature = "tracy"))]
        {
            let _ = color;
            self
        }

        #[cfg(feature = "tracy")]
        return Self { color, ..self };
    }
}

impl Default for PlotConfiguration {
    /// A filled, smooth, uncolored line of plain numbers.
    #[inline]
    fn default() -> Self {
        Self {
            _seal: PhantomData,
            #[cfg(feature = "tracy")]
            format: PlotFormat::default(),
            #[cfg(feature = "tracy")]
            line_style: PlotLineStyle::default(),
            #[cfg(feature = "tracy")]
            fill: true,
            #[cfg(feature = "tracy")]
            color: None,
        }
    }
}

// -----------------------------------------------------------------------------
// Client

/// Instrumentation for drawing 2D plots.
impl Client {
    /// Adds a point with the value `value` to the plot named `plot_name`.
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn plot(plot_name: PlotName, value: f64) {
        #[cfg(not(feature = "tracy"))]
        let _ = (plot_name, value);

        #[cfg(feature = "tracy")]
        unsafe {
            // SAFETY: `plot_name` is null-terminated and outlives the call.
            let () = sys::___tracy_emit_plot(plot_name.name.as_ptr().cast(), value);
        }
    }

    /// Sets the display configuration of the plot named `plot_name`.
    ///
    /// The configuration only has to be reported once, unless it changes.
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn plot_config(plot_name: PlotName, configuration: PlotConfiguration) {
        #[cfg(not(feature = "tracy"))]
        let _ = (plot_name, configuration);

        #[cfg(feature = "tracy")]
        {
            let format = match configuration.format {
                PlotFormat::Number => sys::TracyPlotFormatEnum_TracyPlotFormatNumber,
                PlotFormat::Memory => sys::TracyPlotFormatEnum_TracyPlotFormatMemory,
                PlotFormat::Percentage => sys::TracyPlotFormatEnum_TracyPlotFormatPercentage,
                PlotFormat::Watts => sys::TracyPlotFormatEnum_TracyPlotFormatWatt,
            } as i32;
            let stepped = configuration.line_style == PlotLineStyle::Stepped;
            let filled = configuration.fill;
            let color = configuration.color.unwrap_or(0);

            unsafe {
                // SAFETY: `plot_name` is null-terminated and outlives the call.
                let () = sys::___tracy_emit_plot_config(
                    plot_name.name.as_ptr().cast(),
                    format,
                    stepped.into(),
                    filled.into(),
                    color,
                );
            }
        }
    }
}

// -----------------------------------------------------------------------------
// plot_name

/// Constructs a [`PlotName`] from a literal name.
///
/// The macro can be used in a `const` context, and the resulting
/// name can be passed to [`Client::plot`] and [`Client::plot_config`].
#[macro_export]
#[cfg(feature = "tracy")]
macro_rules! plot_name {
    ($name: expr) => {{ $crate::internal::create_plot(concat!($name, "\0")) }};
}

/// Constructs a [`PlotName`] from a literal name.
///
/// The macro can be used in a `const` context, and the resulting
/// name can be passed to [`Client::plot`] and [`Client::plot_config`].
#[macro_export]
#[cfg(not(feature = "tracy"))]
macro_rules! plot_name {
    ($name: expr) => {{
        /* let _ = $name; */
        $crate::PlotName::__no_tracy()
    }};
}

// -----------------------------------------------------------------------------
// plot

/// Adds a point to a plot.
///
/// Equivalent to calling [`Client::plot`] with the
/// [`plot_name!`](crate::plot_name) of the name.
#[macro_export]
#[cfg(feature = "tracy")]
macro_rules! plot {
    ($name: expr, $value: expr) => {{
        $crate::Client::plot($crate::plot_name!($name), $value);
    }};
}

/// Adds a point to a plot.
///
/// Equivalent to calling [`Client::plot`] with the
/// [`plot_name!`](crate::plot_name) of the name.
#[macro_export]
#[cfg(not(feature = "tracy"))]
macro_rules! plot {
    ($name: expr, $value: expr) => {{ /* let _ = ($name, $value); */ }};
}
