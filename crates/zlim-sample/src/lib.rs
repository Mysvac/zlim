#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]

mod from_rng;
pub use from_rng::FromRng;

mod mesh_sampler;
pub use mesh_sampler::UniformMeshSampler;

mod shape_sampler;
pub use shape_sampler::ShapeSample;
pub use shape_sampler::{BoundaryOf, InteriorOf};
