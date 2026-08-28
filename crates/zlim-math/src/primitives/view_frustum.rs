use serde::{Deserialize, Serialize};
use zlim_reflect::Reflect;

use crate::primitives::HalfSpace;
use crate::{Mat4, Vec3};

/// A region of 3D space defined by the intersection of 6 [`HalfSpace`]s.
///
/// View Frustums are typically an apex-truncated square pyramid (a pyramid
/// without the top) or a cuboid.
///
/// Assumed clipping region: `-1 < x < 1`, `-1 < y < 1`, `0 < z < 1`.
/// The array indices correspond to:
///
/// - `[0]`: left plane,   `x = -1` (X-axis points right)
/// - `[1]`: right plane,  `x =  1` (X-axis points right)
/// - `[2]`: bottom plane, `y = -1` (Y-axis points up)
/// - `[3]`: top plane,    `y =  1` (Y-axis points up)
/// - `[4]`: near plane,   `z =  0` (Z-axis points inward)
/// - `[5]`: far plane,    `z =  1` (Z-axis points inward)
///
/// If you assume Y-down, then `[2]` is Top and `[3]` is Bottom.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[derive(Reflect, Serialize, Deserialize)]
#[reflect(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ViewFrustum {
    /// The six half-spaces making up the frustum
    pub half_spaces: [HalfSpace; 6],
}

impl ViewFrustum {
    /// The index for the near plane in `half_spaces`
    pub const NEAR_PLANE_IDX: usize = 4;
    /// The index for the far plane in `half_spaces`
    pub const FAR_PLANE_IDX: usize = 5;

    /// Returns a view frustum derived from `clip_from_world`.
    ///
    /// The clip matrix is expected to use the convention documented on
    /// [`ViewFrustum`] (`x`/`y` in `[-1, 1]`, `z` in `[0, 1]`, Y-up); the
    /// produced half-spaces follow the `half_spaces` index order.
    #[inline]
    pub fn from_clip_from_world(clip_from_world: &Mat4) -> Self {
        let row0 = clip_from_world.row(0);
        let row1 = clip_from_world.row(1);
        let row2 = clip_from_world.row(2);
        let row3 = clip_from_world.row(3);
        Self {
            half_spaces: [
                HalfSpace::new(row3 + row0),
                HalfSpace::new(row3 - row0),
                HalfSpace::new(row3 + row1),
                HalfSpace::new(row3 - row1),
                HalfSpace::new(row2),
                HalfSpace::new(row3 - row2),
            ],
        }
    }

    /// Returns a view frustum derived from `clip_from_world`, but with a custom far plane.
    ///
    /// The clip matrix is expected to use the convention documented on
    /// [`ViewFrustum`] (`x`/`y` in `[-1, 1]`, `z` in `[0, 1]`, Y-up); only the
    /// far plane is replaced, constructed from the camera's backward direction.
    #[inline]
    pub fn from_clip_from_world_custom_far(
        clip_from_world: &Mat4,
        view_translation: &Vec3,
        view_backward: &Vec3,
        far: f32,
    ) -> Self {
        let row0 = clip_from_world.row(0);
        let row1 = clip_from_world.row(1);
        let row2 = clip_from_world.row(2);
        let row3 = clip_from_world.row(3);
        let far_center = *view_translation - far * *view_backward;
        let far = view_backward.extend(-view_backward.dot(far_center));
        Self {
            half_spaces: [
                HalfSpace::new(row3 + row0),
                HalfSpace::new(row3 - row0),
                HalfSpace::new(row3 + row1),
                HalfSpace::new(row3 - row1),
                HalfSpace::new(row2),
                HalfSpace::new(far),
            ],
        }
    }

    /// Calculates the corners of this frustum. Returns `None` if the frustum isn't properly defined.
    ///
    /// If `Some`, the corners are returned in the following order:
    /// near top left, near top right, near bottom right, near bottom left,
    /// far top left, far top right, far bottom right, far bottom left.
    /// If the far plane is an inactive half space, the intersection points
    /// that include the far plane will be `Vec3::NAN`.
    #[inline]
    pub fn corners(&self) -> Option<[Vec3; 8]> {
        let [left, right, bottom, top, near, far] = self.half_spaces;
        Some([
            HalfSpace::intersection_point(top, left, near)?,
            HalfSpace::intersection_point(top, right, near)?,
            HalfSpace::intersection_point(bottom, right, near)?,
            HalfSpace::intersection_point(bottom, left, near)?,
            HalfSpace::intersection_point(top, left, far)?,
            HalfSpace::intersection_point(top, right, far)?,
            HalfSpace::intersection_point(bottom, right, far)?,
            HalfSpace::intersection_point(bottom, left, far)?,
        ])
    }
}

