use core::f32::consts::FRAC_1_SQRT_2;
use core::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};
use zlim_reflect::derive::TypePath;

use super::InvalidDirectionError;
use crate::{Rot2, Vec2};

// -----------------------------------------------------------------------------
// Dir2

/// A normalized vector pointing in a direction in 2D space
#[derive(Debug, Clone, Copy, PartialEq)]
#[derive(TypePath, Serialize, Deserialize)]
#[serde(into = "adapter::Dir2", try_from = "adapter::Dir2")]
#[type_path = "zlim_math::Dir2"]
#[repr(transparent)]
#[doc(alias = "Direction2d")]
pub struct Dir2(Vec2);

// -----------------------------------------------------------------------------
// serialize

mod adapter {
    use serde::{Deserialize, Serialize};

    use crate::InvalidDirectionError;

    #[derive(Serialize, Deserialize)]
    pub(super) struct Dir2 {
        x: f32,
        y: f32,
    }

    impl TryFrom<Dir2> for super::Dir2 {
        type Error = InvalidDirectionError;
        #[inline]
        fn try_from(value: Dir2) -> Result<Self, Self::Error> {
            super::Dir2::from_xy(value.x, value.y)
        }
    }

    impl From<super::Dir2> for Dir2 {
        #[inline]
        fn from(value: super::Dir2) -> Self {
            Self {
                x: value.x,
                y: value.y,
            }
        }
    }
}

// -----------------------------------------------------------------------------
// reflect

mod reflect_impl {
    use std::borrow::Cow;

    use super::Dir2;
    use zlim_reflect::{
        Reflect,
        derive::impl_reflect,
        ops::{ApplyError, Struct, StructFieldIter},
    };

    impl_reflect! {
        #[reflect(Struct = false)]
        #[reflect(Default, Debug, Clone, Serialize, Deserialize)]
        #[reflect(reflect_apply = reflect_apply, from_reflect = from_reflect)]
        pub struct Dir2 { x: f32, y: f32 }
    }

    fn from_reflect(mut value: Box<dyn Reflect>) -> Result<Box<Dir2>, Box<dyn Reflect>> {
        match value.downcast::<Dir2>() {
            Ok(v) => return Ok(v),
            Err(e) => value = e,
        }
        let Ok(s) = value.reflect_ref().as_struct() else {
            return Err(value);
        };
        if s.field_len() != 2 {
            return Err(value);
        }
        let Some(x) = s.field("x") else {
            return Err(value);
        };
        let Some(&x) = x.downcast_ref::<f32>() else {
            return Err(value);
        };
        let Some(y) = s.field("y") else {
            return Err(value);
        };
        let Some(&y) = y.downcast_ref::<f32>() else {
            return Err(value);
        };
        Ok(Box::new(Dir2::from_xy(x, y).unwrap_or(Dir2::X)))
    }

    fn reflect_apply(this: &mut Dir2, other: &dyn Reflect) -> Result<(), ApplyError> {
        zlim_reflect::impls::struct_apply(this, other)?;
        *this = Dir2::new(this.0).unwrap_or(Dir2::X);
        Ok(())
    }

    impl Struct for Dir2 {
        fn field(&self, name: &str) -> Option<&dyn Reflect> {
            match name {
                "x" => Some(&self.x),
                "y" => Some(&self.y),
                _ => None,
            }
        }

        fn field_mut(&mut self, name: &str) -> Option<&mut dyn Reflect> {
            match name {
                "x" => Some(&mut self.0.x),
                "y" => Some(&mut self.0.y),
                _ => None,
            }
        }

        fn field_at(&self, index: usize) -> Option<&dyn Reflect> {
            match index {
                0 => Some(&self.x),
                1 => Some(&self.y),
                _ => None,
            }
        }

        fn field_at_mut(&mut self, index: usize) -> Option<&mut dyn Reflect> {
            match index {
                0 => Some(&mut self.0.x),
                1 => Some(&mut self.0.y),
                _ => None,
            }
        }

        fn name_at(&self, index: usize) -> Option<&str> {
            match index {
                0 => Some("x"),
                1 => Some("y"),
                _ => None,
            }
        }

        fn index_of(&self, name: &str) -> Option<usize> {
            match name {
                "x" => Some(0),
                "y" => Some(1),
                _ => None,
            }
        }

