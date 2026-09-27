#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]

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

mod polygon;
pub use polygon::is_polygon_simple;

// ---------------------------------------------------------------------
// ViewFrustum

mod view_frustum;
pub use view_frustum::ViewFrustum;

// ---------------------------------------------------------------------
// Bounding volumes

pub mod bounding;

// ---------------------------------------------------------------------
// Prelude

/// The shape prelude.
pub mod prelude {
    #[doc(no_inline)]
    pub use crate::{Measured2d, Measured3d, Primitive2d, Primitive3d, WindingOrder};

    #[doc(no_inline)]
    pub use crate::bounding::*;

    #[doc(no_inline)]
    pub use crate::dim2::*;

    #[doc(no_inline)]
    pub use crate::dim3::*;

    #[doc(no_inline)]
    pub use crate::half_space::HalfSpace;

    #[doc(no_inline)]
    pub use crate::inset::Inset;

    #[doc(no_inline)]
    pub use crate::ray::{Ray2d, Ray3d};

    #[doc(no_inline)]
    pub use crate::view_frustum::ViewFrustum;

    #[doc(no_inline)]
    pub use crate::polygon::is_polygon_simple;
}
