//! Provides a simple aspect ratio struct to help with calculations.

use core::fmt::{Display, Formatter};

use crate::Vec2;
use zlim_reflect::Reflect;

// -----------------------------------------------------------------------------
// AspectRatio

/// An `AspectRatio` is the ratio of width to height.
#[derive(Reflect, Copy, Clone, Debug, PartialEq, PartialOrd)]
#[reflect(Debug, Clone)]
#[repr(transparent)]
pub struct AspectRatio(f32);

impl AspectRatio {
    /// Standard 16:9 aspect ratio
    pub const SIXTEEN_NINE: Self = Self(16.0 / 9.0);
    /// Standard 4:3 aspect ratio
    pub const FOUR_THREE: Self = Self(4.0 / 3.0);
    /// Standard 21:9 ultrawide aspect ratio
    pub const ULTRAWIDE: Self = Self(21.0 / 9.0);

    /// Attempts to create a new [`AspectRatio`] from a given width and height.
    ///
    /// # Errors
    ///
    /// Returns an `Err` with [`AspectRatioError`] if:
    /// - Either width or height is zero ([`AspectRatioError::Zero`])
    /// - Either width or height is infinite ([`AspectRatioError::Infinite`])
    /// - Either width or height is NaN ([`AspectRatioError::NaN`])
    #[inline]
    pub const fn try_new(width: f32, height: f32) -> Result<Self, AspectRatioError> {
        match (width, height) {
            (w, h) if w == 0.0 || h == 0.0 => Err(AspectRatioError::Zero),
            (w, h) if w.is_infinite() || h.is_infinite() => Err(AspectRatioError::Infinite),
            (w, h) if w.is_nan() || h.is_nan() => Err(AspectRatioError::NaN),
            _ => Ok(Self(width / height)),
        }
    }

    /// Attempts to create a new [`AspectRatio`] from a given amount of x pixels and y pixels.
    #[inline]
    pub const fn try_from_pixels(x: u32, y: u32) -> Result<Self, AspectRatioError> {
        Self::try_new(x as f32, y as f32)
    }

    /// Returns the aspect ratio as an `f32` value.
    #[inline]
    pub const fn ratio(&self) -> f32 {
        self.0
    }

    /// Returns the inverse of this aspect ratio (height/width).
    #[inline]
    pub const fn inverse(&self) -> Self {
        Self(1.0 / self.0)
    }

    /// Returns true if the aspect ratio represents a landscape orientation.
    #[inline]
    pub const fn is_landscape(&self) -> bool {
        self.0 > 1.0
    }

    /// Returns true if the aspect ratio represents a portrait orientation.
    #[inline]
    pub const fn is_portrait(&self) -> bool {
        self.0 < 1.0
    }

    /// Returns true if the aspect ratio is exactly square.
    #[inline]
    pub const fn is_square(&self) -> bool {
        self.0 == 1.0
    }
}

impl TryFrom<Vec2> for AspectRatio {
    type Error = AspectRatioError;

    #[inline]
    fn try_from(value: Vec2) -> Result<Self, Self::Error> {
        Self::try_new(value.x, value.y)
    }
}

impl From<AspectRatio> for f32 {
    #[inline]
    fn from(value: AspectRatio) -> Self {
        value.0
    }
}

// -----------------------------------------------------------------------------
// AspectRatioError

/// An Error type for when [`AspectRatio`](`super::AspectRatio`) is provided invalid width or height values
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AspectRatioError {
    /// Error due to width or height having zero as a value.
    Zero,
    /// Error due to width or height being infinite.
    Infinite,
    /// Error due to width or height being Not a Number (NaN).
    NaN,
}

impl Display for AspectRatioError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Zero => f.write_str("AspectRatio error: width or height is zero"),
            Self::Infinite => f.write_str("AspectRatio error: width or height is infinite"),
            Self::NaN => f.write_str("AspectRatio error: width or height is NaN"),
        }
    }
}

