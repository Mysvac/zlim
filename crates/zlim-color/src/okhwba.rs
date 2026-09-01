use serde::{Deserialize, Serialize};
use zlim_math::{Vec3, Vec4};
use zlim_reflect::derive::Reflect;

use crate::{Alpha, ColorToComponents, Gray, Hsla, Hsva, Hue, Hwba, Laba};
use crate::{Lcha, LinearRgba, Mix, Okhsla, Okhsva, Oklaba, Oklcha, Srgba, Xyza};
use crate::{impl_from_via, impl_stable_interpolate_via_mix};

// -----------------------------------------------------------------------------
// Okhwba

/// Color in Okhwb color space with alpha.
///
/// Further information on this color model can be found on <https://bottosson.github.io/posts/colorpicker>.
///
#[doc = include_str!("../docs/conversion.md")]
///
/// <div>
#[doc = include_str!("../docs/diagrams/model_graph.svg")]
/// </div>
#[derive(Debug, Clone, Copy, PartialEq, Reflect, Serialize, Deserialize)]
#[reflect(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Okhwba {
    /// The hue channel. [0.0, 360.0]
    pub hue: f32,
    /// The whiteness channel. [0.0, 1.0]
    pub whiteness: f32,
    /// The blackness channel. [0.0, 1.0]
    pub blackness: f32,
    /// The alpha channel. [0.0, 1.0]
    pub alpha: f32,
}

impl_stable_interpolate_via_mix!(Okhwba);

impl Okhwba {
    /// Construct a new [`Okhwba`] color from components.
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

    /// Construct a new [`Okhwba`] color from (h, w, b) components, with the default alpha (1.0).
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

impl Default for Okhwba {
    fn default() -> Self {
        Self::new(0., 0., 1., 1.)
    }
}

// -----------------------------------------------------------------------------
// Color Traits

impl Mix for Okhwba {
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

impl Gray for Okhwba {
    const BLACK: Self = Self::new(0., 0., 1., 1.);
    const WHITE: Self = Self::new(0., 1., 0., 1.);
}

impl Alpha for Okhwba {
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

impl Hue for Okhwba {
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

impl ColorToComponents for Okhwba {
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

impl From<Okhsva> for Okhwba {
    fn from(
        Okhsva {
            hue,
            saturation,
            value,
            alpha,
        }: Okhsva,
    ) -> Self {
        // Based on https://bottosson.github.io/posts/colorpicker/#okhwb
        let whiteness = (1. - saturation) * value;
        let blackness = 1. - value;

        Okhwba::new(hue, whiteness, blackness, alpha)
    }
}

impl From<Okhwba> for Okhsva {
    fn from(
        Okhwba {
            hue,
            whiteness,
            blackness,
            alpha,
        }: Okhwba,
    ) -> Self {
        // Based on https://bottosson.github.io/posts/colorpicker/#okhwb
        let value = 1. - blackness;
        let saturation = if value != 0. {
            1. - (whiteness / value)
        } else {
            0.
        };

        Okhsva::new(hue, saturation, value, alpha)
    }
}

// Derived conversions through Okhsva.
impl_from_via!(
    Okhsva,
    Okhwba,
    [
        LinearRgba, Srgba, Hwba, Lcha, Xyza, Okhsla, Hsla, Hsva, Laba, Oklaba, Oklcha
    ]
);
