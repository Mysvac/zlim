use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use zlim_math::{Vec3, Vec4, ops};
use zlim_reflect::derive::Reflect;

use crate::color_difference::EuclideanDistance;
use crate::{Alpha, ColorToComponents, ColorToPacked, Gray, Luminance, Mix};
use crate::{impl_componentwise_stable_interpolate, impl_componentwise_vector_space};

// -----------------------------------------------------------------------------
// Conversion

/// Linear RGB color with alpha.
///
/// SDR colors are in `[0.0, 1.0]`. Values above `1.0` are HDR intensities.
///
#[doc = include_str!("../docs/conversion.md")]
///
/// <div>
#[doc = include_str!("../docs/diagrams/model_graph.svg")]
/// </div>
#[derive(Debug, Clone, Copy, PartialEq)]
#[derive(Pod, Zeroable, Reflect, Serialize, Deserialize)]
#[reflect(Debug, Clone, Default, Serialize, Deserialize)]
#[repr(C)]
pub struct LinearRgba {
    /// The red channel. [0.0, 1.0] for SDR colors.
    pub red: f32,
    /// The green channel. [0.0, 1.0] for SDR colors.
    pub green: f32,
    /// The blue channel. [0.0, 1.0] for SDR colors.
    pub blue: f32,
    /// The alpha channel. [0.0, 1.0]
    pub alpha: f32,
}

impl_componentwise_vector_space!(LinearRgba, [red, green, blue, alpha]);
impl_componentwise_stable_interpolate!(LinearRgba, [red, green, blue, alpha]);

impl LinearRgba {
    /// A fully black color with full alpha.
    pub const BLACK: Self = Self {
        red: 0.0,
        green: 0.0,
        blue: 0.0,
        alpha: 1.0,
    };

    /// A fully white color with full alpha.
    pub const WHITE: Self = Self {
        red: 1.0,
        green: 1.0,
        blue: 1.0,
        alpha: 1.0,
    };

    /// A fully transparent color.
    pub const NONE: Self = Self {
        red: 0.0,
        green: 0.0,
        blue: 0.0,
        alpha: 0.0,
    };

    /// A fully red color with full alpha.
    pub const RED: Self = Self {
        red: 1.0,
        green: 0.0,
        blue: 0.0,
        alpha: 1.0,
    };

    /// A fully green color with full alpha.
    pub const GREEN: Self = Self {
        red: 0.0,
        green: 1.0,
        blue: 0.0,
        alpha: 1.0,
    };

    /// A fully blue color with full alpha.
    pub const BLUE: Self = Self {
        red: 0.0,
        green: 0.0,
        blue: 1.0,
        alpha: 1.0,
    };

    /// An invalid color.
    ///
    /// This type can be used to represent an invalid color value;
    /// in some rendering applications the color will be ignored,
    /// enabling performant hacks like hiding lines by setting their color to `INVALID`.
    pub const NAN: Self = Self {
        red: f32::NAN,
        green: f32::NAN,
        blue: f32::NAN,
        alpha: f32::NAN,
    };

    /// Construct a new [`LinearRgba`] color from components.
    pub const fn new(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    /// Construct a new [`LinearRgba`] color from (r, g, b) components, with the default alpha (1.0).
    ///
    /// # Arguments
    ///
    /// * `red` - Red channel. [0.0, 1.0]
    /// * `green` - Green channel. [0.0, 1.0]
    /// * `blue` - Blue channel. [0.0, 1.0]
    pub const fn rgb(red: f32, green: f32, blue: f32) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 1.0,
        }
    }

    /// Return a copy of this color with the red channel set to the given value.
    pub const fn with_red(self, red: f32) -> Self {
        Self { red, ..self }
    }

    /// Return a copy of this color with the green channel set to the given value.
    pub const fn with_green(self, green: f32) -> Self {
        Self { green, ..self }
    }

    /// Return a copy of this color with the blue channel set to the given value.
    pub const fn with_blue(self, blue: f32) -> Self {
        Self { blue, ..self }
    }

    /// Make the color lighter or darker by some amount
    fn adjust_lightness(&mut self, amount: f32) {
        let luminance = self.luminance();
        let target_luminance = (luminance + amount).clamp(0.0, 1.0);
        if target_luminance < luminance {
            let adjustment = (luminance - target_luminance) / luminance;
            self.mix_assign(Self::new(0.0, 0.0, 0.0, self.alpha), adjustment);
        } else if target_luminance > luminance {
            let adjustment = (target_luminance - luminance) / (1. - luminance);
            self.mix_assign(Self::new(1.0, 1.0, 1.0, self.alpha), adjustment);
        }
    }

    /// Converts this color to a `u32`.
    ///
    /// Maps the RGBA channels in RGBA order to a little-endian byte array (GPUs are little-endian).
    /// `A` will be the most significant byte and `R` the least significant.
    pub fn as_u32(&self) -> u32 {
        u32::from_le_bytes(self.to_u8_array())
    }
}

