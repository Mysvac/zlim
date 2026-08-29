#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, expect(internal_features, reason = "needed for fake_variadic"))]
#![cfg_attr(docsrs, feature(doc_cfg, rustdoc_internals))]
#![forbid(unsafe_code)]

// ---------------------------------------------------------------------
// glam

pub use glam::camera::rh::proj::directx as proj;
pub use glam::dcamera::rh::proj::directx as dproj;

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

/// A marker trait for 2D primitives
pub trait Primitive2d {}

/// A marker trait for 3D primitives
pub trait Primitive3d {}

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
// ray

mod ray;
pub use ray::{Ray2d, Ray3d};

// ---------------------------------------------------------------------
// measure

mod measure;
pub use measure::{Measured2d, Measured3d};

// ---------------------------------------------------------------------
// primitives

pub mod primitives;

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
// bounding

pub mod bounding;

// ---------------------------------------------------------------------
// curve

pub mod curve;
pub use curve::Curve;

// ---------------------------------------------------------------------
// cubic_splines

pub mod cubic_splines;

// ---------------------------------------------------------------------
// sampling

#[cfg(feature = "rand")]
pub mod sampling;

#[cfg(feature = "rand")]
pub use sampling::{FromRng, ShapeSample};

// ---------------------------------------------------------------------
// common traits

pub mod common_traits;
pub use common_traits::*;

// ---------------------------------------------------------------------
// Affine3Ext

mod affine3;
pub use affine3::Affine3Ext;

// ---------------------------------------------------------------------
// Modules

/// The math prelude.
pub mod prelude {
    pub use glam::bool::{BVec2, bvec2};
    pub use glam::bool::{BVec3, bvec3};
    pub use glam::bool::{BVec3A, bvec3a};
    pub use glam::bool::{BVec4, bvec4};
    pub use glam::bool::{BVec4A, bvec4a};
    pub use glam::f32::{Mat2, mat2};
    pub use glam::f32::{Mat3, mat3};
    pub use glam::f32::{Mat3A, mat3a};
    pub use glam::f32::{Mat4, mat4};
    pub use glam::f32::{Quat, quat};
    pub use glam::f32::{Vec2, vec2};
    pub use glam::f32::{Vec3, vec3};
    pub use glam::f32::{Vec3A, vec3a};
    pub use glam::f32::{Vec4, vec4};
    pub use glam::i32::{IVec2, ivec2};
    pub use glam::i32::{IVec3, ivec3};
    pub use glam::i32::{IVec4, ivec4};
    pub use glam::u32::{UVec2, uvec2};
    pub use glam::u32::{UVec3, uvec3};
    pub use glam::u32::{UVec4, uvec4};

    pub use glam::swizzles::{Vec2Swizzles, Vec3Swizzles, Vec4Swizzles};
    pub use glam::{EulerRot, FloatExt};

    pub use crate::FloatPow;
    pub use crate::ops;

    pub use crate::Rot2;
    pub use crate::common_traits::StableInterpolate;
    pub use crate::direction::{Dir2, Dir3, Dir3A};
    pub use crate::isometry::{Isometry2d, Isometry3d};
    pub use crate::measure::{Measured2d, Measured3d};
    pub use crate::ray::{Ray2d, Ray3d};
    pub use crate::rects::{IRect, Rect, URect};

    pub use crate::cubic_splines::*;
    pub use crate::curve::*;

    pub use crate::primitives::*;
    pub use crate::{Primitive2d, Primitive3d};

    #[cfg(feature = "rand")]
    pub use crate::sampling::{FromRng, ShapeSample};
}