        fn field_len(&self) -> usize {
            2
        }

        fn iter_fields(&self) -> StructFieldIter<'_> {
            StructFieldIter::new(self)
        }

        fn unpack(self: Box<Self>) -> Vec<(Cow<'static, str>, Box<dyn Reflect>)> {
            vec![
                (Cow::Borrowed("x"), Box::new(self.x) as Box<dyn Reflect>),
                (Cow::Borrowed("y"), Box::new(self.y) as Box<dyn Reflect>),
            ]
        }
    }
}

// -----------------------------------------------------------------------------
// common

impl Dir2 {
    /// A unit vector pointing along the positive X axis.
    pub const X: Self = Self(Vec2::X);
    /// A unit vector pointing along the positive Y axis.
    pub const Y: Self = Self(Vec2::Y);
    /// A unit vector pointing along the negative X axis.
    pub const NEG_X: Self = Self(Vec2::NEG_X);
    /// A unit vector pointing along the negative Y axis.
    pub const NEG_Y: Self = Self(Vec2::NEG_Y);
    /// The directional axes.
    pub const AXES: [Self; 2] = [Self::X, Self::Y];
    /// The cardinal directions.
    pub const CARDINALS: [Self; 4] = [Self::X, Self::NEG_X, Self::Y, Self::NEG_Y];

    /// The "north" direction, equivalent to [`Dir2::Y`].
    pub const NORTH: Self = Self(Vec2::Y);
    /// The "south" direction, equivalent to [`Dir2::NEG_Y`].
    pub const SOUTH: Self = Self(Vec2::NEG_Y);
    /// The "east" direction, equivalent to [`Dir2::X`].
    pub const EAST: Self = Self(Vec2::X);
    /// The "west" direction, equivalent to [`Dir2::NEG_X`].
    pub const WEST: Self = Self(Vec2::NEG_X);
    /// The "north-east" direction, between [`Dir2::NORTH`] and [`Dir2::EAST`].
    pub const NORTH_EAST: Self = Self(Vec2::new(FRAC_1_SQRT_2, FRAC_1_SQRT_2));
    /// The "north-west" direction, between [`Dir2::NORTH`] and [`Dir2::WEST`].
    pub const NORTH_WEST: Self = Self(Vec2::new(-FRAC_1_SQRT_2, FRAC_1_SQRT_2));
    /// The "south-east" direction, between [`Dir2::SOUTH`] and [`Dir2::EAST`].
    pub const SOUTH_EAST: Self = Self(Vec2::new(FRAC_1_SQRT_2, -FRAC_1_SQRT_2));
    /// The "south-west" direction, between [`Dir2::SOUTH`] and [`Dir2::WEST`].
    pub const SOUTH_WEST: Self = Self(Vec2::new(-FRAC_1_SQRT_2, -FRAC_1_SQRT_2));

    /// The diagonals between the cardinal directions.
    pub const DIAGONALS: [Self; 4] = [
        Self::NORTH_EAST,
        Self::NORTH_WEST,
        Self::SOUTH_EAST,
        Self::SOUTH_WEST,
    ];
    /// All neighbors of a tile on a square grid in a 3x3 neighborhood. A combination of [`Self::CARDINALS`] and [`Self::DIAGONALS`]
    pub const ALL_NEIGHBORS: [Self; 8] = [
        Self::X,
        Self::NEG_X,
        Self::Y,
        Self::NEG_Y,
        Self::NORTH_EAST,
        Self::NORTH_WEST,
        Self::SOUTH_EAST,
        Self::SOUTH_WEST,
    ];

    /// Create a direction from a finite, nonzero [`Vec2`], normalizing it.
    ///
    /// Returns [`Err(InvalidDirectionError)`](InvalidDirectionError) if the length
    /// of the given vector is zero (or very close to zero), infinite, or `NaN`.
    pub fn new(value: Vec2) -> Result<Self, InvalidDirectionError> {
        Self::new_and_length(value).map(|(dir, _)| dir)
    }

    /// Create a [`Dir2`] from a [`Vec2`] that is already normalized.
    ///
    /// # Warning
    ///
    /// `value` must be normalized, i.e its length must be `1.0`.
    pub fn new_unchecked(value: Vec2) -> Self {
        #[cfg(debug_assertions)]
        super::assert_is_normalized(
            "The vector given to `Dir2::new_unchecked` is not normalized.",
            value.length_squared(),
        );

        Self(value)
    }

