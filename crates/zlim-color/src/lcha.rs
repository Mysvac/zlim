use serde::{Deserialize, Serialize};
use zlim_math::{Vec3, Vec4, ops};
use zlim_reflect::derive::Reflect;

use crate::{Alpha, ColorToComponents, Gray, Hue, Xyza};
use crate::{Laba, LinearRgba, Luminance, Mix, Srgba};
use crate::{impl_from_via, impl_stable_interpolate_via_mix};

// -----------------------------------------------------------------------------
// Lcha

/// Color in LCH color space, with alpha
///
#[doc = include_str!("../docs/conversion.md")]
///
/// <div>
#[doc = include_str!("../docs/diagrams/model_graph.svg")]
/// </div>
#[derive(Debug, Clone, Copy, PartialEq, Reflect, Serialize, Deserialize)]
#[reflect(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Lcha {
    /// The lightness channel. [0.0, 1.5]
    pub lightness: f32,
    /// The chroma channel. Typically [0.0, 1.5]
    pub chroma: f32,
    /// The hue channel. [0.0, 360.0]
    pub hue: f32,
    /// The alpha channel. [0.0, 1.0]
    pub alpha: f32,
}

impl_stable_interpolate_via_mix!(Lcha);

impl Lcha {
    /// Construct a new [`Lcha`] color from components.
    ///
    /// # Arguments
    ///
    /// * `lightness` - Lightness channel. [0.0, 1.5]
    /// * `chroma` - Chroma channel. Typically [0.0, 1.5]
    /// * `hue` - Hue channel. [0.0, 360.0]
    /// * `alpha` - Alpha channel. [0.0, 1.0]
    pub const fn new(lightness: f32, chroma: f32, hue: f32, alpha: f32) -> Self {
        Self {
            lightness,
            chroma,
            hue,
            alpha,
        }
    }

    /// Construct a new [`Lcha`] color from (h, s, l) components, with the default alpha (1.0).
    ///
    /// # Arguments
    ///
    /// * `lightness` - Lightness channel. [0.0, 1.5]
    /// * `chroma` - Chroma channel. Typically [0.0, 1.5]
    /// * `hue` - Hue channel. [0.0, 360.0]
    pub const fn lch(lightness: f32, chroma: f32, hue: f32) -> Self {
        Self {
            lightness,
            chroma,
            hue,
            alpha: 1.0,
        }
    }

    /// Return a copy of this color with the chroma channel set to the given value.
    pub const fn with_chroma(self, chroma: f32) -> Self {
        Self { chroma, ..self }
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
    /// # use zlim_color::Lcha;
    /// // Unique color for an entity
    /// # let entity_index = 123;
    /// // let entity_index = entity.index();
    /// let color = Lcha::sequential_dispersed(entity_index);
    ///
    /// // Palette with 5 distinct hues
    /// let palette = (0..5).map(Lcha::sequential_dispersed).collect::<Vec<_>>();
    /// ```
    pub const fn sequential_dispersed(index: u32) -> Self {
        const FRAC_U32MAX_GOLDEN_RATIO: u32 = 2654435769; // (u32::MAX / Φ) rounded up
        const RATIO_360: f32 = 360.0 / u32::MAX as f32;

        // from https://extremelearning.com.au/unreasonable-effectiveness-of-quasirandom-sequences/
        //
        // Map a sequence of integers (eg: 154, 155, 156, 157, 158) into the [0.0..1.0] range,
        // so that the closer the numbers are, the larger the difference of their image.
        let hue = index.wrapping_mul(FRAC_U32MAX_GOLDEN_RATIO) as f32 * RATIO_360;
        Self::lch(0.75, 0.35, hue)
    }
}

impl Default for Lcha {
    fn default() -> Self {
        Self::new(1., 0., 0., 1.)
    }
}

// -----------------------------------------------------------------------------
// Color Traits

impl Mix for Lcha {
    #[inline]
    fn mix(&self, other: &Self, factor: f32) -> Self {
        let n_factor = 1.0 - factor;
        Self {
            lightness: self.lightness * n_factor + other.lightness * factor,
            chroma: self.chroma * n_factor + other.chroma * factor,
            hue: crate::color_ops::lerp_hue(self.hue, other.hue, factor),
            alpha: self.alpha * n_factor + other.alpha * factor,
        }
    }
}

impl Gray for Lcha {
    const BLACK: Self = Self::new(0.0, 0.0, 0.0000136603785, 1.0);
    const WHITE: Self = Self::new(1.0, 0.0, 0.0000136603785, 1.0);
}

impl Alpha for Lcha {
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

impl Hue for Lcha {
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

impl Luminance for Lcha {
    #[inline]
    fn with_luminance(&self, lightness: f32) -> Self {
        Self { lightness, ..*self }
    }