impl Default for LinearRgba {
    /// Construct a new [`LinearRgba`] color with the default values (white with full alpha).
    fn default() -> Self {
        Self::WHITE
    }
}

// -----------------------------------------------------------------------------
// Color Traits

impl Luminance for LinearRgba {
    /// Luminance calculated using the [CIE XYZ formula](https://en.wikipedia.org/wiki/Relative_luminance).
    #[inline]
    fn luminance(&self) -> f32 {
        self.red * 0.2126 + self.green * 0.7152 + self.blue * 0.0722
    }

    /// Scales the color to the target luminance, preserving its chromaticity. A saturated
    /// color or a target above 1.0 can push components outside `[0.0, 1.0]`.
    #[inline]
    fn with_luminance(&self, luminance: f32) -> Self {
        let current_luminance = self.luminance();
        let adjustment = luminance / current_luminance;
        Self {
            red: self.red * adjustment,
            green: self.green * adjustment,
            blue: self.blue * adjustment,
            alpha: self.alpha,
        }
    }

    /// The target luminance is clamped to `[0.0, 1.0]`, so this is not suitable for HDR
    /// colors. To scale an HDR color, use [`with_luminance`](Luminance::with_luminance).
    #[inline]
    fn darker(&self, amount: f32) -> Self {
        let mut result = *self;
        result.adjust_lightness(-amount);
        result
    }

    /// The target luminance is clamped to `[0.0, 1.0]`, so this is not suitable for HDR
    /// colors. To scale an HDR color, use [`with_luminance`](Luminance::with_luminance).
    #[inline]
    fn lighter(&self, amount: f32) -> Self {
        let mut result = *self;
        result.adjust_lightness(amount);
        result
    }
}

impl Mix for LinearRgba {
    #[inline]
    fn mix(&self, other: &Self, factor: f32) -> Self {
        let n_factor = 1.0 - factor;
        Self {
            red: self.red * n_factor + other.red * factor,
            green: self.green * n_factor + other.green * factor,
            blue: self.blue * n_factor + other.blue * factor,
            alpha: self.alpha * n_factor + other.alpha * factor,
        }
    }
}

impl Gray for LinearRgba {
    const BLACK: Self = Self::BLACK;
    const WHITE: Self = Self::WHITE;
}

impl Alpha for LinearRgba {
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

impl EuclideanDistance for LinearRgba {
    #[inline]
    fn distance_squared(&self, other: &Self) -> f32 {
        let dr = self.red - other.red;
        let dg = self.green - other.green;
        let db = self.blue - other.blue;
        dr * dr + dg * dg + db * db
    }
}

impl ColorToComponents for LinearRgba {
    fn to_f32_array(self) -> [f32; 4] {
        [self.red, self.green, self.blue, self.alpha]
    }

    fn to_f32_array_no_alpha(self) -> [f32; 3] {
        [self.red, self.green, self.blue]
    }

    fn to_vec4(self) -> Vec4 {
        Vec4::new(self.red, self.green, self.blue, self.alpha)
    }

    fn to_vec3(self) -> Vec3 {
        Vec3::new(self.red, self.green, self.blue)
    }