#[cfg(test)]
mod view_frustum_tests {
    use core::f32::consts::FRAC_1_SQRT_2;

    use approx::assert_relative_eq;

    use super::ViewFrustum;
    use crate::primitives::HalfSpace;
    use crate::proj;
    use crate::{Vec3, Vec4};

    #[test]
    fn test_from_clip_from_world() {
        let clip_from_world = proj::perspective(60.0_f32.to_radians(), 1.0, 1.0, 10.0);
        let frustum = ViewFrustum::from_clip_from_world(&clip_from_world);

        // Left
        assert_relative_eq!(
            frustum.half_spaces[0].normal_d(),
            Vec4::new(0.8660254, 0., -0.5, 0.),
            epsilon = 2e-5
        );
        // Right
        assert_relative_eq!(
            frustum.half_spaces[1].normal_d(),
            Vec4::new(-0.8660254, 0., -0.5, 0.),
            epsilon = 2e-5
        );
        // Bottom
        assert_relative_eq!(
            frustum.half_spaces[2].normal_d(),
            Vec4::new(0., 0.8660254, -0.5, 0.),
            epsilon = 2e-5
        );
        // Top
        assert_relative_eq!(
            frustum.half_spaces[3].normal_d(),
            Vec4::new(0., -0.8660254, -0.5, 0.),
            epsilon = 2e-5
        );
        // Near
        assert_relative_eq!(
            frustum.half_spaces[4].normal_d(),
            Vec4::new(0., 0., -1., -1.),
            epsilon = 2e-5
        );
        // Far
        assert_relative_eq!(
            frustum.half_spaces[5].normal_d(),
            Vec4::new(0., 0., 1., 10.),
            epsilon = 2e-5
        );
    }

    #[test]
    fn cuboid_frustum_corners() {
        let cuboid_frustum = ViewFrustum {
            // left: x = -5; right: x = 4
            // near: y = 0; far: y = 6
            // top: z = 3; bottom: z = -2
            half_spaces: [
                // left: yz plane at x = -5
                HalfSpace::new(Vec4::new(1., 0., 0., 5.)),
                // right: yz plane at x = 4
                HalfSpace::new(Vec4::new(-1., 0., 0., 4.)),
                // bottom: xy plane at z = -2
                HalfSpace::new(Vec4::new(0., 0., 1., 2.)),
                // top: xy plane at z = 3
                HalfSpace::new(Vec4::new(0., 0., -1., 3.)),
                // near: xz plane at origin (y = 0)
                HalfSpace::new(Vec4::new(0., 1., 0., 0.)),
                // far: xz plane at y = 6
                HalfSpace::new(Vec4::new(0., -1., 0., 6.)),
            ],
        };
        let corners = cuboid_frustum.corners().unwrap();
        // near top left
        assert_relative_eq!(corners[0], Vec3::new(-5., 0., 3.), epsilon = 2e-7);
        // near top right
        assert_relative_eq!(corners[1], Vec3::new(4., 0., 3.), epsilon = 2e-7);
        // near bottom right
        assert_relative_eq!(corners[2], Vec3::new(4., 0., -2.), epsilon = 2e-7);
        // near bottom left
        assert_relative_eq!(corners[3], Vec3::new(-5., 0., -2.), epsilon = 2e-7);
        // far top left
        assert_relative_eq!(corners[4], Vec3::new(-5., 6., 3.), epsilon = 2e-7);
        // far top right
        assert_relative_eq!(corners[5], Vec3::new(4., 6., 3.), epsilon = 2e-7);
        // far bottom right
        assert_relative_eq!(corners[6], Vec3::new(4., 6., -2.), epsilon = 2e-7);
        // far bottom left
        assert_relative_eq!(corners[7], Vec3::new(-5., 6., -2.), epsilon = 2e-7);
    }

