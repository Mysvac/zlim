//! # zlim-core
//!
//! Representations of colors in various color spaces.
//!
//! This crate provides a number of color representations, including:
//!
//! - [`Srgba`] (standard RGBA, with gamma correction)
//! - [`LinearRgba`] (linear RGBA, without gamma correction)
//! - [`Hsla`] (hue, saturation, lightness, alpha)
//! - [`Hsva`] (hue, saturation, value, alpha)
//! - [`Hwba`] (hue, whiteness, blackness, alpha)
//! - [`Laba`] (lightness, a-axis, b-axis, alpha)
//! - [`Lcha`] (lightness, chroma, hue, alpha)
//! - [`Oklaba`] (lightness, a-axis, b-axis, alpha)
//! - [`Oklcha`] (lightness, chroma, hue, alpha)
//! - [`Xyza`] (x-axis, y-axis, z-axis, alpha)
//! - [`Okhsla`] (hue, saturation, lightness, alpha)
//! - [`Okhsva`] (hue, saturation, value, alpha)
//! - [`Okhwba`] (hue, whiteness, blackness, alpha)
//!
//! Each of these color spaces is represented as a distinct Rust type.
//!
//! ## Color Space Usage
//!
//! Rendering engines typically use linear RGBA colors, which allow for physically accurate
//! lighting calculations. However, linear RGBA colors are not perceptually uniform, because
//! both human eyes and computer monitors have non-linear responses to light. "Standard" RGBA
//! represents an industry-wide compromise designed to encode colors in a way that looks good to
//! humans in as few bits as possible, but it is not suitable for lighting calculations.
//!
//! Most image file formats and scene graph formats use standard RGBA, because graphic design
//! tools are intended to be used by humans. However, 3D lighting calculations operate in linear
//! RGBA, so it is important to convert standard colors to linear before sending them to the GPU.
//! Most zlim APIs will handle this conversion automatically, but if you are writing a custom
//! shader, you will need to do this conversion yourself.
//!
//! HSL and LCH are "cylindrical" color spaces, which means they represent colors as a combination
//! of hue, saturation, and lightness (or chroma). These color spaces are useful for working
//! with colors in an artistic way - for example, when creating gradients or color palettes.
//! A gradient in HSL space from red to violet will produce a rainbow. The LCH color space is
//! more perceptually accurate than HSL, but is less intuitive to work with.
//!
//! HSV and HWB are very closely related to HSL in their derivation, having identical definitions for
//! hue. Where HSL uses saturation and lightness, HSV uses a slightly modified definition of saturation,
//! and an analog of lightness in the form of value. In contrast, HWB instead uses whiteness and blackness
//! parameters, which can be used to lighten and darken a particular hue respectively.
//!
//! Oklab and Oklch are perceptually uniform color spaces that are designed to be used for tasks such
//! as image processing. They are not as widely used as the other color spaces, but are useful
//! for tasks such as color correction and image analysis, where it is important to be able
//! to do things like change color saturation without causing hue shifts.
//!
//! XYZ is a foundational space commonly used in the definition of other more modern color
//! spaces. The space is more formally known as CIE 1931, where the `x` and `z` axes represent
//! a form of chromaticity, while `y` defines an illuminance level.
//!
//! See also the [Wikipedia article on color spaces](https://en.wikipedia.org/wiki/Color_space).
//!
#![doc = include_str!("../docs/conversion.md")]
//!
//! <div>
#![doc = include_str!("../docs/diagrams/model_graph.svg")]
//! </div>
//!
//! ## Other Utilities
//!
//! The crate also provides a number of color operations, such as blending, color difference,
//! and color range operations.
//!
//! In addition, there is a [`Color`] enum that can represent any of the color
//! types in this crate. This is useful when you need to store a color in a data structure
//! that can't be generic over the color type.
//!
//! Color types that are either physically or perceptually linear also implement `Add<Self>`, `Sub<Self>`, `Mul<f32>` and `Div<f32>`
//! allowing you to use them with splines.
//!
//! Please note that most often adding or subtracting colors is not what you may want.
//! Please have a look at other operations like blending, lightening or mixing colors using e.g. [`Mix`] or [`Luminance`] instead.
//!
//! ## Example
//!
//! ```
//! use zlim_color::{Srgba, Hsla};
//!
//! let srgba = Srgba::new(0.5, 0.2, 0.8, 1.0);
//! let hsla: Hsla = srgba.into();
//!
//! println!("Srgba: {:?}", srgba);
//! println!("Hsla: {:?}", hsla);
//! ```
#![forbid(unsafe_code)]

// -----------------------------------------------------------------------------

mod color;
mod color_gradient;
mod color_ops;
mod color_range;
mod hsla;
mod hsva;
mod hwba;
mod laba;
mod lcha;
mod linear_rgba;
mod okcolor_convert;
mod okhsla;
mod okhsva;
mod okhwba;
mod oklaba;
mod oklcha;
mod primaries;
mod srgba;
mod xyza;

