use serde::{Deserialize, Serialize};
use zlim_math::{Vec3, Vec4};
use zlim_reflect::derive::Reflect;

use crate::{Alpha, ColorToComponents, Gray, Hue, Hwba};
use crate::{Lcha, LinearRgba, Mix, Saturation, Srgba, Xyza};
use crate::{impl_from_via, impl_stable_interpolate_via_mix};

// -----------------------------------------------------------------------------
// Hsva

/// Color in Hue-Saturation-Value (HSV) color space with alpha.
///
/// Further information on this color model can be found on [Wikipedia](https://en.wikipedia.org/wiki/HSL_and_HSV).
///
#[doc = include_str!("../docs/conversion.md")]
///
/// <div>
#[doc = include_str!("../docs/diagrams/model_graph.svg")]
/// </div>
#[derive(Debug, Clone, Copy, PartialEq, Reflect, Serialize, Deserialize)]
#[reflect(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Hsva {
    /// The hue channel. [0.0, 360.0]
    pub hue: f32,
    /// The saturation channel. [0.0, 1.0]
    pub saturation: f32,
    /// The value channel. [0.0, 1.0]
    pub value: f32,
    /// The alpha channel. [0.0, 1.0]
    pub alpha: f32,
}

impl_stable_interpolate_via_mix!(Hsva);

impl Hsva {
    /// Construct a new [`Hsva`] color from components.
    ///
    /// # Arguments
    ///
    /// * `hue` - Hue channel. [0.0, 360.0]
    /// * `saturation` - Saturation channel. [0.0, 1.0]
    /// * `value` - Value channel. [0.0, 1.0]
    /// * `alpha` - Alpha channel. [0.0, 1.0]
    pub const fn new(hue: f32, saturation: f32, value: f32, alpha: f32) -> Self {
        Self {
            hue,
            saturation,
            value,
            alpha,
        }
    }

    /// Construct a new [`Hsva`] color from (h, s, v) components, with the default alpha (1.0).
    ///
    /// # Arguments
    ///
    /// * `hue` - Hue channel. [0.0, 360.0]
    /// * `saturation` - Saturation channel. [0.0, 1.0]
    /// * `value` - Value channel. [0.0, 1.0]
    pub const fn hsv(hue: f32, saturation: f32, value: f32) -> Self {
        Self::new(hue, saturation, value, 1.0)
    }

    /// Return a copy of this color with the saturation channel set to the given value.
    pub const fn with_saturation(self, saturation: f32) -> Self {
        Self { saturation, ..self }
    }

    /// Return a copy of this color with the value channel set to the given value.
    pub const fn with_value(self, value: f32) -> Self {
        Self { value, ..self }
    }
}

impl Default for Hsva {
    fn default() -> Self {
        Self::new(0., 0., 1., 1.)
    }
}

// -----------------------------------------------------------------------------
// Color Traits

impl Mix for Hsva {
    #[inline]
    fn mix(&self, other: &Self, factor: f32) -> Self {
        let n_factor = 1.0 - factor;
        Self {
            hue: crate::color_ops::lerp_hue(self.hue, other.hue, factor),
            saturation: self.saturation * n_factor + other.saturation * factor,
            value: self.value * n_factor + other.value * factor,
            alpha: self.alpha * n_factor + other.alpha * factor,
        }
    }
}

impl Gray for Hsva {
    const BLACK: Self = Self::new(0., 0., 0., 1.);
    const WHITE: Self = Self::new(0., 0., 1., 1.);
}

impl Alpha for Hsva {
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

impl Hue for Hsva {
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

impl Saturation for Hsva {
    #[inline]
    fn with_saturation(&self, saturation: f32) -> Self {
        Self {
            saturation,
            ..*self
        }
    }

    #[inline]
    fn saturation(&self) -> f32 {
        self.saturation
    }

    #[inline]
    fn set_saturation(&mut self, saturation: f32) {
        self.saturation = saturation;
    }
}

// -----------------------------------------------------------------------------
// Conversion

impl From<Hsva> for Hwba {
    fn from(
        Hsva {
            hue,
            saturation,
            value,
            alpha,
        }: Hsva,
    ) -> Self {
        // Based on https://en.wikipedia.org/wiki/HWB_color_model#Conversion
        let whiteness = (1. - saturation) * value;
        let blackness = 1. - value;

        Hwba::new(hue, whiteness, blackness, alpha)
    }
}

impl From<Hwba> for Hsva {
    fn from(
        Hwba {
            hue,
            whiteness,
            blackness,
            alpha,
        }: Hwba,
    ) -> Self {
        // Based on https://en.wikipedia.org/wiki/HWB_color_model#Conversion
        let value = 1. - blackness;
        let saturation = if value != 0. {
            1. - (whiteness / value)
        } else {
            0.
        };

        Hsva::new(hue, saturation, value, alpha)
    }
}

impl ColorToComponents for Hsva {
    fn to_f32_array(self) -> [f32; 4] {
        [self.hue, self.saturation, self.value, self.alpha]
    }

    fn to_f32_array_no_alpha(self) -> [f32; 3] {
        [self.hue, self.saturation, self.value]
    }

    fn to_vec4(self) -> Vec4 {
        Vec4::new(self.hue, self.saturation, self.value, self.alpha)
    }

    fn to_vec3(self) -> Vec3 {
        Vec3::new(self.hue, self.saturation, self.value)
    }

    fn from_f32_array(color: [f32; 4]) -> Self {
        Self {
            hue: color[0],
            saturation: color[1],
            value: color[2],
            alpha: color[3],
        }
    }

    fn from_f32_array_no_alpha(color: [f32; 3]) -> Self {
        Self {
            hue: color[0],
            saturation: color[1],
            value: color[2],
            alpha: 1.0,
        }
    }

    fn from_vec4(color: Vec4) -> Self {
        Self {
            hue: color[0],
            saturation: color[1],
            value: color[2],
            alpha: color[3],
        }
    }

    fn from_vec3(color: Vec3) -> Self {
        Self {
            hue: color[0],
            saturation: color[1],
            value: color[2],
            alpha: 1.0,
        }
    }
}

// Derived conversions through Hwba.
impl_from_via!(Hwba, Hsva, [Srgba, LinearRgba, Lcha, Xyza]);
