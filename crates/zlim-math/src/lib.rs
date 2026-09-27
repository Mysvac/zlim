#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, expect(internal_features, reason = "needed for fake_variadic"))]
#![cfg_attr(docsrs, feature(doc_cfg, rustdoc_internals))]
#![forbid(unsafe_code)]

// ---------------------------------------------------------------------
// glam

pub use glam::EulerRot;
pub use glam::FloatExt;
pub use glam::bool::*;
pub use glam::f32::*;
pub use glam::f64::*;
pub use glam::i8::*;
pub use glam::i16::*;
pub use glam::i32::*;
pub use glam::i64::*;
pub use glam::swizzles::*;
pub use glam::u8::*;
pub use glam::u16::*;
pub use glam::u32::*;
pub use glam::u64::*;

// ---------------------------------------------------------------------
// Basic

pub mod ops;
pub use ops::FloatPow;

// ---------------------------------------------------------------------
// 2D Rotation

mod rotation2d;
pub use rotation2d::Rot2;

// ---------------------------------------------------------------------
// direction

mod direction;
pub use direction::InvalidDirectionError;
pub use direction::{Dir2, Dir3, Dir3A, Dir4};

// ---------------------------------------------------------------------
// isometry

mod isometry;
pub use isometry::{Isometry2d, Isometry3d};

// ---------------------------------------------------------------------
// reflection_matrix

mod matrix;
pub use matrix::reflection_matrix;

// ---------------------------------------------------------------------
// float_ord

mod float_ord;
pub use float_ord::FloatOrd;

// ---------------------------------------------------------------------
// aspect_ratio

mod aspect_ratio;
pub use aspect_ratio::{AspectRatio, AspectRatioError};

// ---------------------------------------------------------------------
// compass

mod compass;
pub use compass::{CompassOctant, CompassQuadrant};

// ---------------------------------------------------------------------
// rects

mod rects;
pub use rects::{IRect, Rect, URect};

// ---------------------------------------------------------------------
// distribution

#[cfg(feature = "rand")]
mod distribution;

// ---------------------------------------------------------------------
// common traits

pub mod common_traits;
pub use common_traits::*;

// ---------------------------------------------------------------------
// Affine3Ext

mod affine3;
pub use affine3::Affine3Ext;

// ---------------------------------------------------------------------
// proj

pub mod dproj;
pub mod proj;

// ---------------------------------------------------------------------
// Modules

/// The math prelude.
pub mod prelude {
    #[doc(no_inline)]
    pub use crate::{BVec2, bvec2};
    #[doc(no_inline)]
    pub use crate::{BVec3, bvec3};
    #[doc(no_inline)]
    pub use crate::{BVec3A, bvec3a};
    #[doc(no_inline)]
    pub use crate::{BVec4, bvec4};
    #[doc(no_inline)]
    pub use crate::{BVec4A, bvec4a};
    #[doc(no_inline)]
    pub use crate::{IVec2, ivec2};
    #[doc(no_inline)]
    pub use crate::{IVec3, ivec3};
    #[doc(no_inline)]
    pub use crate::{IVec4, ivec4};
    #[doc(no_inline)]
    pub use crate::{Mat2, mat2};
    #[doc(no_inline)]
    pub use crate::{Mat3, mat3};
    #[doc(no_inline)]
    pub use crate::{Mat3A, mat3a};
    #[doc(no_inline)]
    pub use crate::{Mat4, mat4};
    #[doc(no_inline)]
    pub use crate::{Quat, quat};
    #[doc(no_inline)]
    pub use crate::{UVec2, uvec2};
    #[doc(no_inline)]
    pub use crate::{UVec3, uvec3};
    #[doc(no_inline)]
    pub use crate::{UVec4, uvec4};
    #[doc(no_inline)]
    pub use crate::{Vec2, vec2};
    #[doc(no_inline)]
    pub use crate::{Vec3, vec3};
    #[doc(no_inline)]
    pub use crate::{Vec3A, vec3a};
    #[doc(no_inline)]
    pub use crate::{Vec4, vec4};

    #[doc(no_inline)]
    pub use crate::{Vec2Swizzles, Vec3Swizzles, Vec4Swizzles};

    #[doc(no_inline)]
    pub use crate::{EulerRot, FloatExt};

    #[doc(no_inline)]
    pub use crate::FloatPow;

    #[doc(no_inline)]
    pub use crate::ops;

    #[doc(no_inline)]
    pub use crate::Rot2;

    #[doc(no_inline)]
    pub use crate::{Dir2, Dir3, Dir3A};

    #[doc(no_inline)]
    pub use crate::{Isometry2d, Isometry3d};

    #[doc(no_inline)]
    pub use crate::{IRect, Rect, URect};
}
