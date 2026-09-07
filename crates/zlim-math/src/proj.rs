//! Projection matrix constructors.
//!
//! DirectX and WebGPU NDC convention: Z range `[0, 1]`, Y-up.
//!
//! Expects a right-handed Y-up view space input.
//!
//! Includes standard, infinite-far, and reverse-depth variants.

use glam::{Mat4, Vec4};

pub use glam::camera::rh::proj::directx::*;

/// Creates a perspective projection matrix with reversed depth for use with DirectX and WebGPU.
///
/// Maps `near` to depth `1` and `far` to depth `0`.
///
/// Reversed Z improves depth precision when used with a floating-point depth buffer.
///
/// Expects a right-handed Y-up view space input.
/// Outputs NDC with Z in `[0, 1]` and Y-up.
///
/// # Panics
///
/// May panic if `near` or `far` are less than or equal to zero in debug mode.
#[inline]
#[must_use]
pub fn perspective_reverse(vertical_fov: f32, aspect_ratio: f32, near: f32, far: f32) -> Mat4 {
    #[cfg(any(debug_assertions, feature = "debug"))]
    assert!(near > 0.0 && far > 0.0);

    let (sin_fov, cos_fov) = crate::ops::sin_cos(0.5 * vertical_fov);
    let h = cos_fov / sin_fov;
    let xx = h / aspect_ratio;
    let yy = h;

    let z_range_inv = 1.0 / (far - near);
    let zz = near * z_range_inv;
    let tz = near * far * z_range_inv;

    Mat4::from_cols(
        Vec4::new(xx, 0.0, 0.0, 0.0),
        Vec4::new(0.0, yy, 0.0, 0.0),
        Vec4::new(0.0, 0.0, zz, -1.0),
        Vec4::new(0.0, 0.0, tz, 0.0),
    )
}
