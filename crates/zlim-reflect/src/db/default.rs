use core::any::TypeId;

use zlim_log as log;
use zlim_utils::debug::DebugLocation;

use super::{TypeDB, TypeDatabase};
use crate::ops::Reflect;

/// Logs a message when the same constructor is registered more than once.
#[cold]
#[inline(never)]
fn warn_defaultor_dup(ty: &'static str, l: DebugLocation) {
    log::trace!("{l}: constructor `fn() -> {ty}` registered repeatedly; ignored.");
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

    /// Inserts the [`Default`] constructor for type `T` into `self`.
    ///
    /// Nothing is stored beyond a monomorphized function pointer — the whole
    /// constructor is `<T as Default>::default`, exactly as `insert_serializer`
    /// stores an instantiation of `Serialize` — so no allocation is needed.
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
    #[inline(never)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn insert_defaultor<T>(&self) -> bool
    where
        T: TypeDatabase + Default,
    {
        #[cold]
        #[inline(never)]
        fn panicked(e: &'static str, a: &'static str, l: DebugLocation) -> ! {
            panic!(
                "{l}: `insert_defaultor` type mismatch — TypeDB is \
                for `{e}`, but the constructor produces `{a}`."
            )
        }

        if self.id != TypeId::of::<T>() {
            panicked(self.type_path, T::type_path(), DebugLocation::caller());
        }

        if self
            .ctor_func
            .set(|| Box::new(T::default()) as Box<dyn Reflect>)
            .is_err()
        {
            warn_defaultor_dup(T::type_path(), DebugLocation::caller());
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
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn register_defaultor<T>() -> bool
    where
        T: TypeDatabase + Default,
    {
        TypeDB::of::<T>().insert_defaultor::<T>()
    }
}
