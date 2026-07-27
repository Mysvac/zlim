/// Defines a 32-bit id type which guarantees global uniqueness via atomics on a static global.
///
/// Note that this means the id space is process-wide, as such it may potentially be exhausted
/// by a combination of long-running processes and multiple `World`s, at which point we panic.
///
/// # Examples
///
/// ```
/// # use zlim_utils::define_atomic_id;
/// define_atomic_id!(UserId);
///
/// let id_1 = UserId::alloc();
/// let id_2 = UserId::alloc();
///
/// assert_eq!(id_1, id_1);
/// assert_ne!(id_1, id_2);
/// ```
#[macro_export]
macro_rules! define_atomic_id {
    ($atomic_id_type:ident) => {
        /// Globally unique 32-bit id, guaranteed via atomics on a static global.
        ///
        /// Note that this means the id space is process-wide, as such it may potentially be exhausted
        /// by a combination of long-running processes and multiple `World`s, at which point we panic.
        #[derive(Copy, Clone, Hash, Eq, PartialEq, PartialOrd, Ord, Debug)]
        #[repr(transparent)]
        pub struct $atomic_id_type(::core::num::NonZeroU32);

        impl $atomic_id_type {
            /// Creates a new id via fetch_add atomic on a static global.
            #[inline]
            pub fn alloc() -> Self {
                #[cold]
                #[inline(never)]
                fn overflow() -> ! {
                    panic!(concat!("too many `", stringify!($atomic_id_type), "`s"))
                }

                use ::core::sync::atomic::AtomicU32;
                use ::core::sync::atomic::Ordering::Relaxed;

                static COUNTER: AtomicU32 = AtomicU32::new(1);

                let id = COUNTER
                    .try_update(Relaxed, Relaxed, |val| val.checked_add(1))
                    .ok() // `1..u32::MAX`
                    .and_then(::core::num::NonZeroU32::new)
                    .unwrap_or_else(|| overflow());

                Self(id)
            }
        }

        impl From<$atomic_id_type> for ::core::num::NonZeroU32 {
            #[inline]
            fn from(value: $atomic_id_type) -> Self {
                value.0
            }
        }

        impl From<::core::num::NonZeroU32> for $atomic_id_type {
            #[inline]
            fn from(value: ::core::num::NonZeroU32) -> Self {
                Self(value)
            }
        }
    };
}
