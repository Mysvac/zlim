use core::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};
use zlim_reflect::derive::TypePath;

use super::Dir3;
use super::InvalidDirectionError;
use crate::{Quat, Vec3, Vec3A};

// -----------------------------------------------------------------------------
// Dir3A

/// A normalized SIMD vector pointing in a direction in 3D space.
///
/// This type stores a 16 byte aligned [`Vec3A`].
/// This may or may not be faster than [`Dir3`]: make sure to benchmark!
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[derive(TypePath)]
#[serde(into = "adapter::Dir3A", try_from = "adapter::Dir3A")]
#[type_path = "zlim_math::Dir3A"]
#[repr(transparent)]
#[doc(alias = "Direction3dA")]
pub struct Dir3A(Vec3A);

// -----------------------------------------------------------------------------
// serialize

mod adapter {
    use serde::{Deserialize, Serialize};

    use crate::InvalidDirectionError;

    #[derive(Serialize, Deserialize)]
    pub(super) struct Dir3A {
        x: f32,
        y: f32,
        z: f32,
    }

    impl TryFrom<Dir3A> for super::Dir3A {
        type Error = InvalidDirectionError;
        #[inline]
        fn try_from(value: Dir3A) -> Result<Self, Self::Error> {
            super::Dir3A::from_xyz(value.x, value.y, value.z)
        }
    }

    impl From<super::Dir3A> for Dir3A {
        #[inline]
        fn from(value: super::Dir3A) -> Self {
            Self {
                x: value.x,
                y: value.y,
                z: value.z,
            }
        }
    }
}

// -----------------------------------------------------------------------------
// reflect

mod reflect_impl {
    use std::borrow::Cow;

    use zlim_reflect::{
        Reflect,
        derive::impl_reflect,
        ops::{ApplyError, Struct, StructFieldIter},
    };

    use super::Dir3A;

    impl_reflect! {
        #[reflect(Struct = false)]
        #[reflect(Default, Debug, Clone, Serialize, Deserialize)]
        #[reflect(reflect_apply = reflect_apply, from_reflect = from_reflect)]
        pub struct Dir3A { x: f32, y: f32, z: f32 }
    }

    fn from_reflect(mut value: Box<dyn Reflect>) -> Result<Box<Dir3A>, Box<dyn Reflect>> {
        match value.downcast::<Dir3A>() {
            Ok(v) => return Ok(v),
            Err(e) => value = e,
        }
        let Ok(s) = value.reflect_ref().as_struct() else {
            return Err(value);
        };
        if s.field_len() != 3 {
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
        let Some(z) = s.field("z") else {
            return Err(value);
        };
        let Some(&z) = z.downcast_ref::<f32>() else {
            return Err(value);
        };
        Ok(Box::new(Dir3A::from_xyz(x, y, z).unwrap_or(Dir3A::X)))
    }

    fn reflect_apply(this: &mut Dir3A, other: &dyn Reflect) -> Result<(), ApplyError> {
        zlim_reflect::impls::struct_apply(this, other)?;
        *this = Dir3A::new(this.0).unwrap_or(Dir3A::X);
        Ok(())
    }

    impl Struct for Dir3A {
        fn field(&self, name: &str) -> Option<&dyn Reflect> {
            match name {
                "x" => Some(&self.x),
                "y" => Some(&self.y),
                "z" => Some(&self.z),
                _ => None,
            }
        }

        fn field_mut(&mut self, name: &str) -> Option<&mut dyn Reflect> {
            match name {
                "x" => Some(&mut self.0.x),
                "y" => Some(&mut self.0.y),
                "z" => Some(&mut self.0.z),
                _ => None,
            }
        }

        fn field_at(&self, index: usize) -> Option<&dyn Reflect> {
            match index {
                0 => Some(&self.x),
                1 => Some(&self.y),
                2 => Some(&self.z),
                _ => None,
            }
        }

        fn field_at_mut(&mut self, index: usize) -> Option<&mut dyn Reflect> {
            match index {
                0 => Some(&mut self.0.x),
                1 => Some(&mut self.0.y),
                2 => Some(&mut self.0.z),
                _ => None,
            }
        }

        fn name_at(&self, index: usize) -> Option<&str> {
            match index {
                0 => Some("x"),
                1 => Some("y"),
                2 => Some("z"),
                _ => None,
            }
        }

        fn index_of(&self, name: &str) -> Option<usize> {
            match name {
                "x" => Some(0),
                "y" => Some(1),
                "z" => Some(2),
                _ => None,
            }
        }

        fn field_len(&self) -> usize {
            3
        }

        fn iter_fields(&self) -> StructFieldIter<'_> {
            StructFieldIter::new(self)
        }

        fn unpack(self: Box<Self>) -> Vec<(Cow<'static, str>, Box<dyn Reflect>)> {
            vec![
                (Cow::Borrowed("x"), Box::new(self.x) as Box<dyn Reflect>),
                (Cow::Borrowed("y"), Box::new(self.y) as Box<dyn Reflect>),
                (Cow::Borrowed("z"), Box::new(self.z) as Box<dyn Reflect>),
            ]
        }
    }
}

