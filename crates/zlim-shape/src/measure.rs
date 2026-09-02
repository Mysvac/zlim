/// A trait for getting measurements of 2D shapes
pub trait Measured2d {
    /// Get the perimeter of the shape
    fn perimeter(&self) -> f32;

    /// Get the area of the shape
    fn area(&self) -> f32;
}

/// A trait for getting measurements of 3D shapes
pub trait Measured3d {
    /// Get the surface area of the shape
    fn area(&self) -> f32;

    /// Get the volume of the shape
    fn volume(&self) -> f32;
}
