/// A debug checked version of [`Option::unwrap_unchecked`].
///
/// This trait offers a "debug fail-fast, release unchecked" pattern:
/// - In debug-style builds, invalid unwraps trigger `unreachable!()` to surface
///   invariants early.
/// - In release-style builds, it maps to unchecked unwrap for maximum speed.
///
/// Implemented for [`Option`] and [`Result`].
pub trait DebugCheckedUnwrap {
    type Item;

    /// Returns the inner value under a caller-validated invariant.
    ///
    /// # Safety
    /// Must only be called on:
    /// - `Some(_)` for [`Option`]
    /// - `Ok(_)` for [`Result`]
    ///
    /// Calling this on `None`/`Err` is immediate undefined behavior in release
    /// builds and will panic in debug-style builds.
    unsafe fn debug_checked_unwrap(self) -> Self::Item;
}

impl<T> DebugCheckedUnwrap for Option<T> {
    type Item = T;

    crate::cfg::debug! {
        if {
            #[inline(always)]
            #[track_caller]
            unsafe fn debug_checked_unwrap(self) -> Self::Item {
                if let Some(inner) = self {
                    inner
                } else {
                    unreachable!()
                }
            }
        } else {
            #[inline(always)]
            unsafe fn debug_checked_unwrap(self) -> Self::Item {
                unsafe { self.unwrap_unchecked() }
            }
        }
    }
}

impl<T, U> DebugCheckedUnwrap for Result<T, U> {
    type Item = T;

    crate::cfg::debug! {
        if {
            #[inline(always)]
            #[track_caller]
            unsafe fn debug_checked_unwrap(self) -> Self::Item {
                if let Ok(inner) = self {
                    inner
                } else {
                    unreachable!()
                }
            }
        } else {
            #[inline(always)]
            unsafe fn debug_checked_unwrap(self) -> Self::Item {
                unsafe { self.unwrap_unchecked() }
            }
        }
    }
}
