//! Numeric utility types.
//!
//! This module currently provides a family of `NonMax*` integer wrappers.
//! A `NonMax*` value guarantees that it is never equal to the maximum value
//! of its underlying integer type.
//!
//! # Why `NonMax`?
//!
//! These types are useful for niche-value optimization, similar to `NonZero*`.
//! Because `MAX` is reserved as an invalid state, `Option<NonMax*>` can often
//! have the same size as the primitive integer.
//!
//! ```no_run
//! use core::mem::size_of;
//! use zlim_utils::num::{NonMaxI32, NonMaxU32};
//!
//! assert_eq!(size_of::<Option<NonMaxU32>>(), size_of::<u32>());
//! assert_eq!(size_of::<Option<NonMaxI32>>(), size_of::<i32>());
//! ```
//!
//! # Available Types
//!
//! - Unsigned: `NonMaxU8`, `NonMaxU16`, `NonMaxU32`, `NonMaxU64`, `NonMaxU128`, `NonMaxUsize`
//! - Signed: `NonMaxI8`, `NonMaxI16`, `NonMaxI32`, `NonMaxI64`, `NonMaxI128`, `NonMaxIsize`
//!
//! # Basic Usage
//!
//! ```no_run
//! use zlim_utils::num::NonMaxU8;
//!
//! let n = NonMaxU8::new(42).unwrap();
//! assert_eq!(n.get(), 42);
//!
//! assert!(NonMaxU8::new(u8::MAX).is_none());
//! ```
#![expect(unsafe_code, reason = "transmute is unsafe")]

use core::cmp::Ordering;
use core::fmt::{Binary, Debug, Display, LowerExp};
use core::fmt::{LowerHex, Octal, UpperExp, UpperHex};
use core::hash::{Hash, Hasher};
use core::mem;
use core::num::NonZero;
use core::num::ParseIntError;
use core::str::FromStr;