impl Dir3A {
    /// A unit vector pointing along the positive X axis.
    pub const X: Self = Self(Vec3A::X);
    /// A unit vector pointing along the positive Y axis.
    pub const Y: Self = Self(Vec3A::Y);
    /// A unit vector pointing along the positive Z axis.
    pub const Z: Self = Self(Vec3A::Z);
    /// A unit vector pointing along the negative X axis.
    pub const NEG_X: Self = Self(Vec3A::NEG_X);
    /// A unit vector pointing along the negative Y axis.
    pub const NEG_Y: Self = Self(Vec3A::NEG_Y);
    /// A unit vector pointing along the negative Z axis.
    pub const NEG_Z: Self = Self(Vec3A::NEG_Z);
    /// The directional axes.
    pub const AXES: [Self; 3] = [Self::X, Self::Y, Self::Z];

    /// Create a direction from a finite, nonzero [`Vec3A`], normalizing it.
    ///
    /// Returns [`Err(InvalidDirectionError)`](InvalidDirectionError) if the length
    /// of the given vector is zero (or very close to zero), infinite, or `NaN`.
    pub fn new(value: Vec3A) -> Result<Self, InvalidDirectionError> {
        Self::new_and_length(value).map(|(dir, _)| dir)
    }

    /// Create a [`Dir3A`] from a [`Vec3A`] that is already normalized.
    ///
    /// # Warning
    ///
    /// `value` must be normalized, i.e its length must be `1.0`.
    pub fn new_unchecked(value: Vec3A) -> Self {
        #[cfg(debug_assertions)]
        super::assert_is_normalized(
            "The vector given to `Dir3A::new_unchecked` is not normalized.",
            value.length_squared(),
        );

        Self(value)
    }

    /// Create a direction from a finite, nonzero [`Vec3A`], normalizing it and
    /// also returning its original length.
    ///
    /// Returns [`Err(InvalidDirectionError)`](InvalidDirectionError) if the length
    /// of the given vector is zero (or very close to zero), infinite, or `NaN`.
    pub fn new_and_length(value: Vec3A) -> Result<(Self, f32), InvalidDirectionError> {
        let length = value.length();
        let direction = (length.is_finite() && length > 0.0).then_some(value / length);

        direction
            .map(|dir| (Self(dir), length))
            .ok_or(InvalidDirectionError::from_length(length))
    }

    /// Create a direction from its `x`, `y`, and `z` components.
    ///
    /// Returns [`Err(InvalidDirectionError)`](InvalidDirectionError) if the length
    /// of the vector formed by the components is zero (or very close to zero), infinite, or `NaN`.
    pub fn from_xyz(x: f32, y: f32, z: f32) -> Result<Self, InvalidDirectionError> {
        Self::new(Vec3A::new(x, y, z))
    }

    /// Create a direction from its `x`, `y`, and `z` components, assuming the resulting vector is normalized.
    ///
    /// # Warning
    ///
    /// The vector produced from `x`, `y`, and `z` must be normalized, i.e its length must be `1.0`.
    pub fn from_xyz_unchecked(x: f32, y: f32, z: f32) -> Self {
        Self::new_unchecked(Vec3A::new(x, y, z))
    }

