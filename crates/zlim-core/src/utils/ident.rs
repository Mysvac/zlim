/// Defines a strongly-typed, niche-optimized ID backed by [`NonZeroU32`].
///
/// The generated type is `#[repr(transparent)]` over a single `NonZeroU32`,
/// meaning [`Option<Id>`] has no size overhead — the niche value (`u32::MAX`)
/// represents `None`.
///
/// # Generated API
///
/// | Method / impl        | Description                                   |
/// |----------------------|-----------------------------------------------|
/// | `Clone`, `Copy`      | Cheap bitwise copy.                           |
/// | `PartialEq`, `Eq`    | Value equality.                               |
/// | `PartialOrd`, `Ord`  | Total ordering (delegates to the inner `u32`). |
/// | `Hash`               | Hashes the inner `u32` via `write_u32`.       |
/// | `Debug`, `Display`   | Formats the inner `u32` value.                |
/// | `without_provenance` | Construct from `usize`; panics if ≥ `u32::MAX`.|
/// | `index`              | Return the inner value as `usize`.            |
///
/// [`NonZeroU32`]: core::num::NonMaxU32
macro_rules! define_ident {
    ($(#[$id_meta:meta])* $ident:ident) => {
        $(#[$id_meta])*
        #[derive(Clone, Copy, PartialEq, Eq)]
        #[repr(transparent)]
        pub struct $ident(::core::num::NonZeroU32);

        impl $ident {
            /// Create a new ID without bound checking.
            ///
            /// # Safety
            ///
            /// `id != u32::MAX`
            #[expect(clippy::allow_attributes, reason = "allow unused function")]
            #[allow(unused, reason = "Some types may not require this function")]
            #[inline(always)]
            pub(crate) const unsafe fn new(id: u32) -> Self {
                ::core::debug_assert!(id != u32::MAX);
                unsafe { ::core::mem::transmute::<u32, Self>(id ^ u32::MAX) }
            }

            /// Creates a new ID from a usize.
            ///
            /// # Panics
            /// Panics if `id >= u32::MAX`.
            #[inline(always)]
            pub const fn without_provenance(id: usize) -> Self {
                #[cold]
                #[inline(never)]
                const fn overflow() -> ! {
                    ::core::panic!(::core::concat!(::core::stringify!($ident), " must be < u32::MAX"));
                }

                if id >= u32::MAX as usize {
                    overflow();
                }

                unsafe { ::core::mem::transmute::<u32, Self>((id as u32) ^ u32::MAX) }
            }

            /// Get the usize corresponding to the ID.
            #[inline(always)]
            pub const fn get(self) -> u32 {
                unsafe { ::core::mem::transmute::<Self, u32>(self) ^ u32::MAX }
            }

            /// Get the usize corresponding to the ID.
            #[inline(always)]
            pub const fn index(self) -> usize {
                self.get() as usize
            }
        }

        impl ::core::hash::Hash for $ident {
            #[inline(always)]
            fn hash<H: ::core::hash::Hasher>(&self, state: &mut H) {
                // Sparse hashing is optimized for smaller values.
                // So we use represented values, rather than the underlying bits
                state.write_u32(self.get());
            }
        }

        impl ::core::fmt::Debug for $ident {
            #[inline(always)]
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                ::core::fmt::Debug::fmt(&self.get(), f)
            }
        }

        impl ::core::fmt::Display for $ident {
            #[inline(always)]
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                ::core::fmt::Display::fmt(&self.get(), f)
            }
        }

        impl PartialOrd for $ident {
            #[inline]
            fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
                Some(self.cmp(other))
            }

            #[inline]
            fn lt(&self, other: &Self) -> bool {
                self.get() < other.get()
            }

            #[inline]
            fn le(&self, other: &Self) -> bool {
                self.get() <= other.get()
            }

            #[inline]
            fn gt(&self, other: &Self) -> bool {
                self.get() > other.get()
            }

            #[inline]
            fn ge(&self, other: &Self) -> bool {
                self.get() >= other.get()
            }
        }

        impl Ord for $ident {
            #[inline]
            fn cmp(&self, other: &Self) -> ::core::cmp::Ordering {
                self.get().cmp(&other.get())
            }

            #[inline]
            fn max(self, other: Self) -> Self {
                // SAFETY: The maximum of two non-max values is still non-max.
                unsafe { Self::new(self.get().max(other.get())) }
            }

            #[inline]
            fn min(self, other: Self) -> Self {
                // SAFETY: The minimum of two non-max values is still non-max.
                unsafe { Self::new(self.get().min(other.get())) }
            }

            #[inline]
            fn clamp(self, min: Self, max: Self) -> Self {
                // SAFETY: A non-max value clamped between two non-max values is still non-max.
                unsafe { Self::new(self.get().clamp(min.get(), max.get())) }
            }
        }
    };
}

pub(crate) use define_ident;
