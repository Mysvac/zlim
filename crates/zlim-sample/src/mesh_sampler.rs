//! Functionality related to random sampling from triangle meshes.

use rand::{RngExt, distr::Distribution};
use rand_distr::weighted::{Error, WeightedAliasIndex};
use zlim_math::Vec3;
use zlim_shape::{Measured2d, Triangle3d};

use crate::shape_sampler::ShapeSample;

/// A [distribution] that caches data to allow fast sampling from a collection of triangles.
///
/// Generally used through [`sample`] or [`sample_iter`].
///
/// [distribution]: Distribution
/// [`sample`]: Distribution::sample
/// [`sample_iter`]: Distribution::sample_iter
///
/// Example
/// ```
/// # use zlim_shape::prelude::*;
/// # use zlim_math::Vec3;
/// # use zlim_sample::UniformMeshSampler;
/// # use rand::{SeedableRng, rngs::StdRng, distr::Distribution};
/// #
/// let faces = Tetrahedron::default().faces();
/// let sampler = UniformMeshSampler::try_new(faces).unwrap();
/// let rng = StdRng::seed_from_u64(8765309);
/// // 50 random points on the tetrahedron:
/// let samples: Vec<Vec3> = sampler.sample_iter(rng).take(50).collect();
/// ```
pub struct UniformMeshSampler {
    triangles: Vec<Triangle3d>,
    distribution: WeightedAliasIndex<f32>,
}

impl Distribution<Vec3> for UniformMeshSampler {
    fn sample<R: RngExt + ?Sized>(&self, rng: &mut R) -> Vec3 {
        let face_index = self.distribution.sample(rng);
        self.triangles[face_index].sample_interior(rng)
    }
}

impl UniformMeshSampler {
    /// Construct a new [`UniformMeshSampler`] from a list of [triangles].
    ///
    /// Returns an error if the distribution of areas for the collection of triangles
    /// could not be formed (most notably if the collection has zero surface area).
    ///
    /// [triangles]: Triangle3d
    pub fn try_new<T>(triangles: T) -> Result<Self, Error>
    where
        T: IntoIterator<Item = Triangle3d>,
    {
        let triangles: Vec<Triangle3d> = triangles.into_iter().collect();
        let areas: Vec<f32> = triangles.iter().map(Measured2d::area).collect();

        let distribution = WeightedAliasIndex::new(areas)?;
        Ok(Self {
            triangles,
            distribution,
        })
    }
}