pub mod color_difference;
pub mod palettes;

pub use color::*;
pub use color_gradient::*;
pub use color_ops::*;
pub use color_range::*;
pub use hsla::*;
pub use hsva::*;
pub use hwba::*;
pub use laba::*;
pub use lcha::*;
pub use linear_rgba::*;
pub use okhsla::*;
pub use okhsva::*;
pub use okhwba::*;
pub use oklaba::*;
pub use oklcha::*;
pub use primaries::*;
pub use srgba::*;
pub use xyza::*;

/// The color prelude.
pub mod prelude {
    pub use crate::{
        color::*, color_ops::*, hsla::*, hsva::*, hwba::*, laba::*, lcha::*, linear_rgba::*,
        okhsla::*, okhsva::*, okhwba::*, oklaba::*, oklcha::*, srgba::*, xyza::*,
    };
}

// -----------------------------------------------------------------------------
// internal macros

macro_rules! impl_componentwise_vector_space {
    ($ty: ident, [$($element: ident),+]) => {
        impl core::ops::Add<Self> for $ty {
            type Output = Self;

            fn add(self, rhs: Self) -> Self::Output {
                Self::Output {
                    $($element: self.$element + rhs.$element,)+
                }
            }
        }

        impl core::ops::AddAssign<Self> for $ty {
            fn add_assign(&mut self, rhs: Self) {
                *self = *self + rhs;
            }
        }

        impl core::ops::Neg for $ty {
            type Output = Self;

            fn neg(self) -> Self::Output {
                Self::Output {
                    $($element: -self.$element,)+
                }
            }
        }

        impl core::ops::Sub<Self> for $ty {
            type Output = Self;

            fn sub(self, rhs: Self) -> Self::Output {
                Self::Output {
                    $($element: self.$element - rhs.$element,)+
                }
            }
        }

        impl core::ops::SubAssign<Self> for $ty {
            fn sub_assign(&mut self, rhs: Self) {
                *self = *self - rhs;
            }
        }

        impl core::ops::Mul<f32> for $ty {
            type Output = Self;

            fn mul(self, rhs: f32) -> Self::Output {
                Self::Output {
                    $($element: self.$element * rhs,)+
                }
            }
        }

        impl core::ops::Mul<$ty> for f32 {
            type Output = $ty;

            fn mul(self, rhs: $ty) -> Self::Output {
                Self::Output {
                    $($element: self * rhs.$element,)+
                }
            }
        }

        impl core::ops::MulAssign<f32> for $ty {
            fn mul_assign(&mut self, rhs: f32) {
                *self = *self * rhs;
            }
        }

        impl core::ops::Div<f32> for $ty {
            type Output = Self;

            fn div(self, rhs: f32) -> Self::Output {
                Self::Output {
                    $($element: self.$element / rhs,)+
                }
            }
        }

        impl core::ops::DivAssign<f32> for $ty {
            fn div_assign(&mut self, rhs: f32) {
                *self = *self / rhs;
            }
        }

        impl zlim_math::VectorSpace for $ty {
            type Scalar = f32;
            const ZERO: Self = Self {
                $($element: 0.0,)+
            };
        }
    };
}

pub(crate) use impl_componentwise_vector_space;

/// Generates a lerp-based [`StableInterpolate`] implementation for
/// component-wise linear colors (i.e. colors whose channels can be
/// interpolated directly without perceptual distortion).
///
/// [`StableInterpolate`]: zlim_math::StableInterpolate
macro_rules! impl_componentwise_stable_interpolate {
    ($ty:ident, [$($element:ident),+]) => {
        impl zlim_math::StableInterpolate for $ty {
            fn interpolate_stable(&self, other: &Self, t: f32) -> Self {
                Self {
                    $($element: self.$element + (other.$element - self.$element) * t,)+
                }
            }
        }
    };
}

pub(crate) use impl_componentwise_stable_interpolate;

/// Generates a [`StableInterpolate`] implementation that delegates to the
/// color's [`Mix`] implementation, giving hue-aware (short-arc wrapping)
/// interpolation for cylindrical color spaces.
///
/// [`StableInterpolate`]: zlim_math::StableInterpolate
/// [`Mix`]: crate::Mix
macro_rules! impl_stable_interpolate_via_mix {
    ($ty:ident) => {
        impl zlim_math::StableInterpolate for $ty {
            fn interpolate_stable(&self, other: &Self, t: f32) -> Self {
                self.mix(other, t)
            }
        }
    };
}

pub(crate) use impl_stable_interpolate_via_mix;