    fn from_f32_array(color: [f32; 4]) -> Self {
        Self {
            red: color[0],
            green: color[1],
            blue: color[2],
            alpha: color[3],
        }
    }

    fn from_f32_array_no_alpha(color: [f32; 3]) -> Self {
        Self {
            red: color[0],
            green: color[1],
            blue: color[2],
            alpha: 1.0,
        }
    }

    fn from_vec4(color: Vec4) -> Self {
        Self {
            red: color[0],
            green: color[1],
            blue: color[2],
            alpha: color[3],
        }
    }

    fn from_vec3(color: Vec3) -> Self {
        Self {
            red: color[0],
            green: color[1],
            blue: color[2],
            alpha: 1.0,
        }
    }
}

impl ColorToPacked for LinearRgba {
    fn to_u8_array(self) -> [u8; 4] {
        [self.red, self.green, self.blue, self.alpha]
            .map(|v| ops::round(v.clamp(0.0, 1.0) * 255.0) as u8)
    }

    fn to_u8_array_no_alpha(self) -> [u8; 3] {
        [self.red, self.green, self.blue].map(|v| ops::round(v.clamp(0.0, 1.0) * 255.0) as u8)
    }

    fn from_u8_array(color: [u8; 4]) -> Self {
        Self::from_f32_array(color.map(|u| u as f32 / 255.0))
    }

    fn from_u8_array_no_alpha(color: [u8; 3]) -> Self {
        Self::from_f32_array_no_alpha(color.map(|u| u as f32 / 255.0))
    }
}

// -----------------------------------------------------------------------------
// wgpu_types & encase

impl From<LinearRgba> for wgpu_types::Color {
    fn from(color: LinearRgba) -> Self {
        wgpu_types::Color {
            r: color.red as f64,
            g: color.green as f64,
            b: color.blue as f64,
            a: color.alpha as f64,
        }
    }
}

// [`LinearRgba`] is intended to be used with shaders. So it's the only color
// type that implements [`ShaderType`] to make it easier to use inside shaders.
mod encase_impls {
    use super::LinearRgba;
    use encase::internal::{AlignmentValue, SizeValue};
    use encase::internal::{BufferMut, BufferRef, ReadFrom, Reader};
    use encase::internal::{CreateFrom, WriteInto, Writer};
    use encase::private::Metadata;
    use encase::{ShaderSize, ShaderType};

    impl ShaderType for LinearRgba {
        type ExtraMetadata = ();

        const METADATA: Metadata<Self::ExtraMetadata> = {
            let min_size = SizeValue::from(<f32 as ShaderSize>::SHADER_SIZE).mul(4);
            let alignment = AlignmentValue::from_next_power_of_two_size(min_size);
            Metadata {
                alignment,
                has_uniform_min_alignment: false,
                min_size,
                is_pod: true,
                extra: (),
            }
        };

        // const UNIFORM_COMPAT_ASSERT: fn() = || {};
    }

    impl WriteInto for LinearRgba {
        fn write_into<B: BufferMut>(&self, writer: &mut Writer<B>) {
            WriteInto::write_into(&self.red, writer);
            WriteInto::write_into(&self.green, writer);
            WriteInto::write_into(&self.blue, writer);
            WriteInto::write_into(&self.alpha, writer);
        }
    }

    impl ReadFrom for LinearRgba {
        fn read_from<B: BufferRef>(&mut self, reader: &mut Reader<B>) {
            ReadFrom::read_from(&mut self.red, reader);
            ReadFrom::read_from(&mut self.green, reader);
            ReadFrom::read_from(&mut self.blue, reader);
            ReadFrom::read_from(&mut self.alpha, reader);
        }
    }

    impl CreateFrom for LinearRgba {
        fn create_from<B: BufferRef>(reader: &mut Reader<B>) -> Self {
            LinearRgba {
                red: CreateFrom::create_from(reader),
                green: CreateFrom::create_from(reader),
                blue: CreateFrom::create_from(reader),
                alpha: CreateFrom::create_from(reader),
            }
        }
    }