    /// Create a direction from a finite, nonzero [`Vec2`], normalizing it and
    /// also returning its original length.
    ///
    /// Returns [`Err(InvalidDirectionError)`](InvalidDirectionError) if the length
    /// of the given vector is zero (or very close to zero), infinite, or `NaN`.
    pub fn new_and_length(value: Vec2) -> Result<(Self, f32), InvalidDirectionError> {
        let length = value.length();
        let direction = (length.is_finite() && length > 0.0).then_some(value / length);

        direction
            .map(|dir| (Self(dir), length))
            .ok_or(InvalidDirectionError::from_length(length))
    }

    /// Create a direction from its `x` and `y` components.
    ///
    /// Returns [`Err(InvalidDirectionError)`](InvalidDirectionError) if the length
    /// of the vector formed by the components is zero (or very close to zero), infinite, or `NaN`.
    pub fn from_xy(x: f32, y: f32) -> Result<Self, InvalidDirectionError> {
        Self::new(Vec2::new(x, y))
    }

    /// Create a direction from its `x` and `y` components, assuming the resulting vector is normalized.
    ///
    /// # Warning
    ///
    /// The vector produced from `x` and `y` must be normalized, i.e its length must be `1.0`.
    pub fn from_xy_unchecked(x: f32, y: f32) -> Self {
        Self::new_unchecked(Vec2::new(x, y))
    }

    /// Creates a 2D direction containing `[angle.cos(), angle.sin()]`.
    #[inline]
    pub fn from_angle(angle: f32) -> Self {
        Self(Vec2::from_angle(angle))
    }

    /// Returns the inner [`Vec2`]
    #[inline]
    pub const fn as_vec2(&self) -> Vec2 {
        self.0
    }

    /// Performs a spherical linear interpolation between `self` and `rhs`
    /// based on the value `s`.
    ///
    /// This corresponds to interpolating between the two directions at a constant angular velocity.
    ///
    /// When `s == 0.0`, the result will be equal to `self`.
    /// When `s == 1.0`, the result will be equal to `rhs`.
    ///
    /// # Example
    ///
    /// ```
    /// # use zlim_math::Dir2;
    /// # use approx::{assert_relative_eq, RelativeEq};
    /// #
    /// let dir1 = Dir2::X;
    /// let dir2 = Dir2::Y;
    ///
    /// let result1 = dir1.slerp(dir2, 1.0 / 3.0);
    /// #[cfg(feature = "approx")]
    /// assert_relative_eq!(result1, Dir2::from_xy(0.75_f32.sqrt(), 0.5).unwrap());
    ///
    /// let result2 = dir1.slerp(dir2, 0.5);
    /// #[cfg(feature = "approx")]
    /// assert_relative_eq!(result2, Dir2::from_xy(0.5_f32.sqrt(), 0.5_f32.sqrt()).unwrap());
    /// ```
    #[inline]
    pub fn slerp(self, rhs: Self, s: f32) -> Self {
        let angle = self.angle_to(rhs.0);
        Rot2::radians(angle * s) * self
    }

    /// Get the rotation that rotates this direction to `other`.
    #[inline]
    pub fn rotation_to(self, other: Self) -> Rot2 {
        // Rotate `self` to X-axis, then X-axis to `other`:
        other.rotation_from_x() * self.rotation_to_x()
    }

    /// Get the rotation that rotates `other` to this direction.
    #[inline]
    pub fn rotation_from(self, other: Self) -> Rot2 {
        other.rotation_to(self)
    }

    /// Get the rotation that rotates the X-axis to this direction.
    #[inline]
    pub fn rotation_from_x(self) -> Rot2 {
        Rot2::from_sin_cos(self.0.y, self.0.x)
    }

    /// Get the rotation that rotates this direction to the X-axis.
    #[inline]
    pub fn rotation_to_x(self) -> Rot2 {
        // (This is cheap, it just negates one component.)
        self.rotation_from_x().inverse()
    }

