use core::any::TypeId;
use core::panic::Location;

use zlim_utils::mem::Global;

use super::{CtorFunc, TypeDB, TypeDatabase};
use crate::ops::Reflect;

/// Logs a message when the same constructor is registered more than once.
///
/// Uses `debug!` in release mode and `info!` in debug mode.  The original
/// registration is kept; this is purely informational.
#[cold]
#[inline(never)]
fn warn_defaultor_dup(ty: &'static str, l: &'static Location<'static>) {
    #[cfg(not(feature = "debug"))]
    log::debug!("{l}: constructor `fn() -> {ty}` registered repeatedly; ignored.");
    // Upgrade the message level in debug mode.
    #[cfg(feature = "debug")]
    log::info!("{l}: constructor `fn() -> {ty}` registered repeatedly; ignored.");
}

impl TypeDB {
    /// Returns the default value for this type, if a constructor has been
    /// registered via [`register_defaultor`](Self::register_defaultor).
    #[inline]
    pub fn default(&self) -> Option<Box<dyn Reflect>> {
        self.ctor_func.get().map(|f| f())
    }

    /// Returns `true` if a default constructor has been registered for
    /// this type.
    #[inline]
    pub fn contains_defaultor(&self) -> bool {
        self.ctor_func.get().is_some()
    }

    /// Inserts a default constructor for type `T` into `self`.
    ///
    /// The constructor is stored and can be invoked via [`TypeDB::default`].
    ///
    /// # Panics
    ///
    /// Panics if `self` does not belong to type `T` (i.e.
    /// `self.type_id() != TypeId::of::<T>()`).
    ///
    /// # Returns
    ///
    /// `true` on first registration, `false` if a constructor was already
    /// registered (a message is logged and the original is kept).
    #[cold]
    #[track_caller]
    #[inline(never)]
    pub fn insert_defaultor<T, F>(&self, f: F) -> bool
    where
        T: TypeDatabase,
        F: Copy + Sync + 'static + Fn() -> T,
    {
        #[cold]
        #[inline(never)]
        fn panicked(e: &'static str, a: &'static str, l: &'static Location<'static>) -> ! {
            panic!(
                "{l}: `insert_defaultor` type mismatch — TypeDB is \
                for `{e}`, but the constructor produces `{a}`."
            )
        }

        if self.id != TypeId::of::<T>() {
            panicked(self.type_path, T::type_path(), Location::caller());
        }

        let func = move || Box::new(f()) as Box<dyn Reflect>;
        let fun: CtorFunc = Global::alloc_value(func);

        if self.ctor_func.set(fun).is_err() {
            warn_defaultor_dup(T::type_path(), Location::caller());
            false
        } else {
            true
        }
    }

    /// Convenience wrapper: resolves `T`'s [`TypeDB`] via [`TypeDB::of`]
    /// then calls [`insert_defaultor`](Self::insert_defaultor).
    ///
    /// # Return
    ///
    /// Returns `true` on first registration, `false` if a constructor was
    /// already registered.
    #[cold]
    #[track_caller]
    pub fn register_defaultor<T, F>(f: F) -> bool
    where
        T: TypeDatabase,
        F: Copy + Sync + 'static + Fn() -> T,
    {
        let db = TypeDB::of::<T>();
        db.insert_defaultor(f)
    }
}
