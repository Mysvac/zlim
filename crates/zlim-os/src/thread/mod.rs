// -----------------------------------------------------------------------------
// available_parallelism

use core::num::NonZeroUsize;

/// Returns an estimate of the default amount of parallelism a program should use.
///
/// Similar to [`std::thread::available_parallelism`], but in no_std
/// environments (or when the std call fails) this returns `1`.
pub fn available_parallelism() -> NonZeroUsize {
    crate::cfg::switch! {
        crate::cfg::wasm => {
            const { NonZeroUsize::new(1).unwrap() }
        }
        _ => {
            std::thread::available_parallelism()
                .unwrap_or(const { NonZeroUsize::new(1).unwrap() })
        }
    }
}

// -----------------------------------------------------------------------------
