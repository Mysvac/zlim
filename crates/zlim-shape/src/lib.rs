#![doc = include_str!("../README.md")]

// ---------------------------------------------------------------------
// Marker traits

/// A marker trait for 2D primitives
pub trait Primitive2d {}

/// A marker trait for 3D primitives
pub trait Primitive3d {}

impl Primitive2d for zlim_math::Dir2 {}
impl Primitive3d for zlim_math::Dir3 {}
impl Primitive3d for zlim_math::Dir3A {}

// ---------------------------------------------------------------------
// WindingOrder

/// The winding order for a set of points
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(alias = "Orientation")]
pub enum WindingOrder {
    /// A clockwise winding order
    Clockwise,
    /// A counterclockwise winding order
    #[doc(alias = "AntiClockwise")]
    CounterClockwise,
    /// An invalid winding order indicating that it could not be computed reliably.
    /// This often happens in *degenerate cases* where the points lie on the same line
    #[doc(alias("Degenerate", "Collinear"))]
    Invalid,
}

// ---------------------------------------------------------------------
// Measurements

mod measure;
pub use measure::{Measured2d, Measured3d};

// ---------------------------------------------------------------------
// Rays

mod ray;
pub use ray::{Ray2d, Ray3d};

// ---------------------------------------------------------------------
// 2D primitives

mod dim2;
pub use dim2::*;

// ---------------------------------------------------------------------
// 3D primitives

mod dim3;
pub use dim3::*;

// ---------------------------------------------------------------------
// HalfSpace

mod half_space;
pub use half_space::HalfSpace;

// ---------------------------------------------------------------------
// Inset

mod inset;
pub use inset::Inset;

// ---------------------------------------------------------------------
// Polygon

// Internal helpers (`is_polygon_simple`), consumed by `dim2`.
mod polygon;

// ---------------------------------------------------------------------
// ViewFrustum

mod view_frustum;
pub use view_frustum::ViewFrustum;

// ---------------------------------------------------------------------
// Bounding volumes

pub mod bounding;

// ---------------------------------------------------------------------
// Random sampling of shapes

#[cfg(feature = "rand")]
pub mod sampling;

#[cfg(feature = "rand")]
pub use sampling::ShapeSample;

// ---------------------------------------------------------------------
// Prelude

/// The shape prelude.
///
/// This includes all primitive shape types in this crate, re-exported for
/// your convenience.
pub mod prelude {
    // just re-export everything, it's just shape definitions anyways
    #[doc(hidden)]
    pub use crate::{Measured2d, Measured3d, Primitive2d, Primitive3d, WindingOrder};

    #[doc(hidden)]
    pub use crate::bounding::*;

    #[doc(hidden)]
    pub use crate::dim2::*;

    #[doc(hidden)]
    pub use crate::dim3::*;

    #[doc(hidden)]
    pub use crate::half_space::HalfSpace;

    #[doc(hidden)]
    pub use crate::inset::Inset;

    #[doc(hidden)]
    pub use crate::ray::{Ray2d, Ray3d};

    #[doc(hidden)]
    pub use crate::view_frustum::ViewFrustum;

    #[doc(hidden)]
    #[cfg(feature = "rand")]
    pub use crate::sampling::ShapeSample;
}