    /// Returns the inner [`Vec3A`]
    #[inline]
    pub const fn as_vec3a(&self) -> Vec3A {
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
    /// # use zlim_math::Dir3A;
    /// # use approx::{assert_relative_eq, RelativeEq};
    /// #
    /// let dir1 = Dir3A::X;
    /// let dir2 = Dir3A::Y;
    ///
    /// let result1 = dir1.slerp(dir2, 1.0 / 3.0);
    /// #[cfg(feature = "approx")]
    /// assert_relative_eq!(
    ///     result1,
    ///     Dir3A::from_xyz(0.75_f32.sqrt(), 0.5, 0.0).unwrap(),
    ///     epsilon = 0.000001
    /// );
    ///
    /// let result2 = dir1.slerp(dir2, 0.5);
    /// #[cfg(feature = "approx")]
    /// assert_relative_eq!(result2, Dir3A::from_xyz(0.5_f32.sqrt(), 0.5_f32.sqrt(), 0.0).unwrap());
    /// ```
    #[inline]
    pub fn slerp(self, rhs: Self, s: f32) -> Self {
        let quat = Quat::IDENTITY.slerp(
            Quat::from_rotation_arc(Vec3::from(self.0), Vec3::from(rhs.0)),
            s,
        );
        Dir3A(quat.mul_vec3a(self.0))
    }

    /// Returns `self` after an approximate normalization, assuming the value is already nearly normalized.
    /// Useful for preventing numerical error accumulation.
    ///
    /// See [`Dir3::fast_renormalize`] for an example of when such error accumulation might occur.
    #[inline]
    pub fn fast_renormalize(self) -> Self {
        let length_squared = self.0.length_squared();
        // Based on a Taylor approximation of the inverse square root, see [`Dir3::fast_renormalize`] for more details.
        Self(self * (0.5 * (3.0 - length_squared)))
    }
}

impl Default for Dir3A {
    fn default() -> Self {
        Self::X
    }
}

impl From<Dir3> for Dir3A {
    fn from(value: Dir3) -> Self {
        Self(value.0.into())
    }
}

impl From<Dir3A> for Dir3 {
    fn from(value: Dir3A) -> Self {
        Self(value.0.into())
    }
}

impl TryFrom<Vec3A> for Dir3A {
    type Error = InvalidDirectionError;

