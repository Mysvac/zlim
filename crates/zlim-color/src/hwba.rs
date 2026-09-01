//! Implementation of the Hue-Whiteness-Blackness (HWB) color model as described
//! in [_HWB - A More Intuitive Hue-Based Color Model_] by _Smith et al_.
//!
//! [_HWB - A More Intuitive Hue-Based Color Model_]: https://web.archive.org/web/20240226005220/http://alvyray.com/Papers/CG/HWB_JGTv208.pdf

use serde::{Deserialize, Serialize};
use zlim_math::{Vec3, Vec4, ops};
use zlim_reflect::derive::Reflect;

use crate::{Alpha, ColorToComponents, Gray, Hue, Lcha};
use crate::{LinearRgba, Mix, Srgba, Xyza};
use crate::{impl_from_via, impl_stable_interpolate_via_mix};

// -----------------------------------------------------------------------------
// Laba

/// Color in Hue-Whiteness-Blackness (HWB) color space with alpha.
///
/// Further information on this color model can be found on [Wikipedia](https://en.wikipedia.org/wiki/HWB_color_model).
///
#[doc = include_str!("../docs/conversion.md")]
///
/// <div>
#[doc = include_str!("../docs/diagrams/model_graph.svg")]
/// </div>
#[derive(Debug, Clone, Copy, PartialEq, Reflect, Serialize, Deserialize)]
#[reflect(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Hwba {
    /// The hue channel. [0.0, 360.0]
    pub hue: f32,
    /// The whiteness channel. [0.0, 1.0]
    pub whiteness: f32,
    /// The blackness channel. [0.0, 1.0]
    pub blackness: f32,
    /// The alpha channel. [0.0, 1.0]
    pub alpha: f32,
}

impl_stable_interpolate_via_mix!(Hwba);

impl Hwba {
    /// Construct a new [`Hwba`] color from components.
    ///
    /// # Arguments
    ///
    /// * `hue` - Hue channel. [0.0, 360.0]
    /// * `whiteness` - Whiteness channel. [0.0, 1.0]
    /// * `blackness` - Blackness channel. [0.0, 1.0]
    /// * `alpha` - Alpha channel. [0.0, 1.0]
    pub const fn new(hue: f32, whiteness: f32, blackness: f32, alpha: f32) -> Self {
        Self {
            hue,
            whiteness,
            blackness,
            alpha,
        }
    }

    /// Construct a new [`Hwba`] color from (h, s, l) components, with the default alpha (1.0).
    ///
    /// # Arguments
    ///
    /// * `hue` - Hue channel. [0.0, 360.0]
    /// * `whiteness` - Whiteness channel. [0.0, 1.0]
    /// * `blackness` - Blackness channel. [0.0, 1.0]
    pub const fn hwb(hue: f32, whiteness: f32, blackness: f32) -> Self {
        Self::new(hue, whiteness, blackness, 1.0)
    }

    /// Return a copy of this color with the whiteness channel set to the given value.
    pub const fn with_whiteness(self, whiteness: f32) -> Self {
        Self { whiteness, ..self }
    }

    /// Return a copy of this color with the blackness channel set to the given value.
    pub const fn with_blackness(self, blackness: f32) -> Self {
        Self { blackness, ..self }
    }
}

impl Default for Hwba {
    fn default() -> Self {
        Self::new(0., 0., 1., 1.)
    }
}

// -----------------------------------------------------------------------------
// Color Traits

impl Mix for Hwba {
    #[inline]
    fn mix(&self, other: &Self, factor: f32) -> Self {
        let n_factor = 1.0 - factor;
        Self {
            hue: crate::color_ops::lerp_hue(self.hue, other.hue, factor),
            whiteness: self.whiteness * n_factor + other.whiteness * factor,
            blackness: self.blackness * n_factor + other.blackness * factor,
            alpha: self.alpha * n_factor + other.alpha * factor,
        }
    }
}

impl Gray for Hwba {
    const BLACK: Self = Self::new(0., 0., 1., 1.);
    const WHITE: Self = Self::new(0., 1., 0., 1.);
}

