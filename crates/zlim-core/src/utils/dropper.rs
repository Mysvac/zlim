use zlim_ptr::OwningPtr;

// -----------------------------------------------------------------------------
// Dropper

/// Type-erased drop function wrapper for values stored behind [`OwningPtr`].
///
/// `Dropper` stores a monomorphized function pointer that can drop a value of a
/// specific type `T` from an erased pointer. It is typically used in storage
/// internals where concrete types are not known at call sites.
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct Dropper {
    func: unsafe fn(OwningPtr<'_>),
}

impl Dropper {
    /// Drops a value of type `T` from an erased owning pointer.
    ///
    /// # Safety
    /// - `ptr` must point to a valid initialized value of type `T`.
    unsafe fn drop_fn<T>(ptr: OwningPtr<'_>) {
        ptr.debug_assert_aligned::<T>();
        unsafe {
            ptr.drop_as::<T>();
        }
    }

    /// Creates a [`Dropper`] for `T` if `T` needs drop.
    ///
    /// Returns `None` for trivially droppable types (those where
    /// `mem::needs_drop` returns `false`).  This allows storage internals
    /// to skip storing or invoking unnecessary drop callbacks, saving both
    /// memory and CPU cycles.
    ///
    /// This is typically called once per component type during column or
    /// slot initialisation.
    pub const fn of<T>() -> Option<Dropper> {
        if ::core::mem::needs_drop::<T>() {
            Some(Dropper {
                func: Self::drop_fn::<T>,
            })
        } else {
            None
        }
    }

    /// Invokes the stored drop function on `ptr`.
    ///
    /// # Safety
    /// The caller must ensure `ptr` points to a valid initialized value of the
    /// exact type this [`Dropper`] was created for, and that this value is
    /// dropped at most once.
    #[inline(always)]
    pub unsafe fn call(self, ptr: OwningPtr<'_>) {
        unsafe {
            (self.func)(ptr);
        }
    }
}

// -----------------------------------------------------------------------------
