use serde::{Deserialize, Serialize};
use zlim_math::{Vec3, Vec4};
use zlim_reflect::derive::Reflect;

use crate::impl_stable_interpolate_via_mix;
use crate::okcolor_convert::{okhsl_to_oklab, oklab_to_okhsl};
use crate::{Alpha, ColorToComponents, Gray, Hsla, Hsva, Hue, Hwba, Laba, Lcha};
use crate::{LinearRgba, Luminance, Mix, Oklaba, Oklcha, Saturation, Srgba, Xyza};
use crate::{impl_componentwise_vector_space, impl_from_via};

// -----------------------------------------------------------------------------
// Okhsla

/// Color in Okhsl color space with alpha.
///
/// Further information on this color model can be found on <https://bottosson.github.io/posts/colorpicker>.
///
/// Okhsl is defined relative to the sRGB (Rec. 709) gamut. Converting a wide-gamut or
/// HDR color clamps the lightness to `1.0` but can push saturation outside `[0.0, 1.0]`.
///
#[doc = include_str!("../docs/conversion.md")]
///
/// <div>
#[doc = include_str!("../docs/diagrams/model_graph.svg")]
/// </div>
#[derive(Debug, Clone, Copy, PartialEq, Reflect, Serialize, Deserialize)]
#[reflect(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Okhsla {
    /// The hue channel. [0.0, 360.0]
    pub hue: f32,
    /// The saturation channel. [0.0, 1.0]
    pub saturation: f32,
    /// The lightness channel. [0.0, 1.0]
    pub lightness: f32,
    /// The alpha channel. [0.0, 1.0]
    pub alpha: f32,
}

impl_componentwise_vector_space!(Okhsla, [hue, saturation, lightness, alpha]);
impl_stable_interpolate_via_mix!(Okhsla);

impl Okhsla {
    /// Construct a new [`Okhsla`] color from components.
    ///
    /// # Arguments
    ///
    /// * `hue` - Hue channel. [0.0, 360.0]
    /// * `saturation` - Saturation channel. [0.0, 1.0]
    /// * `lightness` - Lightness channel. [0.0, 1.0]
    /// * `alpha` - Alpha channel. [0.0, 1.0]
    pub const fn new(hue: f32, saturation: f32, lightness: f32, alpha: f32) -> Self {
        Self {
            hue,
            saturation,
            lightness,
            alpha,
        }
    }

    /// Construct a new [`Okhsla`] color from (h, s, l) components, with the default alpha (1.0).
    ///
    /// # Arguments
    ///
    /// * `hue` - Hue channel. [0.0, 360.0]
    /// * `saturation` - Saturation channel. [0.0, 1.0]
    /// * `lightness` - Lightness channel. [0.0, 1.0]
    pub const fn hsl(hue: f32, saturation: f32, lightness: f32) -> Self {
        Self::new(hue, saturation, lightness, 1.0)
    }

    /// Return a copy of this color with the saturation channel set to the given value.
    pub const fn with_saturation(self, saturation: f32) -> Self {
        Self { saturation, ..self }
    }

    /// Return a copy of this color with the lightness channel set to the given value.
    pub const fn with_lightness(self, lightness: f32) -> Self {
        Self { lightness, ..self }
    }

    /// Generate a deterministic but [quasi-randomly distributed](https://en.wikipedia.org/wiki/Low-discrepancy_sequence)
    /// color from a provided `index`.
    ///
    /// This can be helpful for generating debug colors.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use zlim_color::Okhsla;
    /// // Unique color for an entity
    /// # let entity_index = 123;
    /// // let entity_index = entity.index();
    /// let color = Okhsla::sequential_dispersed(entity_index);
    ///
    /// // Palette with 5 distinct hues
    /// let palette = (0..5).map(Okhsla::sequential_dispersed).collect::<Vec<_>>();
    /// ```
    pub const fn sequential_dispersed(index: u32) -> Self {
        const FRAC_U32MAX_GOLDEN_RATIO: u32 = 2654435769; // (u32::MAX / Φ) rounded up
        const RATIO_360: f32 = 360.0 / u32::MAX as f32;

        // from https://extremelearning.com.au/unreasonable-effectiveness-of-quasirandom-sequences/
        //
        // Map a sequence of integers (eg: 154, 155, 156, 157, 158) into the [0.0..1.0] range,
        // so that the closer the numbers are, the larger the difference of their image.
        let hue = index.wrapping_mul(FRAC_U32MAX_GOLDEN_RATIO) as f32 * RATIO_360;
        Self::hsl(hue, 1., 0.5)
    }
}