    fn try_from(value: Vec3A) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Dir3A> for Vec3A {
    fn from(value: Dir3A) -> Self {
        value.0
    }
}

impl core::ops::Deref for Dir3A {
    type Target = Vec3A;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::Neg for Dir3A {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

impl core::ops::Mul<f32> for Dir3A {
    type Output = Vec3A;
    fn mul(self, rhs: f32) -> Self::Output {
        self.0 * rhs
    }
}

impl core::ops::Mul<Dir3A> for f32 {
    type Output = Vec3A;
    fn mul(self, rhs: Dir3A) -> Self::Output {
        self * rhs.0
    }
}

impl core::ops::Mul<Dir3A> for Quat {
    type Output = Dir3A;

    /// Rotates the [`Dir3A`] using a [`Quat`].
    fn mul(self, direction: Dir3A) -> Self::Output {
        let rotated = self * *direction;

        #[cfg(debug_assertions)]
        super::assert_is_normalized(
            "`Dir3A` is denormalized after rotation.",
            rotated.length_squared(),
        );

        Dir3A(rotated)
    }
}

impl Display for Dir3A {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

#[cfg(any(feature = "approx", test))]
impl approx::AbsDiffEq for Dir3A {
    type Epsilon = f32;
    fn default_epsilon() -> f32 {
        f32::EPSILON
    }
    fn abs_diff_eq(&self, other: &Self, epsilon: f32) -> bool {
        self.as_ref().abs_diff_eq(other.as_ref(), epsilon)
    }
}

#[cfg(any(feature = "approx", test))]
impl approx::RelativeEq for Dir3A {
    fn default_max_relative() -> f32 {
        f32::EPSILON
    }
    fn relative_eq(&self, other: &Self, epsilon: f32, max_relative: f32) -> bool {
        self.as_ref()
            .relative_eq(other.as_ref(), epsilon, max_relative)
    }
}

#[cfg(any(feature = "approx", test))]
impl approx::UlpsEq for Dir3A {
    fn default_max_ulps() -> u32 {
        4
    }
    fn ulps_eq(&self, other: &Self, epsilon: f32, max_ulps: u32) -> bool {
        self.as_ref().ulps_eq(other.as_ref(), epsilon, max_ulps)
    }
}

// ---------------------------------------------------------------------
// Dir4

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops;
    use approx::assert_relative_eq;
    use glam::EulerRot;
    #[test]
    fn dir3a_creation() {
        assert_eq!(Dir3A::new(Vec3A::X * 12.5), Ok(Dir3A::X));
        assert_eq!(
            Dir3A::new(Vec3A::new(0.0, 0.0, 0.0)),
            Err(InvalidDirectionError::Zero)
        );
        assert_eq!(
            Dir3A::new(Vec3A::new(f32::INFINITY, 0.0, 0.0)),
            Err(InvalidDirectionError::Infinite)
        );
        assert_eq!(
            Dir3A::new(Vec3A::new(f32::NEG_INFINITY, 0.0, 0.0)),
            Err(InvalidDirectionError::Infinite)
        );
        assert_eq!(
            Dir3A::new(Vec3A::new(f32::NAN, 0.0, 0.0)),
            Err(InvalidDirectionError::NaN)
        );
        assert_eq!(Dir3A::new_and_length(Vec3A::X * 6.5), Ok((Dir3A::X, 6.5)));

        // Test rotation
        assert!(
            (Quat::from_rotation_z(core::f32::consts::FRAC_PI_2) * Dir3A::X)
                .abs_diff_eq(Vec3A::Y, 10e-6)
        );
    }

    /// The interpolation follows the arc rather than a straight line between the two vectors, so a
    /// third of the way from `X` towards `Z` lands 30° from `X`, and the last sample walks the arc
    /// backwards from `Z` to `Y`.
    #[test]
    fn dir3a_slerp() {
        assert_relative_eq!(
            Dir3A::X.slerp(Dir3A::Y, 0.5),
            Dir3A::from_xyz(ops::sqrt(0.5f32), ops::sqrt(0.5f32), 0.0).unwrap()
        );
        assert_relative_eq!(Dir3A::Y.slerp(Dir3A::Z, 0.0), Dir3A::Y);
        assert_relative_eq!(Dir3A::Z.slerp(Dir3A::X, 1.0), Dir3A::X, epsilon = 0.000001);
        assert_relative_eq!(
            Dir3A::X.slerp(Dir3A::Z, 1.0 / 3.0),
            Dir3A::from_xyz(ops::sqrt(0.75f32), 0.0, 0.5).unwrap(),
            epsilon = 0.000001
        );
        assert_relative_eq!(
            Dir3A::Z.slerp(Dir3A::Y, 2.0 / 3.0),
            Dir3A::from_xyz(0.0, ops::sqrt(0.75f32), 0.5).unwrap()
        );
    }

    #[test]
    fn dir3a_renorm() {
        // Evil denormalized quaternion
        let rot3 = Quat::from_euler(EulerRot::XYZ, 1.0, 2.0, 3.0) * (1.0 + 1e-5);
        let mut dir_a = Dir3A::X;
        let mut dir_b = Dir3A::X;

        // We test that renormalizing an already normalized dir doesn't do anything
        assert_relative_eq!(dir_b, dir_b.fast_renormalize(), epsilon = 0.000001);

        for _ in 0..50 {
            dir_a = rot3 * dir_a;
            dir_b = rot3 * dir_b;
            dir_b = dir_b.fast_renormalize();
        }

        // `dir_a` should've gotten denormalized, meanwhile `dir_b` should stay normalized.
        assert!(
            !dir_a.is_normalized(),
            "Denormalization doesn't work, test is faulty"
        );
        assert!(dir_b.is_normalized(), "Renormalisation did not work.");
    }
}
