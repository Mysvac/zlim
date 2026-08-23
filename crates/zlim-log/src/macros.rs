// Not using `zlim_utils`, hoping to speed up compilation (parallelization).

/// Call [`trace!`](crate::trace) once per call site.
///
/// Useful for logging within systems which are called every frame.
#[macro_export]
macro_rules! trace_once {
    ($($arg:tt)+) => ({
        static ONCE: ::core::sync::atomic::AtomicBool = ::core::sync::atomic::AtomicBool::new(true);
        if ONCE.swap(false, ::core::sync::atomic::Ordering::Relaxed) { $crate::trace!($($arg)+); }
    });
}

/// Call [`debug!`](crate::debug) once per call site.
///
/// Useful for logging within systems which are called every frame.
#[macro_export]
macro_rules! debug_once {
    ($($arg:tt)+) => ({
        static ONCE: ::core::sync::atomic::AtomicBool = ::core::sync::atomic::AtomicBool::new(true);
        if ONCE.swap(false, ::core::sync::atomic::Ordering::Relaxed) { $crate::debug!($($arg)+); }
    });
}

/// Call [`info!`](crate::info) once per call site.
///
/// Useful for logging within systems which are called every frame.
#[macro_export]
macro_rules! info_once {
    ($($arg:tt)+) => ({
        static ONCE: ::core::sync::atomic::AtomicBool = ::core::sync::atomic::AtomicBool::new(true);
        if ONCE.swap(false, ::core::sync::atomic::Ordering::Relaxed) { $crate::info!($($arg)+); }
    });
}

/// Call [`warn!`](crate::warn) once per call site.
///
/// Useful for logging within systems which are called every frame.
#[macro_export]
macro_rules! warn_once {
    ($($arg:tt)+) => ({
        static ONCE: ::core::sync::atomic::AtomicBool = ::core::sync::atomic::AtomicBool::new(true);
        if ONCE.swap(false, ::core::sync::atomic::Ordering::Relaxed) { $crate::warn!($($arg)+); }
    });
}

/// Call [`error!`](crate::error) once per call site.
///
/// Useful for logging within systems which are called every frame.
#[macro_export]
macro_rules! error_once {
    ($($arg:tt)+) => ({
        static ONCE: ::core::sync::atomic::AtomicBool = ::core::sync::atomic::AtomicBool::new(true);
        if ONCE.swap(false, ::core::sync::atomic::Ordering::Relaxed) { $crate::error!($($arg)+); }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn simple() {
        crate::trace_once!("");
        crate::debug_once!("");
        crate::info_once!("");
        crate::warn_once!("");
        crate::error_once!("");
    }
}