impl Alpha for Hwba {
    #[inline]
    fn with_alpha(&self, alpha: f32) -> Self {
        Self { alpha, ..*self }
    }

    #[inline]
    fn alpha(&self) -> f32 {
        self.alpha
    }

    #[inline]
    fn set_alpha(&mut self, alpha: f32) {
        self.alpha = alpha;
    }
}

impl Hue for Hwba {
    #[inline]
    fn with_hue(&self, hue: f32) -> Self {
        Self { hue, ..*self }
    }

    #[inline]
    fn hue(&self) -> f32 {
        self.hue
    }

    #[inline]
    fn set_hue(&mut self, hue: f32) {
        self.hue = hue;
    }
}

impl ColorToComponents for Hwba {
    fn to_f32_array(self) -> [f32; 4] {
        [self.hue, self.whiteness, self.blackness, self.alpha]
    }

    fn to_f32_array_no_alpha(self) -> [f32; 3] {
        [self.hue, self.whiteness, self.blackness]
    }

    fn to_vec4(self) -> Vec4 {
        Vec4::new(self.hue, self.whiteness, self.blackness, self.alpha)
    }

    fn to_vec3(self) -> Vec3 {
        Vec3::new(self.hue, self.whiteness, self.blackness)
    }

    fn from_f32_array(color: [f32; 4]) -> Self {
        Self {
            hue: color[0],
            whiteness: color[1],
            blackness: color[2],
            alpha: color[3],
        }
    }

    fn from_f32_array_no_alpha(color: [f32; 3]) -> Self {
        Self {
            hue: color[0],
            whiteness: color[1],
            blackness: color[2],
            alpha: 1.0,
        }
    }

    fn from_vec4(color: Vec4) -> Self {
        Self {
            hue: color[0],
            whiteness: color[1],
            blackness: color[2],
            alpha: color[3],
        }
    }

    fn from_vec3(color: Vec3) -> Self {
        Self {
            hue: color[0],
            whiteness: color[1],
            blackness: color[2],
            alpha: 1.0,
        }
    }
}

// -----------------------------------------------------------------------------
// Conversion

impl From<Srgba> for Hwba {
    fn from(
        Srgba {
            red,
            green,
            blue,
            alpha,
        }: Srgba,
    ) -> Self {
        // Based on "HWB - A More Intuitive Hue-Based Color Model" Appendix B
        let x_max = 0f32.max(red).max(green).max(blue);
        let x_min = 1f32.min(red).min(green).min(blue);

        let chroma = x_max - x_min;

        let hue = if chroma == 0.0 {
            0.0
        } else if red == x_max {
            60.0 * (green - blue) / chroma
        } else if green == x_max {
            60.0 * (2.0 + (blue - red) / chroma)
        } else {
            60.0 * (4.0 + (red - green) / chroma)
        };
        let hue = if hue < 0.0 { 360.0 + hue } else { hue };

        let whiteness = x_min;
        let blackness = 1.0 - x_max;

        Hwba {
            hue,
            whiteness,
            blackness,
            alpha,
        }
    }
}

impl From<Hwba> for Srgba {
    fn from(
        Hwba {
            hue,
            whiteness,
            blackness,
            alpha,
        }: Hwba,
    ) -> Self {
        // Based on "HWB - A More Intuitive Hue-Based Color Model" Appendix B
        let w = whiteness;
        let v = 1. - blackness;

        let h = (hue % 360.) / 60.;
        let i = ops::floor(h);
        let f = h - i;

        let i = i as u8;

        let f = if i.is_multiple_of(2) { f } else { 1. - f };

        let n = w + f * (v - w);

        let (red, green, blue) = match i {
            0 => (v, n, w),
            1 => (n, v, w),
            2 => (w, v, n),
            3 => (w, n, v),
            4 => (n, w, v),
            5 => (v, w, n),
            _ => unreachable!("i is bounded in [0, 6)"),
        };

        Srgba::new(red, green, blue, alpha)
    }
}

// Derived conversions through Srgba.
impl_from_via!(Srgba, Hwba, [LinearRgba, Lcha, Xyza]);