    fn luminance(&self) -> f32 {
        self.lightness
    }

    fn darker(&self, amount: f32) -> Self {
        Self::new(
            (self.lightness - amount).max(0.),
            self.chroma,
            self.hue,
            self.alpha,
        )
    }

    fn lighter(&self, amount: f32) -> Self {
        Self::new(
            (self.lightness + amount).min(1.),
            self.chroma,
            self.hue,
            self.alpha,
        )
    }
}

impl ColorToComponents for Lcha {
    fn to_f32_array(self) -> [f32; 4] {
        [self.lightness, self.chroma, self.hue, self.alpha]
    }

    fn to_f32_array_no_alpha(self) -> [f32; 3] {
        [self.lightness, self.chroma, self.hue]
    }

    fn to_vec4(self) -> Vec4 {
        Vec4::new(self.lightness, self.chroma, self.hue, self.alpha)
    }

    fn to_vec3(self) -> Vec3 {
        Vec3::new(self.lightness, self.chroma, self.hue)
    }

    fn from_f32_array(color: [f32; 4]) -> Self {
        Self {
            lightness: color[0],
            chroma: color[1],
            hue: color[2],
            alpha: color[3],
        }
    }

    fn from_f32_array_no_alpha(color: [f32; 3]) -> Self {
        Self {
            lightness: color[0],
            chroma: color[1],
            hue: color[2],
            alpha: 1.0,
        }
    }

    fn from_vec4(color: Vec4) -> Self {
        Self {
            lightness: color[0],
            chroma: color[1],
            hue: color[2],
            alpha: color[3],
        }
    }

    fn from_vec3(color: Vec3) -> Self {
        Self {
            lightness: color[0],
            chroma: color[1],
            hue: color[2],
            alpha: 1.0,
        }
    }
}

// -----------------------------------------------------------------------------
// Conversion

impl From<Lcha> for Laba {
    fn from(
        Lcha {
            lightness,
            chroma,
            hue,
            alpha,
        }: Lcha,
    ) -> Self {
        // Based on http://www.brucelindbloom.com/index.html?Eqn_LCH_to_Lab.html
        let l = lightness;
        let (sin, cos) = ops::sin_cos(hue.to_radians());
        let a = chroma * cos;
        let b = chroma * sin;

        Laba::new(l, a, b, alpha)
    }
}

impl From<Laba> for Lcha {
    fn from(
        Laba {
            lightness,
            a,
            b,
            alpha,
        }: Laba,
    ) -> Self {
        // Based on http://www.brucelindbloom.com/index.html?Eqn_Lab_to_LCH.html
        let chroma = ops::hypot(a, b);
        let hue = ops::atan2(b, a).to_degrees();
        let hue = if hue < 0.0 { hue + 360.0 } else { hue };

        Lcha::new(lightness, chroma, hue, alpha)
    }
}

// Derived conversions through Laba.
impl_from_via!(Laba, Lcha, [Srgba, LinearRgba, Xyza]);