    impl ShaderSize for LinearRgba {}
}

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euclidean_distance() {
        // White to black
        let a = LinearRgba::new(0.0, 0.0, 0.0, 1.0);
        let b = LinearRgba::new(1.0, 1.0, 1.0, 1.0);
        assert_eq!(a.distance_squared(&b), 3.0);

        // Alpha shouldn't matter
        let a = LinearRgba::new(0.0, 0.0, 0.0, 1.0);
        let b = LinearRgba::new(1.0, 1.0, 1.0, 0.0);
        assert_eq!(a.distance_squared(&b), 3.0);

        // Red to green
        let a = LinearRgba::new(0.0, 0.0, 0.0, 1.0);
        let b = LinearRgba::new(1.0, 0.0, 0.0, 1.0);
        assert_eq!(a.distance_squared(&b), 1.0);
    }

    #[test]
    fn to_and_from_u8() {
        // from_u8_array
        let a = LinearRgba::from_u8_array([255, 0, 0, 255]);
        let b = LinearRgba::new(1.0, 0.0, 0.0, 1.0);
        assert_eq!(a, b);

        // from_u8_array_no_alpha
        let a = LinearRgba::from_u8_array_no_alpha([255, 255, 0]);
        let b = LinearRgba::rgb(1.0, 1.0, 0.0);
        assert_eq!(a, b);

        // to_u8_array
        let a = LinearRgba::new(0.0, 0.0, 1.0, 1.0).to_u8_array();
        let b = [0, 0, 255, 255];
        assert_eq!(a, b);

        // to_u8_array_no_alpha
        let a = LinearRgba::rgb(0.0, 1.0, 1.0).to_u8_array_no_alpha();
        let b = [0, 255, 255];
        assert_eq!(a, b);

        // clamping
        let a = LinearRgba::rgb(0.0, 100.0, -100.0).to_u8_array_no_alpha();
        let b = [0, 255, 0];
        assert_eq!(a, b);
    }

    #[test]
    fn with_luminance_scales_unclamped() {
        use crate::tests::assert_approx_eq;

        // A target above 1.0 produces an HDR color.
        let gray = LinearRgba::rgb(0.5, 0.5, 0.5);
        let bright = gray.with_luminance(2.0);
        assert_approx_eq!(bright.red, 2.0, 1e-4);
        assert_approx_eq!(bright.green, 2.0, 1e-4);
        assert_approx_eq!(bright.blue, 2.0, 1e-4);

        // An HDR color scales and keeps its chromaticity.
        let hdr = LinearRgba::rgb(2.0, 4.0, 8.0);
        let adjusted = hdr.with_luminance(2.0 * hdr.luminance());
        assert_approx_eq!(adjusted.red, 4.0, 1e-4);
        assert_approx_eq!(adjusted.green, 8.0, 1e-4);
        assert_approx_eq!(adjusted.blue, 16.0, 1e-4);

        // A saturated SDR color scales past 1.0 instead of clamping, and the
        // result hits the requested luminance exactly.
        let red = LinearRgba::rgb(1.0, 0.0, 0.0);
        let bright = red.with_luminance(0.9);
        assert!(bright.red > 4.0);
        assert_eq!(bright.green, 0.0);
        assert_eq!(bright.blue, 0.0);
        assert_approx_eq!(bright.luminance(), 0.9, 1e-4);
    }

    #[test]
    fn darker_lighter() {
        // Darker and lighter should be commutative.
        let color = LinearRgba::new(0.4, 0.5, 0.6, 1.0);
        let darker1 = color.darker(0.1);
        let darker2 = darker1.darker(0.1);
        let twice_as_dark = color.darker(0.2);
        assert!(darker2.distance_squared(&twice_as_dark) < 0.0001);

        let lighter1 = color.lighter(0.1);
        let lighter2 = lighter1.lighter(0.1);
        let twice_as_light = color.lighter(0.2);
        assert!(lighter2.distance_squared(&twice_as_light) < 0.0001);
    }
}