macro_rules! impl_non_max {
    ($NonMax:ident, $Int:ty) => {
        /// An integer that is known not to equal its maximum value.
        ///
        /// This enables some memory layout optimization.
        /// For example, `Option<NonMaxU32>` is the same size as `u32`:
        ///
        /// ```no_run
        /// use zlim_utils::num::NonMaxU32;
        /// use std::mem::size_of;
        ///
        /// assert_eq!(size_of::<Option<NonMaxU32>>(), size_of::<u32>());
        /// ```
        ///
        /// Unlike NonZero, NonMax has some overhead; retrieving the target value
        /// requires one XOR operation.
        ///
        /// We guarantee the stability of the underlying implementation, therefore:
        /// - `transmute::<NonMax<T>, T>(nonmax) ^ T::MAX == nonmax.get()`.
        #[derive(Copy, Clone, PartialEq, Eq)] // Use derive to support compile-time operation.
        #[repr(transparent)]
        pub struct $NonMax(NonZero<$Int>);

        impl $NonMax {
            /// The value `0`, represented as a non-max value.
            pub const ZERO: $NonMax =
                unsafe { mem::transmute::<$Int, $NonMax>((0 as $Int) ^ <$Int>::MAX) };

            /// The maximum value that can be represented by this type.
            /// This is equivalent to `<Int>::MAX - 1`.
            pub const MAX: $NonMax =
                unsafe { mem::transmute::<$Int, $NonMax>((<$Int>::MAX - 1) ^ <$Int>::MAX) };

            /// The minimum value that can be represented by this type.
            /// This is equivalent to `<Int>::MIN`.
            pub const MIN: $NonMax =
                unsafe { mem::transmute::<$Int, $NonMax>(<$Int>::MIN ^ <$Int>::MAX) };

            /// The size of this integer type in bits.
            pub const BITS: u32 = <$Int>::BITS;

            /// Creates a non-max value if the given value is not the maximum value
            /// of the underlying integer type.
            ///
            /// Returns `None` if `n == <Int>::MAX`.
            #[must_use]
            #[inline(always)]
            pub const fn new(n: $Int) -> Option<Self> {
                match NonZero::<$Int>::new(n ^ <$Int>::MAX) {
                    Some(inner) => Some(Self(inner)),
                    None => None,
                }
            }

            /// Creates a non-max value without checking whether the value is the maximum.
            ///
            /// # Safety
            /// The value must not be the maximum value of the underlying integer type.
            #[must_use]
            #[inline(always)]
            pub const unsafe fn new_unchecked(n: $Int) -> Self {
                debug_assert!(n != <$Int>::MAX);
                unsafe { mem::transmute::<$Int, $NonMax>(n ^ <$Int>::MAX) }
            }

            /// Returns the value as the primitive integer type.
            #[inline(always)]
            pub const fn get(self) -> $Int {
                unsafe { mem::transmute::<$NonMax, $Int>(self) ^ <$Int>::MAX }
            }
        }

        impl Default for $NonMax {
            #[inline(always)]
            fn default() -> Self {
                // SAFETY: `0 != MAX`
                unsafe { Self::new_unchecked(0) }
            }
        }

        impl PartialOrd for $NonMax {
            #[inline]
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
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

        impl Ord for $NonMax {
            #[inline]
            fn cmp(&self, other: &Self) -> Ordering {
                self.get().cmp(&other.get())
            }

            #[inline]
            fn max(self, other: Self) -> Self {
                // SAFETY: The maximum of two non-max values is still non-max.
                unsafe { Self::new_unchecked(self.get().max(other.get())) }
            }

            #[inline]
            fn min(self, other: Self) -> Self {
                // SAFETY: The minimum of two non-max values is still non-max.
                unsafe { Self::new_unchecked(self.get().min(other.get())) }
            }

            #[inline]
            fn clamp(self, min: Self, max: Self) -> Self {
                // SAFETY: A non-max value clamped between two non-max values is still non-max.
                unsafe { Self::new_unchecked(self.get().clamp(min.get(), max.get())) }
            }
        }

        impl Hash for $NonMax {
            #[inline]
            fn hash<H: Hasher>(&self, state: &mut H) {
                self.get().hash(state)
            }
        }

        impl From<$NonMax> for $Int {
            #[inline]
            fn from(nonmax: $NonMax) -> Self {
                nonmax.get()
            }
        }

        impl Debug for $NonMax {
            #[inline]
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                Debug::fmt(&self.get(), f)
            }
        }

        impl Display for $NonMax {
            #[inline]
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                Display::fmt(&self.get(), f)
            }
        }

        impl Binary for $NonMax {
            #[inline]
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                Binary::fmt(&self.get(), f)
            }
        }

        impl Octal for $NonMax {
            #[inline]
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                Octal::fmt(&self.get(), f)
            }
        }

        impl LowerHex for $NonMax {
            #[inline]
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                LowerHex::fmt(&self.get(), f)
            }
        }

        impl UpperHex for $NonMax {
            #[inline]
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                UpperHex::fmt(&self.get(), f)
            }
        }

        impl LowerExp for $NonMax {
            #[inline]
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                LowerExp::fmt(&self.get(), f)
            }
        }

        impl UpperExp for $NonMax {
            #[inline]
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                UpperExp::fmt(&self.get(), f)
            }
        }

        impl FromStr for $NonMax {
            type Err = ParseIntError;

            fn from_str(src: &str) -> Result<Self, Self::Err> {
                match src.parse::<$Int>() {
                    Ok(x) => match Self::new(x) {
                        Some(r) => Ok(r),
                        None => {
                            ::core::hint::cold_path();
                            // ↓ A hack, to create a `ParseIntError`.
                            const {
                                match u8::from_str_radix("-1", 10) {
                                    Ok(_) => unreachable!(),
                                    Err(e) => Err(e),
                                }
                            }
                        }
                    },
                    Err(e) => Err(e),
                }
            }
        }

        impl serde_core::Serialize for $NonMax {
            #[inline]
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde_core::Serializer,
            {
                self.get().serialize(serializer)
            }
        }

        impl<'de> serde_core::Deserialize<'de> for $NonMax {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde_core::Deserializer<'de>,
            {
                let value = <$Int as serde_core::Deserialize>::deserialize(deserializer)?;

                Self::new(value).ok_or_else(|| {
                    core::hint::cold_path();
                    serde_core::de::Error::custom(format_args!(
                        "invalid type: {}, expected < {}",
                        value,
                        <$Int>::MAX
                    ))
                })
            }
        }
    };
}

