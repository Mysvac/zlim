//! This module holds local implementations of the [`Distribution`] trait for [`StandardUniform`].
//!
//! Which allow certain zlim math types (those whose values can be randomly generated without
//! additional input other than an [`RngExt`]) to be produced using [`rand`]'s APIs.

use core::f32::consts::{PI, TAU};

use glam::Vec3A;
use rand::RngExt;
use rand::distr::{Distribution, StandardUniform};

use crate::{Dir2, Dir3, Dir3A, Rot2, Vec2, Vec3, ops};

impl Distribution<Dir2> for StandardUniform {
    #[inline]
    fn sample<R: RngExt + ?Sized>(&self, rng: &mut R) -> Dir2 {
        let theta = rng.random_range(0.0..TAU);
        let (sin, cos) = ops::sin_cos(theta);
        let vector = Vec2::new(cos, sin);
        Dir2::new_unchecked(vector)
    }
}

impl Distribution<Dir3> for StandardUniform {
    #[inline]
    fn sample<R: RngExt + ?Sized>(&self, rng: &mut R) -> Dir3 {
        let z = rng.random_range(-1f32..=1f32);
        let (a_sin, a_cos) = ops::sin_cos(rng.random_range(-PI..=PI));
        let c = ops::sqrt(1f32 - z * z);
        let x = a_sin * c;
        let y = a_cos * c;

        Dir3::new_unchecked(Vec3::new(x, y, z))
    }
}

impl Distribution<Dir3A> for StandardUniform {
    #[inline]
    fn sample<R: RngExt + ?Sized>(&self, rng: &mut R) -> Dir3A {
        let z = rng.random_range(-1f32..=1f32);
        let (a_sin, a_cos) = ops::sin_cos(rng.random_range(-PI..=PI));
        let c = ops::sqrt(1f32 - z * z);
        let x = a_sin * c;
        let y = a_cos * c;

        Dir3A::new_unchecked(Vec3A::new(x, y, z))
    }
}

impl Distribution<Rot2> for StandardUniform {
    #[inline]
    fn sample<R: RngExt + ?Sized>(&self, rng: &mut R) -> Rot2 {
        let angle = rng.random_range(0.0..TAU);
        Rot2::radians(angle)
    }
}