impl core::error::Error for AspectRatioError {}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::{AspectRatio, AspectRatioError};
    use crate::Vec2;
    use crate::ops;

    #[test]
    fn standard_constants() {
        assert!(ops::abs(AspectRatio::SIXTEEN_NINE.ratio() - 16.0 / 9.0) < 1e-6);
        assert!(ops::abs(AspectRatio::FOUR_THREE.ratio() - 4.0 / 3.0) < 1e-6);
        assert!(ops::abs(AspectRatio::ULTRAWIDE.ratio() - 21.0 / 9.0) < 1e-6);
    }

    #[test]
    fn try_new_errors() {
        assert_eq!(AspectRatio::try_new(0.0, 2.0), Err(AspectRatioError::Zero));
        assert_eq!(AspectRatio::try_new(2.0, 0.0), Err(AspectRatioError::Zero));
        assert_eq!(
            AspectRatio::try_new(f32::INFINITY, 2.0),
            Err(AspectRatioError::Infinite)
        );
        assert_eq!(
            AspectRatio::try_new(2.0, f32::NEG_INFINITY),
            Err(AspectRatioError::Infinite)
        );
        assert_eq!(
            AspectRatio::try_new(f32::NAN, 2.0),
            Err(AspectRatioError::NaN)
        );
        assert_eq!(
            AspectRatio::try_new(2.0, f32::NAN),
            Err(AspectRatioError::NaN)
        );
    }

    #[test]
    fn try_new_ok() {
        let ratio = AspectRatio::try_new(1920.0, 1080.0).unwrap();
        assert!(ops::abs(ratio.ratio() - 16.0 / 9.0) < 1e-6);
    }

    #[test]
    fn try_from_pixels() {
        assert_eq!(
            AspectRatio::try_from_pixels(1920, 1080),
            AspectRatio::try_new(1920.0, 1080.0)
        );
        assert_eq!(
            AspectRatio::try_from_pixels(0, 1080),
            Err(AspectRatioError::Zero)
        );
    }

    #[test]
    fn accessors() {
        let landscape = AspectRatio::try_new(2.0, 1.0).unwrap();
        let portrait = AspectRatio::try_new(1.0, 2.0).unwrap();
        let square = AspectRatio::try_new(1.0, 1.0).unwrap();

        assert!(landscape.is_landscape());
        assert!(!landscape.is_portrait());
        assert!(!landscape.is_square());

        assert!(portrait.is_portrait());
        assert!(!portrait.is_landscape());

        assert!(square.is_square());
        assert!(!square.is_landscape());
        assert!(!square.is_portrait());

        // inverse swaps landscape/portrait
        assert!(ops::abs(landscape.inverse().ratio() - 0.5) < 1e-6);
        assert!(landscape.inverse().is_portrait());
    }

    #[test]
    fn conversions() {
        let ratio = AspectRatio::try_new(4.0, 3.0).unwrap();
        let f: f32 = ratio.into();
        assert!(ops::abs(f - 4.0 / 3.0) < 1e-6);

        let from_vec2 = AspectRatio::try_from(Vec2::new(16.0, 9.0)).unwrap();
        assert!(ops::abs(from_vec2.ratio() - 16.0 / 9.0) < 1e-6);
        assert_eq!(
            AspectRatio::try_from(Vec2::new(0.0, 1.0)),
            Err(AspectRatioError::Zero)
        );
    }

    #[test]
    fn display_and_error() {
        assert_eq!(
            AspectRatioError::Zero.to_string(),
            "AspectRatio error: width or height is zero"
        );
        assert_eq!(
            AspectRatioError::Infinite.to_string(),
            "AspectRatio error: width or height is infinite"
        );
        assert_eq!(
            AspectRatioError::NaN.to_string(),
            "AspectRatio error: width or height is NaN"
        );
        // Error trait is implemented.
        let _: &dyn core::error::Error = &AspectRatioError::NaN;
    }
}
