use core::ops::{Deref, DerefMut};

use crate::world::FromWorld;

/// A system-local variable.
///
/// When used as a system parameter, each compiled system instance owns one
/// independent value of `T`. This makes `Local<T>` a convenient alternative to
/// global `static` state for per-system counters, caches, and temporary state.
///
/// The value is initialized from `T::default()` during system initialization
/// and then reused across subsequent runs of that system.
///
/// # Examples
///
/// ```ignore
/// fn system(mut counter: Local<u64>) {
///     *counter += 1;
/// }
/// ```
#[derive(Debug)]
#[repr(transparent)]
pub struct Local<'s, T: FromWorld + Send + Sync>(&'s mut T);

impl<'s, T: FromWorld + Send + Sync> Deref for Local<'s, T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<'s, T: FromWorld + Send + Sync> DerefMut for Local<'s, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}