    /// Get the rotation that rotates the Y-axis to this direction.
    #[inline]
    pub fn rotation_from_y(self) -> Rot2 {
        // `x <- y`, `y <- -x` correspond to rotating clockwise by pi/2;
        // this transforms the Y-axis into the X-axis, maintaining the relative position
        // of our direction. Then we just use the same technique as `rotation_from_x`.
        Rot2::from_sin_cos(-self.0.x, self.0.y)
    }

    /// Get the rotation that rotates this direction to the Y-axis.
    #[inline]
    pub fn rotation_to_y(self) -> Rot2 {
        self.rotation_from_y().inverse()
    }

    /// Returns `self` after an approximate normalization, assuming the value is already nearly normalized.
    /// Useful for preventing numerical error accumulation.
    /// See [`Dir3::fast_renormalize`] for an example of when such error accumulation might occur.
    #[inline]
    pub fn fast_renormalize(self) -> Self {
        let length_squared = self.0.length_squared();
        // Based on a Taylor approximation of the inverse square root, see [`Dir3::fast_renormalize`] for more details.
        Self(self * (0.5 * (3.0 - length_squared)))
    }

    /// Returns the perpendicular vector rotated to 90 degrees counterclockwise.
    #[inline]
    pub fn perpendicular(self) -> Self {
        Self::new_unchecked(self.as_vec2().perp())
    }
}

impl Default for Dir2 {
    fn default() -> Self {
        Self::X
    }
}

impl TryFrom<Vec2> for Dir2 {
    type Error = InvalidDirectionError;

