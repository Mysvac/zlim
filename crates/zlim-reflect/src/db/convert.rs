use core::any::TypeId;
use core::panic::Location;
use std::sync::PoisonError;

use zlim_utils::mem::Global;

use super::{IntoFunc, TypeDB, TypeDatabase};
use crate::ops::Reflect;

/// Logs a message when the same convertor is registered more than once.
///
/// Uses `debug!` in release mode and `info!` in debug mode.  The original
/// registration is kept; this is purely informational.
#[cold]
#[inline(never)]
fn warn_convertor_dup(from: &'static str, into: &'static str, l: &'static Location<'static>) {
    #[cfg(not(feature = "debug"))]
    log::debug!("{l}: convertor `{from} -> {into}` registered repeatedly; ignored.");
    // Upgrade the message level in debug mode.
    #[cfg(feature = "debug")]
    log::info!("{l}: convertor `{from} -> {into}` registered repeatedly; ignored.");
}

/// Panics when `insert_convertor` is called on a [`TypeDB`] that belongs to
/// neither the `From` nor the `Into` type.
///
/// This indicates a logic error: the caller should either call
/// `insert_convertor` on the `From` type's `TypeDB`, or use
/// [`register_convertor`](TypeDB::register_convertor) which infers the
/// correct `TypeDB` automatically.
#[cold]
#[inline(never)]
fn panic_invalid_convert(
    from: &'static str,
    into: &'static str,
    l: &'static Location<'static>,
) -> ! {
    panic!(
        "{l}: `insert_convertor` type mismatch — \
        convertor is `{from}` -> `{into}`, but this TypeDB belongs to \
        neither type. Call `insert_convertor` on the `From` type's \
        TypeDB, or use `TypeDB::register_convertor` instead."
    )
}

impl TypeDB {
    /// Converts a boxed reflected value to another type registered in the
    /// database.
    ///
    /// Returns `Err(from)` if no conversion function to `to` has been
    /// registered via [`register_convertor`](Self::register_convertor).
    #[inline(never)]
    pub fn convert(
        &self,
        from: Box<dyn Reflect>,
        to: TypeId,
    ) -> Result<Box<dyn Reflect>, Box<dyn Reflect>> {
        match self
            .into_func
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(to)
            .copied()
        {
            Some(f) => Ok(f(from)),
            None => Err(from),
        }
    }

    /// Returns `true` if a conversion function to the given type has been
    /// registered.
    #[inline]
    pub fn contains_convertor(&self, to: TypeId) -> bool {
        self.into_func
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(to)
            .is_some()
    }

    /// Inserts a conversion function from `From` to `Into` into `self`.
    ///
    /// The function is stored in `self`'s convertor table. When
    /// [`TypeDB::convert`] is called with `Into`'s [`TypeId`],
    /// the stored function is invoked.
    ///
    /// `self` must be the [`TypeDB`] of `From`. If `self` belongs to a
    /// different type but `Into` matches, the call is forwarded to
    /// [`register_convertor`] automatically.
    ///
    /// # Panics
    ///
    /// Panics if `self` belongs to **neither** `From` nor `Into` — the
    /// convertor has no relationship to this [`TypeDB`]. In that case, call
    /// [`register_convertor`] instead.
    ///
    /// Also panics if the boxed value later passed to the conversion
    /// function (via [`TypeDB::convert`]) is not actually of type
    /// `From`.
    ///
    /// # Returns
    ///
    /// `true` on first registration, `false` if a convertor for `Into` was
    /// already registered (a message is logged and the original is kept).
    ///
    /// [`register_convertor`]: Self::register_convertor
    #[cold]
    #[track_caller]
    pub fn insert_convertor<From, Into, F>(&self, f: F) -> bool
    where
        From: TypeDatabase,
        Into: TypeDatabase,
        F: Copy + Sync + 'static + Fn(From) -> Into,
    {
        if self.id != TypeId::of::<From>() {
            ::core::hint::cold_path();
            if self.id != TypeId::of::<Into>() {
                ::core::hint::cold_path();
                panic_invalid_convert(From::type_path(), Into::type_path(), Location::caller());
            }
            return TypeDB::register_convertor(f);
        }

        // Fast-path: already registered → skip allocation.
        if self
            .into_func
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(TypeId::of::<Into>())
        {
            warn_convertor_dup(From::type_path(), Into::type_path(), Location::caller());
            return false;
        }

        let func = move |from: Box<dyn Reflect>| -> Box<dyn Reflect> {
            match from.take::<From>() {
                Ok(v) => Box::new(f(v)),
                Err(e) => {
                    ::core::hint::cold_path();
                    panic!(
                        "Convert type mismatched, From Type `{}` with Input Type: {}",
                        From::type_path(),
                        e.reflect_type_path(),
                    );
                }
            }
        };

        let into_func: IntoFunc = Global::alloc_value(func);

        let inserted = self
            .into_func
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .try_insert(TypeId::of::<Into>(), || into_func);

        // Race: another thread inserted between our read check and write lock.
        if !inserted {
            warn_convertor_dup(From::type_path(), Into::type_path(), Location::caller());
        }

        inserted
    }

    /// Registers a conversion function from `From` to `Into` in the global
    /// type database.
    ///
    /// This is a **static method** — it automatically resolves `From`'s
    /// [`TypeDB`] via [`TypeDB::of`] and calls [`insert_convertor`] on it.
    ///
    /// Prefer this over [`insert_convertor`] when you don't already hold
    /// a `&TypeDB` reference.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use zlim_reflect::db::TypeDB;
    ///
    /// // Register a conversion from u32 to i32.
    /// TypeDB::register_convertor::<u32, i32, _>(|x| x as i32);
    /// ```
    ///
    /// # Panics
    ///
    /// Same panic conditions as [`insert_convertor`] — since this
    /// resolves to `From`'s [`TypeDB`], the type-mismatch panic
    /// should never trigger through this path.
    ///
    /// # Returns
    ///
    /// `true` on first registration, `false` if a convertor for `Into` was
    /// already registered for this `From` type (a message is logged and
    /// the original is kept).
    ///
    /// [`insert_convertor`]: Self::insert_convertor
    #[cold]
    #[track_caller]
    pub fn register_convertor<From, Into, F>(f: F) -> bool
    where
        From: TypeDatabase,
        Into: TypeDatabase,
        F: Copy + Sync + 'static + Fn(From) -> Into,
    {
        let db = TypeDB::of::<From>();
        db.insert_convertor(f)
    }
}
