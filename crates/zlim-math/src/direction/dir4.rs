use core::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};
use zlim_reflect::derive::TypePath;

use super::InvalidDirectionError;
use crate::Vec4;

// -----------------------------------------------------------------------------
// Dir4

/// A normalized vector pointing in a direction in 4D space
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[derive(TypePath)]
#[serde(into = "adapter::Dir4", try_from = "adapter::Dir4")]
#[type_path = "zlim_math::Dir4"]
#[repr(transparent)]
#[doc(alias = "Direction4d")]
pub struct Dir4(Vec4);

// -----------------------------------------------------------------------------
// serialize

mod adapter {
    use serde::{Deserialize, Serialize};

    use crate::InvalidDirectionError;

    #[derive(Serialize, Deserialize)]
    pub(super) struct Dir4 {
        x: f32,
        y: f32,
        z: f32,
        w: f32,
    }

    impl TryFrom<Dir4> for super::Dir4 {
        type Error = InvalidDirectionError;
        #[inline]
        fn try_from(value: Dir4) -> Result<Self, Self::Error> {
            super::Dir4::from_xyzw(value.x, value.y, value.z, value.w)
        }
    }

    impl From<super::Dir4> for Dir4 {
        #[inline]
        fn from(value: super::Dir4) -> Self {
            Self {
                x: value.x,
                y: value.y,
                z: value.z,
                w: value.w,
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

    use super::Dir4;

    impl_reflect! {
        #[reflect(Struct = false)]
        #[reflect(Default, Debug, Clone, Serialize, Deserialize)]
        #[reflect(reflect_apply = reflect_apply, from_reflect = from_reflect)]
        pub struct Dir4 { x: f32, y: f32, z: f32, w: f32 }
    }

    fn from_reflect(mut value: Box<dyn Reflect>) -> Result<Box<Dir4>, Box<dyn Reflect>> {
        match value.downcast::<Dir4>() {
            Ok(v) => return Ok(v),
            Err(e) => value = e,
        }
        let Ok(s) = value.reflect_ref().as_struct() else {
            return Err(value);
        };
        if s.field_len() != 4 {
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
        let Some(w) = s.field("w") else {
            return Err(value);
        };
        let Some(&w) = w.downcast_ref::<f32>() else {
            return Err(value);
        };
        Ok(Box::new(Dir4::from_xyzw(x, y, z, w).unwrap_or(Dir4::X)))
    }

    fn reflect_apply(this: &mut Dir4, other: &dyn Reflect) -> Result<(), ApplyError> {
        zlim_reflect::impls::struct_apply(this, other)?;
        *this = Dir4::new(this.0).unwrap_or(Dir4::X);
        Ok(())
    }

    impl Struct for Dir4 {
        fn field(&self, name: &str) -> Option<&dyn Reflect> {
            match name {
                "x" => Some(&self.x),
                "y" => Some(&self.y),
                "z" => Some(&self.z),
                "w" => Some(&self.w),
                _ => None,
            }
        }

        fn field_mut(&mut self, name: &str) -> Option<&mut dyn Reflect> {
            match name {
                "x" => Some(&mut self.0.x),
                "y" => Some(&mut self.0.y),
                "z" => Some(&mut self.0.z),
                "w" => Some(&mut self.0.w),
                _ => None,
            }
        }

        fn field_at(&self, index: usize) -> Option<&dyn Reflect> {
            match index {
                0 => Some(&self.x),
                1 => Some(&self.y),
                2 => Some(&self.z),
                3 => Some(&self.w),
                _ => None,
            }
        }

        fn field_at_mut(&mut self, index: usize) -> Option<&mut dyn Reflect> {
            match index {
                0 => Some(&mut self.0.x),
                1 => Some(&mut self.0.y),
                2 => Some(&mut self.0.z),
                3 => Some(&mut self.0.w),
                _ => None,
            }
        }

        fn name_at(&self, index: usize) -> Option<&str> {
            match index {
                0 => Some("x"),
                1 => Some("y"),
                2 => Some("z"),
                3 => Some("w"),
                _ => None,
            }
        }

        fn index_of(&self, name: &str) -> Option<usize> {
            match name {
                "x" => Some(0),
                "y" => Some(1),
                "z" => Some(2),
                "w" => Some(3),
                _ => None,
            }
        }

        fn field_len(&self) -> usize {
            4
        }

        fn iter_fields(&self) -> StructFieldIter<'_> {
            StructFieldIter::new(self)
        }

        fn unpack(self: Box<Self>) -> Vec<(Cow<'static, str>, Box<dyn Reflect>)> {
            vec![
                (Cow::Borrowed("x"), Box::new(self.x) as Box<dyn Reflect>),
                (Cow::Borrowed("y"), Box::new(self.y) as Box<dyn Reflect>),
                (Cow::Borrowed("z"), Box::new(self.z) as Box<dyn Reflect>),
                (Cow::Borrowed("w"), Box::new(self.w) as Box<dyn Reflect>),
            ]
        }
    }
}

impl Dir4 {
    /// A unit vector pointing along the positive X axis
    pub const X: Self = Self(Vec4::X);
    /// A unit vector pointing along the positive Y axis.
    pub const Y: Self = Self(Vec4::Y);
    /// A unit vector pointing along the positive Z axis.
    pub const Z: Self = Self(Vec4::Z);
    /// A unit vector pointing along the positive W axis.
    pub const W: Self = Self(Vec4::W);
    /// A unit vector pointing along the negative X axis.
    pub const NEG_X: Self = Self(Vec4::NEG_X);
    /// A unit vector pointing along the negative Y axis.
    pub const NEG_Y: Self = Self(Vec4::NEG_Y);
    /// A unit vector pointing along the negative Z axis.
    pub const NEG_Z: Self = Self(Vec4::NEG_Z);
    /// A unit vector pointing along the negative W axis.
    pub const NEG_W: Self = Self(Vec4::NEG_W);
    /// The directional axes.
    pub const AXES: [Self; 4] = [Self::X, Self::Y, Self::Z, Self::W];

    /// Create a direction from a finite, nonzero [`Vec4`], normalizing it.
    ///
    /// Returns [`Err(InvalidDirectionError)`](InvalidDirectionError) if the length
    /// of the given vector is zero (or very close to zero), infinite, or `NaN`.
    pub fn new(value: Vec4) -> Result<Self, InvalidDirectionError> {
        Self::new_and_length(value).map(|(dir, _)| dir)
    }

    /// Create a [`Dir4`] from a [`Vec4`] that is already normalized.
    ///
    /// # Warning
    ///
    /// `value` must be normalized, i.e its length must be `1.0`.
    pub fn new_unchecked(value: Vec4) -> Self {
        #[cfg(debug_assertions)]
        super::assert_is_normalized(
            "The vector given to `Dir4::new_unchecked` is not normalized.",
            value.length_squared(),
        );
        Self(value)
    }

    /// Create a direction from a finite, nonzero [`Vec4`], normalizing it and
    /// also returning its original length.
    ///
    /// Returns [`Err(InvalidDirectionError)`](InvalidDirectionError) if the length
    /// of the given vector is zero (or very close to zero), infinite, or `NaN`.
    pub fn new_and_length(value: Vec4) -> Result<(Self, f32), InvalidDirectionError> {
        let length = value.length();
        let direction = (length.is_finite() && length > 0.0).then_some(value / length);

        direction
            .map(|dir| (Self(dir), length))
            .ok_or(InvalidDirectionError::from_length(length))
    }

    /// Create a direction from its `x`, `y`, `z`, and `w` components.
    ///
    /// Returns [`Err(InvalidDirectionError)`](InvalidDirectionError) if the length
    /// of the vector formed by the components is zero (or very close to zero), infinite, or `NaN`.
    pub fn from_xyzw(x: f32, y: f32, z: f32, w: f32) -> Result<Self, InvalidDirectionError> {
        Self::new(Vec4::new(x, y, z, w))
    }

    /// Create a direction from its `x`, `y`, `z`, and `w` components, assuming the resulting vector is normalized.
    ///
    /// # Warning
    ///
    /// The vector produced from `x`, `y`, `z`, and `w` must be normalized, i.e its length must be `1.0`.
    pub fn from_xyzw_unchecked(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self::new_unchecked(Vec4::new(x, y, z, w))
    }

    /// Returns the inner [`Vec4`]
    #[inline]
    pub const fn as_vec4(&self) -> Vec4 {
        self.0
    }

    /// Returns `self` after an approximate normalization, assuming the value is already nearly normalized.
    /// Useful for preventing numerical error accumulation.
    #[inline]
    pub fn fast_renormalize(self) -> Self {
        // We numerically approximate the inverse square root by a Taylor series around 1
        // As we expect the error (x := length_squared - 1) to be small
        // inverse_sqrt(length_squared) = (1 + x)^(-1/2) = 1 - 1/2 x + O(x²)
        // inverse_sqrt(length_squared) ≈ 1 - 1/2 (length_squared - 1) = 1/2 (3 - length_squared)

        // Iterative calls to this method quickly converge to a normalized value,
        // so long as the denormalization is not large ~ O(1/10).
        // One iteration can be described as:
        // l_sq <- l_sq * (1 - 1/2 (l_sq - 1))²;
        // Rewriting in terms of the error x:
        // 1 + x <- (1 + x) * (1 - 1/2 x)²
        // 1 + x <- (1 + x) * (1 - x + 1/4 x²)
        // 1 + x <- 1 - x + 1/4 x² + x - x² + 1/4 x³
        // x <- -1/4 x² (3 - x)
        // If the error is small, say in a range of (-1/2, 1/2), then:
        // |-1/4 x² (3 - x)| <= (3/4 + 1/4 * |x|) * x² <= (3/4 + 1/4 * 1/2) * x² < x² < 1/2 x
        // Therefore the sequence of iterates converges to 0 error as a second order method.

        let length_squared = self.0.length_squared();
        Self(self * (0.5 * (3.0 - length_squared)))
    }
}

impl Default for Dir4 {
    fn default() -> Self {
        Self::X
    }
}

impl TryFrom<Vec4> for Dir4 {
    type Error = InvalidDirectionError;

    fn try_from(value: Vec4) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl core::ops::Deref for Dir4 {
    type Target = Vec4;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::Neg for Dir4 {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

impl core::ops::Mul<f32> for Dir4 {
    type Output = Vec4;
    fn mul(self, rhs: f32) -> Self::Output {
        self.0 * rhs
    }
}

impl core::ops::Mul<Dir4> for f32 {
    type Output = Vec4;
    fn mul(self, rhs: Dir4) -> Self::Output {
        self * rhs.0
    }
}

impl Display for Dir4 {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl From<Dir4> for Vec4 {
    fn from(value: Dir4) -> Self {
        value.0
    }
}

#[cfg(any(feature = "approx", test))]
impl approx::AbsDiffEq for Dir4 {
    type Epsilon = f32;
    fn default_epsilon() -> f32 {
        f32::EPSILON
    }

    fn abs_diff_eq(&self, other: &Self, epsilon: f32) -> bool {
        self.as_ref().abs_diff_eq(other.as_ref(), epsilon)
    }
}

#[cfg(any(feature = "approx", test))]
impl approx::RelativeEq for Dir4 {
    fn default_max_relative() -> f32 {
        f32::EPSILON
    }

    fn relative_eq(&self, other: &Self, epsilon: f32, max_relative: f32) -> bool {
        self.as_ref()
            .relative_eq(other.as_ref(), epsilon, max_relative)
    }
}

#[cfg(any(feature = "approx", test))]
impl approx::UlpsEq for Dir4 {
    fn default_max_ulps() -> u32 {
        4
    }

    fn ulps_eq(&self, other: &Self, epsilon: f32, max_ulps: u32) -> bool {
        self.as_ref().ulps_eq(other.as_ref(), epsilon, max_ulps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use glam::{EulerRot, Mat4, Quat};

    #[test]
    fn dir4_creation() {
        assert_eq!(Dir4::new(Vec4::X * 12.5), Ok(Dir4::X));
        assert_eq!(
            Dir4::new(Vec4::new(0.0, 0.0, 0.0, 0.0)),
            Err(InvalidDirectionError::Zero)
        );
        assert_eq!(
            Dir4::new(Vec4::new(f32::INFINITY, 0.0, 0.0, 0.0)),
            Err(InvalidDirectionError::Infinite)
        );
        assert_eq!(
            Dir4::new(Vec4::new(f32::NEG_INFINITY, 0.0, 0.0, 0.0)),
            Err(InvalidDirectionError::Infinite)
        );
        assert_eq!(
            Dir4::new(Vec4::new(f32::NAN, 0.0, 0.0, 0.0)),
            Err(InvalidDirectionError::NaN)
        );
        assert_eq!(Dir4::new_and_length(Vec4::X * 6.5), Ok((Dir4::X, 6.5)));
    }

    #[test]
    fn dir4_renorm() {
        // Evil denormalized matrix
        let mat4 = Mat4::from_quat(Quat::from_euler(EulerRot::XYZ, 1.0, 2.0, 3.0)) * (1.0 + 1e-5);

        let mut dir_a = Dir4::from_xyzw(1., 1., 0., 0.).unwrap();
        let mut dir_b = Dir4::from_xyzw(1., 1., 0., 0.).unwrap();

        // We test that renormalizing an already normalized dir doesn't do anything
        assert_relative_eq!(dir_b, dir_b.fast_renormalize(), epsilon = 0.000001);
        for _ in 0..50 {
            dir_a = Dir4(mat4 * *dir_a);
            dir_b = Dir4(mat4 * *dir_b);
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
