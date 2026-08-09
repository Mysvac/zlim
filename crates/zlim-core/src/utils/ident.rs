/// Defines a strongly-typed, niche-optimized ID backed by [`NonMaxU32`].
///
/// The generated type is `#[repr(transparent)]` over a single `NonMaxU32`,
/// meaning [`Option<Id>`] has no size overhead — the niche value
/// (`u32::MAX`) represents `None`.
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
/// [`NonMaxU32`]: zlim_utils::num::NonMaxU32
macro_rules! define_ident {
    ($(#[$id_meta:meta])* $ident:ident) => {
        $(#[$id_meta])*
        #[derive(Clone, Copy, PartialOrd, Ord)]
        #[repr(transparent)]
        pub struct $ident(::zlim_utils::num::NonMaxU32);

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
                debug_assert!(id != u32::MAX);
                unsafe { Self(::zlim_utils::num::NonMaxU32::new_unchecked(id)) }
            }

            /// Creates a new ID from a usize.
            ///
            /// # Panics
            /// Panics if `id >= u32::MAX`.
            #[inline(always)]
            pub const fn without_provenance(id: usize) -> Self {
                if id >= u32::MAX as usize {
                    ::core::hint::cold_path();
                    panic!(concat!(stringify!($ident), " must be < u32::MAX"));
                }
                unsafe { Self(::zlim_utils::num::NonMaxU32::new_unchecked(id as u32)) }
            }

            /// Get the usize corresponding to the ID.
            #[inline(always)]
            pub const fn index(self) -> usize {
                self.0.get() as usize
            }
        }

        impl PartialEq for $ident {
            #[inline(always)]
            fn eq(&self, other: &Self) -> bool {
                self.0 == other.0
            }
        }

        impl Eq for $ident {}

        impl ::core::hash::Hash for $ident {
            #[inline(always)]
            fn hash<H: ::core::hash::Hasher>(&self, state: &mut H) {
                // Sparse hashing is optimized for smaller values.
                // So we use represented values, rather than the underlying bits
                state.write_u32(self.0.get());
            }
        }

        impl ::core::fmt::Debug for $ident {
            #[inline(always)]
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                ::core::fmt::Debug::fmt(&self.0.get(), f)
            }
        }

        impl ::core::fmt::Display for $ident {
            #[inline(always)]
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                ::core::fmt::Display::fmt(&self.0.get(), f)
            }
        }
    };
}

pub(crate) use define_ident;