impl Default for Okhsla {
    fn default() -> Self {
        Self::new(0., 0., 1., 1.)
    }
}

// -----------------------------------------------------------------------------
// Color Traits

impl Mix for Okhsla {
    #[inline]
    fn mix(&self, other: &Self, factor: f32) -> Self {
        let n_factor = 1.0 - factor;
        Self {
            hue: crate::color_ops::lerp_hue(self.hue, other.hue, factor),
            saturation: self.saturation * n_factor + other.saturation * factor,
            lightness: self.lightness * n_factor + other.lightness * factor,
            alpha: self.alpha * n_factor + other.alpha * factor,
        }
    }
}

impl Gray for Okhsla {
    const BLACK: Self = Self::new(0., 0., 0., 1.);
    const WHITE: Self = Self::new(0., 0., 1., 1.);
}

impl Alpha for Okhsla {
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

impl Hue for Okhsla {
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

impl Saturation for Okhsla {
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

impl Luminance for Okhsla {
    #[inline]
    fn with_luminance(&self, lightness: f32) -> Self {
        Self { lightness, ..*self }
    }

    fn luminance(&self) -> f32 {
        self.lightness
    }

    fn darker(&self, amount: f32) -> Self {
        Self {
            lightness: (self.lightness - amount).clamp(0., 1.),
            ..*self
        }
    }

    fn lighter(&self, amount: f32) -> Self {
        Self {
            lightness: (self.lightness + amount).min(1.),
            ..*self
        }
    }
}

impl ColorToComponents for Okhsla {
    fn to_f32_array(self) -> [f32; 4] {
        [self.hue, self.saturation, self.lightness, self.alpha]
    }

    fn to_f32_array_no_alpha(self) -> [f32; 3] {
        [self.hue, self.saturation, self.lightness]
    }

    fn to_vec4(self) -> Vec4 {
        Vec4::new(self.hue, self.saturation, self.lightness, self.alpha)
    }

    fn to_vec3(self) -> Vec3 {
        Vec3::new(self.hue, self.saturation, self.lightness)
    }

    fn from_f32_array(color: [f32; 4]) -> Self {
        Self {
            hue: color[0],
            saturation: color[1],
            lightness: color[2],
            alpha: color[3],
        }
    }

    fn from_f32_array_no_alpha(color: [f32; 3]) -> Self {
        Self {
            hue: color[0],
            saturation: color[1],
            lightness: color[2],
            alpha: 1.0,
        }
    }

    fn from_vec4(color: Vec4) -> Self {
        Self {
            hue: color[0],
            saturation: color[1],
            lightness: color[2],
            alpha: color[3],
        }
    }

    fn from_vec3(color: Vec3) -> Self {
        Self {
            hue: color[0],
            saturation: color[1],
            lightness: color[2],
            alpha: 1.0,
        }
    }
}

// -----------------------------------------------------------------------------
// wgpu_type

impl From<Okhsla> for wgpu_types::Color {
    fn from(color: Okhsla) -> Self {
        wgpu_types::Color {
            r: color.hue as f64,
            g: color.saturation as f64,
            b: color.lightness as f64,
            a: color.alpha as f64,
        }
    }
}

// -----------------------------------------------------------------------------
// Conversion

impl From<Oklaba> for Okhsla {
    fn from(value: Oklaba) -> Self {
        oklab_to_okhsl(value)
    }
}

impl From<Okhsla> for Oklaba {
    fn from(value: Okhsla) -> Self {
        okhsl_to_oklab(value)
    }
}

// Derived conversions through Oklaba.
impl_from_via!(
    Oklaba,
    Okhsla,
    [
        LinearRgba, Hsla, Hsva, Hwba, Lcha, Srgba, Xyza, Laba, Oklcha
    ]
);