    #[test]
    fn pyramid_frustum_corners() {
        // a frustum where the near plane intersects the left right top and bottom planes
        // at a single point
        let pyramid_frustum = ViewFrustum {
            half_spaces: [
                // left
                HalfSpace::new(Vec4::new(FRAC_1_SQRT_2, FRAC_1_SQRT_2, 0., FRAC_1_SQRT_2)),
                // right
                HalfSpace::new(Vec4::new(-FRAC_1_SQRT_2, FRAC_1_SQRT_2, 0., FRAC_1_SQRT_2)),
                // bottom
                HalfSpace::new(Vec4::new(0., FRAC_1_SQRT_2, FRAC_1_SQRT_2, FRAC_1_SQRT_2)),
                // top
                HalfSpace::new(Vec4::new(0., FRAC_1_SQRT_2, -FRAC_1_SQRT_2, FRAC_1_SQRT_2)),
                // near: xz plane at y = -1
                HalfSpace::new(Vec4::new(0., 1., 0., 1.)),
                // far: xz plane at y = 3
                HalfSpace::new(Vec4::new(0., -1., 0., 3.)),
            ],
        };
        let corners = pyramid_frustum.corners().unwrap();
        // near top left
        assert_relative_eq!(corners[0], Vec3::new(0., -1., 0.), epsilon = 2e-7);
        // near top right
        assert_relative_eq!(corners[1], Vec3::new(0., -1., 0.), epsilon = 2e-7);
        // near bottom right
        assert_relative_eq!(corners[2], Vec3::new(0., -1., 0.), epsilon = 2e-7);
        // near bottom left
        assert_relative_eq!(corners[3], Vec3::new(0., -1., 0.), epsilon = 2e-7);
        // far top left
        assert_relative_eq!(corners[4], Vec3::new(-4., 3., 4.), epsilon = 2e-7);
        // far top right
        assert_relative_eq!(corners[5], Vec3::new(4., 3., 4.), epsilon = 2e-7);
        // far bottom right
        assert_relative_eq!(corners[6], Vec3::new(4., 3., -4.), epsilon = 2e-7);
        // far bottom left
        assert_relative_eq!(corners[7], Vec3::new(-4., 3., -4.), epsilon = 2e-7);
    }

    #[test]
    fn frustum_with_some_nan_corners() {
        // frustum with no far plane has NAN far corners
        let no_far = ViewFrustum {
            half_spaces: [
                // left: a yz plane rotated outwards
                HalfSpace::new(Vec4::new(FRAC_1_SQRT_2, FRAC_1_SQRT_2, 0., FRAC_1_SQRT_2)),
                // right: a yz plane rotated outwards
                HalfSpace::new(Vec4::new(-FRAC_1_SQRT_2, FRAC_1_SQRT_2, 0., FRAC_1_SQRT_2)),
                // bottom: xz plane rotated outwards
                HalfSpace::new(Vec4::new(0., FRAC_1_SQRT_2, FRAC_1_SQRT_2, FRAC_1_SQRT_2)),
                // top: an xz plane rotated outwards
                HalfSpace::new(Vec4::new(0., FRAC_1_SQRT_2, -FRAC_1_SQRT_2, FRAC_1_SQRT_2)),
                // near: xz plane at origin (y = 0)
                HalfSpace::new(Vec4::new(0., 1., 0., 0.)),
                // far
                HalfSpace::new(Vec4::new(0., 1., 0., f32::INFINITY)),
            ],
        };
        let corners = no_far.corners().unwrap();
        // near top left
        assert_relative_eq!(corners[0], Vec3::new(-1., 0., 1.), epsilon = 2e-7);
        // near top right
        assert_relative_eq!(corners[1], Vec3::new(1., 0., 1.), epsilon = 2e-7);
        // near bottom right
        assert_relative_eq!(corners[2], Vec3::new(1., 0., -1.), epsilon = 2e-7);
        // near bottom left
        assert_relative_eq!(corners[3], Vec3::new(-1., 0., -1.), epsilon = 2e-7);
        // far top left
        assert!(corners[4].is_nan());
        // far top right
        assert!(corners[5].is_nan());
        // far bottom right
        assert!(corners[6].is_nan());
        // far bottom left
        assert!(corners[7].is_nan());
    }

    #[test]
    fn invalid_frustum_corners() {
        let invalid = ViewFrustum {
            half_spaces: [
                // the left and the bottom half spaces are the same, resulting in no intersection point
                HalfSpace::new(Vec4::new(FRAC_1_SQRT_2, FRAC_1_SQRT_2, 0., FRAC_1_SQRT_2)),
                HalfSpace::new(Vec4::new(-FRAC_1_SQRT_2, FRAC_1_SQRT_2, 0., -FRAC_1_SQRT_2)),
                HalfSpace::new(Vec4::new(FRAC_1_SQRT_2, FRAC_1_SQRT_2, 0., FRAC_1_SQRT_2)),
                HalfSpace::new(Vec4::new(0., FRAC_1_SQRT_2, FRAC_1_SQRT_2, FRAC_1_SQRT_2)),
                HalfSpace::new(Vec4::new(0., 1., 0., 0.)),
                HalfSpace::new(Vec4::new(0., -1., 0., 3.)),
            ],
        };
        assert!(invalid.corners().is_none());
    }
}