macro_rules! impl_from_via {
    ($via:ty, $target:ty, [$($from:ty),* $(,)?]) => {
        $(
            impl From<$from> for $target {
                fn from(value: $from) -> Self {
                    <$via>::from(value).into()
                }
            }

            impl From<$target> for $from {
                fn from(value: $target) -> Self {
                    <$via>::from(value).into()
                }
            }
        )*
    };
}

pub(crate) use impl_from_via;

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use crate::*;
    use zlim_math::StableInterpolate;

    const fn assert_standard_color<T>()
    where
        T: core::fmt::Debug,
        T: Clone + Copy,
        T: PartialEq,
        T: Default,
        T: From<Color> + Into<Color>,
        T: From<Srgba> + Into<Srgba>,
        T: From<LinearRgba> + Into<LinearRgba>,
        T: From<Hsla> + Into<Hsla>,
        T: From<Hsva> + Into<Hsva>,
        T: From<Hwba> + Into<Hwba>,
        T: From<Laba> + Into<Laba>,
        T: From<Lcha> + Into<Lcha>,
        T: From<Oklaba> + Into<Oklaba>,
        T: From<Oklcha> + Into<Oklcha>,
        T: From<Xyza> + Into<Xyza>,
        T: From<Okhsla> + Into<Okhsla>,
        T: From<Okhsva> + Into<Okhsva>,
        T: From<Okhwba> + Into<Okhwba>,
        T: Alpha,
    {
    }

    // Compile-time check that every color type satisfies the `StandardColor`
    // trait surface (all cross-space conversions plus `Alpha`).
    const _: () = {
        assert_standard_color::<Color>();
        assert_standard_color::<Srgba>();
        assert_standard_color::<LinearRgba>();
        assert_standard_color::<Hsla>();
        assert_standard_color::<Hsva>();
        assert_standard_color::<Hwba>();
        assert_standard_color::<Laba>();
        assert_standard_color::<Lcha>();
        assert_standard_color::<Oklaba>();
        assert_standard_color::<Oklcha>();
        assert_standard_color::<Xyza>();
        assert_standard_color::<Okhsla>();
        assert_standard_color::<Okhsva>();
        assert_standard_color::<Okhwba>();
    };

    pub(crate) fn assert_mat3_approx_eq(a: zlim_math::Mat3, b: zlim_math::Mat3, tolerance: f32) {
        for col in 0..3 {
            for row in 0..3 {
                assert_approx_eq!(
                    a.col(col)[row],
                    b.col(col)[row],
                    tolerance,
                    std::format!("matrices differ at column {col}, row {row}: {a} != {b}")
                );
            }
        }
    }

    macro_rules! assert_approx_eq {
        ($x:expr, $y:expr, $d:expr) => {
            assert!(!f32::is_nan($x));
            assert!(!f32::is_nan($y));
            if zlim_math::ops::abs($x - $y) >= $d {
                panic!(
                    "assertion failed: `(left !== right)` \
                    (left: `{}`, right: `{}`, tolerance: `{}`)",
                    $x, $y, $d
                );
            }
        };

        ($x:expr, $y:expr, $d:expr, $msg:expr) => {
            assert!(!f32::is_nan($x));
            assert!(!f32::is_nan($y));
            if zlim_math::ops::abs($x - $y) >= $d {
                panic!(
                    "assertion failed: `(left !== right)` \
                    (left: `{}`, right: `{}`, tolerance: `{}`). {}",
                    $x, $y, $d, $msg
                );
            }
        };
    }

    pub(crate) use assert_approx_eq;

    // TODO! Fully perceptually-uniform interpolation (via Oklab) for the
    // cylindrical spaces; currently they interpolate channel-wise with
    // short-arc hue wrapping through `Mix`.
    #[test]
    pub fn test_color_stable_interpolate() {
        let b = Srgba::BLACK;
        let w = Srgba::WHITE;
        // `Srgba` interpolates in linear space for perceptual correctness, so
        // the midpoint is the linear 0.5 converted back to sRGB.
        assert_eq!(
            b.interpolate_stable(&w, 0.5),
            Srgba::new(0.7353569, 0.7353569, 0.7353569, 1.0),
        );

        let b = LinearRgba::BLACK;
        let w = LinearRgba::WHITE;
        assert_eq!(
            b.interpolate_stable(&w, 0.5),
            LinearRgba::new(0.5, 0.5, 0.5, 1.0),
        );

        let b = Xyza::BLACK;
        let w = Xyza::WHITE;
        assert_eq!(b.interpolate_stable(&w, 0.5), Xyza::gray(0.5),);

        let b = Laba::BLACK;
        let w = Laba::WHITE;
        assert_eq!(b.interpolate_stable(&w, 0.5), Laba::new(0.5, 0.0, 0.0, 1.0),);

        let b = Oklaba::BLACK;
        let w = Oklaba::WHITE;
        assert_eq!(b.interpolate_stable(&w, 0.5), Oklaba::gray(0.5),);
    }
}
