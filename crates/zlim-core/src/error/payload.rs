use core::any::Any;

/// A wrapper around a panic payload that adds additional error output for unexpected panics.
///
/// When an `ErrorHandler` throws a panic, the error message typically contains an
/// `ErrorContext`, providing clear and sufficient information.
///
/// However, for unexpected panics — such as those originating from `unwrap()` inside
/// a `System` or `Command` — the upper-layer context is lost.
///
/// This type is used during `Schedule` and `Command` execution to inspect captured panics.
/// If the captured panic is an unexpected error, it outputs an optional extra message
/// to the standard error stream. If it is already a `PanicPayload`, it propagates
/// directly without additional output.
pub struct PanicPayload {
    pub payload: Box<dyn Any + Send>,
}

impl PanicPayload {
    /// Takes a panic payload and wraps it in a `PanicPayload`, while optionally
    /// outputting an additional message if the payload is not already a `PanicPayload`.
    ///
    /// If the payload is already a `PanicPayload`, it is returned as-is.
    /// Otherwise, the `addtion` closure is called to produce an extra message,
    /// which is printed to stderr before wrapping the original payload.
    pub fn take_payload(
        payload: Box<dyn Any + Send>,
        addtion: impl FnOnce() -> String,
    ) -> Box<PanicPayload> {
        match payload.downcast::<PanicPayload>() {
            Ok(panic_payload) => panic_payload,
            #[expect(clippy::print_stderr, reason = "panic outout")]
            Err(payload) => {
                ::core::hint::cold_path();
                std::eprintln!("{}", addtion());
                Box::new(PanicPayload { payload })
            }
        }
    }

    /// Resumes a panic payload, optionally outputting an additional message
    /// if the payload is not already a `PanicPayload`.
    ///
    /// If the payload is already a `PanicPayload`, it is resumed directly.
    /// Otherwise, the `addtion` closure is called to produce an extra message,
    /// which is printed to stderr before wrapping the original payload in a
    /// `PanicPayload` and resuming it.
    ///
    /// This function always panics (returns `!`).
    pub fn resume_payload(payload: Box<dyn Any + Send>, addtion: impl FnOnce() -> String) -> ! {
        match payload.downcast::<PanicPayload>() {
            Ok(panic_payload) => std::panic::resume_unwind(panic_payload),
            #[expect(clippy::print_stderr, reason = "panic outout")]
            Err(payload) => {
                ::core::hint::cold_path();
                std::eprintln!("{}", addtion());
                std::panic::resume_unwind(Box::new(PanicPayload { payload }))
            }
        }
    }
}
