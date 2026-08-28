//! This module defines primitive shapes.

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
