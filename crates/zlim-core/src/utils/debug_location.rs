use core::fmt::{Debug, Display};
use core::marker::PhantomData;

use core::panic::Location;

// -----------------------------------------------------------------------------
// DebugLocation

/// A wrapper type that provides [`Location`] information in debug mode.
///
/// In `debug_assertions` builds (or with feature `debug`), this stores
/// `Location::caller()` and prints it through [`Display`] / [`Debug`].
/// In release-like builds, it becomes a near-zero-cost placeholder.
///
/// This type is commonly used in logging and panic diagnostics where call-site
/// context is useful during development but should not bloat release output.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct DebugLocation(
    PhantomData<&'static Location<'static>>,
    #[cfg(any(debug_assertions, feature = "debug"))] &'static Location<'static>,
);

// -----------------------------------------------------------------------------
// Methods

impl Display for DebugLocation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        #[cfg(any(debug_assertions, feature = "debug"))]
        Display::fmt(self.1, f)?;

        Ok(())
    }
}

impl Debug for DebugLocation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        #[cfg(any(debug_assertions, feature = "debug"))]
        Debug::fmt(self.1, f)?;

        Ok(())
    }
}

impl DebugLocation {
    /// Captures the current call-site location.
    ///
    /// The returned value carries caller metadata in debug-style builds, and a
    /// lightweight placeholder in release-style builds.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub const fn caller() -> Self {
        Self(
            PhantomData,
            #[cfg(any(debug_assertions, feature = "debug"))]
            Location::caller(),
        )
    }
}

// -----------------------------------------------------------------------------
