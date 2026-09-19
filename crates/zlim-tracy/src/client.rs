#[cfg(feature = "tracy")]
use tracy_client_sys as sys;

// -----------------------------------------------------------------------------
// Client

/// Instrumentation for the profiler.
///
/// The profiler is active exactly when the `tracy` cargo feature is enabled, and it starts up and
/// shuts down on its own, so this type is never instantiated and never dropped. It exists to group
/// the functions that report events, which is why all of them are associated functions.
///
/// When the `tracy` feature is disabled, every function of this type is a no-op that the compiler
/// removes, so instrumentation can be written once and used in both configurations.
pub struct Client;

impl Client {
    /// Reports whether the profiler is compiled in.
    #[must_use]
    #[inline(always)]
    pub fn is_running() -> bool {
        cfg!(feature = "tracy")
    }

    /// Reports whether a profiler application is connected to the client.
    ///
    /// This is always `false` when the `tracy` feature is disabled.
    #[must_use]
    #[inline(always)]
    pub fn is_connected() -> bool {
        #[cfg(not(feature = "tracy"))]
        return false;

        #[cfg(feature = "tracy")]
        return unsafe {
            // SAFETY: the client is running, so querying it is allowed at any time.
            sys::___tracy_connected() != 0
        };
    }
}

// -----------------------------------------------------------------------------
// Messages

/// Instrumentation for events that occur at a single instant.
impl Client {
    /// Emits a message.
    ///
    /// A non-zero `callstack_depth` also collects a callstack of at most that
    /// many frames for the message. Collecting a callstack adds a non-trivial
    /// amount of overhead to this call.
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn message(message: &str, callstack_depth: u16) {
        #[cfg(not(feature = "tracy"))]
        let _ = (message, callstack_depth);

        #[cfg(feature = "tracy")]
        unsafe {
            // SAFETY: the message outlives the call, which copies it into the profiler queue.
            let () = sys::___tracy_emit_logString(
                sys::TracyMessageSeverity_TracyMessageSeverityInfo as i8,
                0,
                crate::internal::adjust_stack_depth(callstack_depth).into(),
                message.len(),
                message.as_ptr().cast(),
            );
        }
    }

    /// Emits a message with an associated color.
    ///
    /// A non-zero `callstack_depth` also collects a callstack of at most that
    /// many frames for the message. Collecting a callstack adds a non-trivial
    /// amount of overhead to this call.
    ///
    /// The color is RGBA (0xRRGGBBAA), where the least significant 8 bits are
    /// the alpha component and the most significant 8 bits are the red component.
    #[cfg_attr(not(feature = "tracy"), inline(always))]
    pub fn color_message(message: &str, rgba: u32, callstack_depth: u16) {
        #[cfg(not(feature = "tracy"))]
        let _ = (message, rgba, callstack_depth);

        #[cfg(feature = "tracy")]
        unsafe {
            // SAFETY: the message outlives the call, which copies it into the profiler queue.
            let () = sys::___tracy_emit_logString(
                sys::TracyMessageSeverity_TracyMessageSeverityInfo as i8,
                (rgba >> 8) as i32,
                crate::internal::adjust_stack_depth(callstack_depth).into(),
                message.len(),
                message.as_ptr().cast(),
            );
        }
    }
}