impl_non_max!(NonMaxU8, u8);
impl_non_max!(NonMaxU16, u16);
impl_non_max!(NonMaxU32, u32);
impl_non_max!(NonMaxU64, u64);
impl_non_max!(NonMaxU128, u128);
impl_non_max!(NonMaxUsize, usize);

impl_non_max!(NonMaxI8, i8);
impl_non_max!(NonMaxI16, i16);
impl_non_max!(NonMaxI32, i32);
impl_non_max!(NonMaxI64, i64);
impl_non_max!(NonMaxI128, i128);
impl_non_max!(NonMaxIsize, isize);

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    #[test]
    fn size_optimization() {
        assert_eq!(size_of::<Option<NonMaxU8>>(), size_of::<u8>());
        assert_eq!(size_of::<Option<NonMaxI8>>(), size_of::<i8>());
        assert_eq!(size_of::<Option<NonMaxU16>>(), size_of::<u16>());
        assert_eq!(size_of::<Option<NonMaxI16>>(), size_of::<i16>());
        assert_eq!(size_of::<Option<NonMaxU32>>(), size_of::<u32>());
        assert_eq!(size_of::<Option<NonMaxI32>>(), size_of::<i32>());
        assert_eq!(size_of::<Option<NonMaxU64>>(), size_of::<u64>());
        assert_eq!(size_of::<Option<NonMaxI64>>(), size_of::<i64>());
        assert_eq!(size_of::<Option<NonMaxUsize>>(), size_of::<usize>());
        assert_eq!(size_of::<Option<NonMaxIsize>>(), size_of::<isize>());
    }

    #[test]
    fn constants() {
        // Unsigned
        assert_eq!(NonMaxU8::ZERO.get(), 0u8);
        assert_eq!(NonMaxU8::MAX.get(), u8::MAX - 1);
        assert_eq!(NonMaxU8::MIN.get(), u8::MIN);
        assert_eq!(NonMaxU8::BITS, u8::BITS);

        assert_eq!(NonMaxU32::ZERO.get(), 0u32);
        assert_eq!(NonMaxU32::MAX.get(), u32::MAX - 1);
        assert_eq!(NonMaxU32::MIN.get(), u32::MIN);
        assert_eq!(NonMaxU32::BITS, u32::BITS);

        // Signed
        assert_eq!(NonMaxI8::ZERO.get(), 0i8);
        assert_eq!(NonMaxI8::MAX.get(), i8::MAX - 1);
        assert_eq!(NonMaxI8::MIN.get(), i8::MIN);
        assert_eq!(NonMaxI8::BITS, i8::BITS);

        assert_eq!(NonMaxI32::ZERO.get(), 0i32);
        assert_eq!(NonMaxI32::MAX.get(), i32::MAX - 1);
        assert_eq!(NonMaxI32::MIN.get(), i32::MIN);
        assert_eq!(NonMaxI32::BITS, i32::BITS);
    }

    #[test]
    fn new_and_get() {
        unsafe {
            assert_eq!(NonMaxU8::new_unchecked(0).get(), 0);
            assert_eq!(NonMaxU8::new_unchecked(42).get(), 42);
            assert_eq!(NonMaxU8::new_unchecked(u8::MAX - 1).get(), u8::MAX - 1);

            assert_eq!(NonMaxI8::new_unchecked(-128).get(), -128);
            assert_eq!(NonMaxI8::new_unchecked(-1).get(), -1);
            assert_eq!(NonMaxI8::new_unchecked(0).get(), 0);
            assert_eq!(NonMaxI8::new_unchecked(126).get(), 126);
        }

        // Unsigned - valid values
        assert_eq!(NonMaxU8::new(0).unwrap().get(), 0);
        assert_eq!(NonMaxU8::new(42).unwrap().get(), 42);
        assert_eq!(NonMaxU8::new(u8::MAX - 1).unwrap().get(), u8::MAX - 1);

        // Unsigned - invalid value (MAX)
        assert!(NonMaxU8::new(u8::MAX).is_none());

        // Signed - valid values
        assert_eq!(NonMaxI8::new(-128).unwrap().get(), -128);
        assert_eq!(NonMaxI8::new(-1).unwrap().get(), -1);
        assert_eq!(NonMaxI8::new(0).unwrap().get(), 0);
        assert_eq!(NonMaxI8::new(42).unwrap().get(), 42);
        assert_eq!(NonMaxI8::new(126).unwrap().get(), 126);

        // Signed - invalid value (MAX)
        assert!(NonMaxI8::new(i8::MAX).is_none());

        // usize
        assert_eq!(NonMaxUsize::new(0).unwrap().get(), 0);
        assert!(NonMaxUsize::new(usize::MAX).is_none());
        assert_eq!(NonMaxUsize::new(usize::MIN).unwrap().get(), usize::MIN);

        // isize
        assert_eq!(NonMaxIsize::new(0).unwrap().get(), 0);
        assert!(NonMaxIsize::new(isize::MAX).is_none());
        assert_eq!(NonMaxIsize::new(isize::MIN).unwrap().get(), isize::MIN);
    }

    #[test]
    fn eq_ord() {
        let a = NonMaxU8::new(42).unwrap();
        let b = NonMaxU8::new(42).unwrap();
        let c = NonMaxU8::new(43).unwrap();

        assert_eq!(a, b);
        assert_ne!(a, c);

        let small = NonMaxU8::new(10).unwrap();
        let medium = NonMaxU8::new(20).unwrap();
        let large = NonMaxU8::new(30).unwrap();

        assert!(small < medium);
        assert!(medium <= large);
        assert!(large > medium);
        assert!(large >= small);

        // Signed comparison
        let neg = NonMaxI8::new(-10).unwrap();
        let zero = NonMaxI8::new(0).unwrap();
        let pos = NonMaxI8::new(10).unwrap();

        assert!(neg < zero);
        assert!(zero < pos);
        assert!(pos > neg);
    }

    #[test]
    fn min_max_clamp() {
        let a = NonMaxU8::new(10).unwrap();
        let b = NonMaxU8::new(20).unwrap();
        let c = NonMaxU8::new(30).unwrap();

        assert_eq!(a.max(b), b);
        assert_eq!(a.min(b), a);
        assert_eq!(b.clamp(a, c), b);
        assert_eq!(a.clamp(b, c), b); // a is less than min, so returns min
        assert_eq!(c.clamp(a, b), b); // c is greater than max, so returns max
    }

    #[test]
    fn from() {
        let nonmax = NonMaxU8::new(42).unwrap();
        let val: u8 = nonmax.into();
        assert_eq!(val, 42);
    }

    #[test]
    fn transmute_guarantee() {
        // transmute::<NonMax<T>, T>(nonmax) == nonmax.get() ^ T::MAX
        let value = 42u8;
        let nonmax = NonMaxU8::new(value).unwrap();

        unsafe {
            let transmuted: u8 = mem::transmute::<NonMaxU8, u8>(nonmax);
            assert_eq!(transmuted, value ^ u8::MAX);
        }
    }

    #[test]
    fn all_values_except_max() {
        for i in 0..=u8::MAX - 1 {
            let nonmax = NonMaxU8::new(i).unwrap();
            assert_eq!(nonmax.get(), i);
        }
        for i in i8::MIN..=i8::MAX - 1 {
            let nonmax = NonMaxI8::new(i).unwrap();
            assert_eq!(nonmax.get(), i);
        }

        // MAX should be None
        assert!(NonMaxI8::new(i8::MAX).is_none());
        assert!(NonMaxU8::new(u8::MAX).is_none());
    }
}