    fn try_from(value: Vec2) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Dir2> for Vec2 {
    fn from(value: Dir2) -> Self {
        value.as_vec2()
    }
}

impl core::ops::Deref for Dir2 {
    type Target = Vec2;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::Neg for Dir2 {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

impl core::ops::Mul<f32> for Dir2 {
    type Output = Vec2;
    fn mul(self, rhs: f32) -> Self::Output {
        self.0 * rhs
    }
}

impl core::ops::Mul<Dir2> for f32 {
    type Output = Vec2;
    fn mul(self, rhs: Dir2) -> Self::Output {
        self * rhs.0
    }
}

impl core::ops::Mul<Dir2> for Rot2 {
    type Output = Dir2;

    /// Rotates the [`Dir2`] using a [`Rot2`].
    fn mul(self, direction: Dir2) -> Self::Output {
        let rotated = self * *direction;

        #[cfg(debug_assertions)]
        super::assert_is_normalized(
            "`Dir2` is denormalized after rotation.",
            rotated.length_squared(),
        );

        Dir2(rotated)
    }
}

impl Display for Dir2 {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

// -----------------------------------------------------------------------------
// approx

#[cfg(any(feature = "approx", test))]
impl approx::AbsDiffEq for Dir2 {
    type Epsilon = f32;
    fn default_epsilon() -> f32 {
        f32::EPSILON
    }
    fn abs_diff_eq(&self, other: &Self, epsilon: f32) -> bool {
        self.as_ref().abs_diff_eq(other.as_ref(), epsilon)
    }
}

#[cfg(any(feature = "approx", test))]
impl approx::RelativeEq for Dir2 {
    fn default_max_relative() -> f32 {
        f32::EPSILON
    }
    fn relative_eq(&self, other: &Self, epsilon: f32, max_relative: f32) -> bool {
        self.as_ref()
            .relative_eq(other.as_ref(), epsilon, max_relative)
    }
}

#[cfg(any(feature = "approx", test))]
impl approx::UlpsEq for Dir2 {
    fn default_max_ulps() -> u32 {
        4
    }
    fn ulps_eq(&self, other: &Self, epsilon: f32, max_ulps: u32) -> bool {
        self.as_ref().ulps_eq(other.as_ref(), epsilon, max_ulps)
    }
}

// ---------------------------------------------------------------------
// Dir3

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops;
    use approx::assert_relative_eq;

    #[test]
    fn dir2_creation() {
        assert_eq!(Dir2::new(Vec2::X * 12.5), Ok(Dir2::X));
        assert_eq!(
            Dir2::new(Vec2::new(0.0, 0.0)),
            Err(InvalidDirectionError::Zero)
        );
        assert_eq!(
            Dir2::new(Vec2::new(f32::INFINITY, 0.0)),
            Err(InvalidDirectionError::Infinite)
        );
        assert_eq!(
            Dir2::new(Vec2::new(f32::NEG_INFINITY, 0.0)),
            Err(InvalidDirectionError::Infinite)
        );
        assert_eq!(
            Dir2::new(Vec2::new(f32::NAN, 0.0)),
            Err(InvalidDirectionError::NaN)
        );
        assert_eq!(Dir2::new_and_length(Vec2::X * 6.5), Ok((Dir2::X, 6.5)));
    }

    /// The interpolation follows the arc between the two directions, so the halfway point of
    /// the quarter turn from `X` to `Y` is the 45° diagonal, while a third of the way back
    /// from `Y` towards `X` and two thirds of the way forward from `X` both land on the 60°
    /// direction.
    #[test]
    fn dir2_slerp() {
        assert_relative_eq!(
            Dir2::X.slerp(Dir2::Y, 0.5),
            Dir2::from_xy(ops::sqrt(0.5_f32), ops::sqrt(0.5_f32)).unwrap()
        );
        assert_eq!(Dir2::Y.slerp(Dir2::X, 0.0), Dir2::Y);
        assert_relative_eq!(Dir2::X.slerp(Dir2::Y, 1.0), Dir2::Y);
        assert_relative_eq!(
            Dir2::Y.slerp(Dir2::X, 1.0 / 3.0),
            Dir2::from_xy(0.5, ops::sqrt(0.75_f32)).unwrap()
        );
        assert_relative_eq!(
            Dir2::X.slerp(Dir2::Y, 2.0 / 3.0),
            Dir2::from_xy(0.5, ops::sqrt(0.75_f32)).unwrap()
        );
    }

    /// Each of the six rotation helpers is checked at 45°, 90° or 135°, covering both the
    /// direction-to-direction and the direction-to-axis forms, which also pins down that all of
    /// them measure the rotation counterclockwise.
    #[test]
    fn dir2_to_rotation2d() {
        assert_relative_eq!(Dir2::EAST.rotation_to(Dir2::NORTH_EAST), Rot2::FRAC_PI_4);
        assert_relative_eq!(Dir2::NORTH.rotation_from(Dir2::NORTH_EAST), Rot2::FRAC_PI_4);
        assert_relative_eq!(Dir2::SOUTH.rotation_to_x(), Rot2::FRAC_PI_2);
        assert_relative_eq!(Dir2::SOUTH.rotation_to_y(), Rot2::PI);
        assert_relative_eq!(Dir2::NORTH_WEST.rotation_from_x(), Rot2::degrees(135.0));
        assert_relative_eq!(Dir2::NORTH_WEST.rotation_from_y(), Rot2::FRAC_PI_4);
    }

    #[test]
    fn dir2_renorm() {
        // Evil denormalized Rot2
        let (sin, cos) = ops::sin_cos(1.0_f32);
        let rot2 = Rot2::from_sin_cos(sin * (1.0 + 1e-5), cos * (1.0 + 1e-5));
        let mut dir_a = Dir2::X;
        let mut dir_b = Dir2::X;

        // We test that renormalizing an already normalized dir doesn't do anything
        assert_relative_eq!(dir_b, dir_b.fast_renormalize(), epsilon = 0.000001);

        for _ in 0..50 {
            dir_a = rot2 * dir_a;
            dir_b = rot2 * dir_b;
            dir_b = dir_b.fast_renormalize();
        }

        // `dir_a` should've gotten denormalized, meanwhile `dir_b` should stay normalized.
        assert!(
            !dir_a.is_normalized(),
            "Denormalization doesn't work, test is faulty"
        );
        assert!(dir_b.is_normalized(), "Renormalisation did not work.");
    }

    #[test]
    fn dir2_perp() {
        // (1, 0) rotated 90 deg counterclockwise becomes (0, 1)
        assert_eq!(Dir2::X.perpendicular(), Dir2::Y);

        // (0, 1) rotated 90 deg counterclockwise becomes (-1, 0)
        assert_eq!(Dir2::Y.perpendicular(), Dir2::NEG_X);

        // (-1, 0) rotated 90 deg counterclockwise becomes (0, -1)
        assert_eq!(Dir2::NEG_X.perpendicular(), Dir2::NEG_Y);

        // (0, -1) rotated 90 deg counterclockwise becomes (1, 0)
        assert_eq!(Dir2::NEG_Y.perpendicular(), Dir2::X);
    }
}
