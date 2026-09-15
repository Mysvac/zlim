use core::any::Any;

// -----------------------------------------------------------------------------
// Settings

/// Settings used by the asset system.
pub trait Settings: Any + Send + Sync {}

impl<T: Send + Sync + Any> Settings for T {}

// -----------------------------------------------------------------------------
// dyn Settings

impl dyn Settings {
    /// Returns true if the inner type is the same as `T`.
    #[inline]
    pub fn is<T: Any>(&self) -> bool {
        <dyn Any>::is::<T>(self as &dyn Any)
    }

    /// Attempts to downcast the box to a concrete type.
    #[inline]
    pub fn downcast<T: Any>(self: Box<Self>) -> Result<Box<T>, Box<Self>> {
        if (&*self as &dyn Any).is::<T>() {
            #[expect(unsafe_code, reason = "already checked above")]
            unsafe {
                Ok(<Box<dyn Any>>::downcast::<T>(self).unwrap_unchecked())
            }
        } else {
            Err(self)
        }
    }

    /// Returns some reference to the inner value if it is of type `T`,
    /// or `None` if it isn't.
    #[inline]
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        <dyn Any>::downcast_ref(self as &dyn Any)
    }

    /// Returns some mutable reference to the inner value if it is of type `T`,
    /// or `None` if it isn't.
    #[inline]
    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        <dyn Any>::downcast_mut(self as &mut dyn Any)
    }
}
