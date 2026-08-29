#![doc = include_str!("../README.md")]

mod entity;
mod plugin;
mod propagate;
mod traits;
mod transform;

pub use entity::{EntityCommandsTransformExt, EntityTransformExt};
pub use plugin::TransformPlugin;
pub use propagate::TransformChangeDetection;
pub use propagate::TransformChangeRoot;
pub use propagate::TransformPropagateStrategy;
pub use propagate::TransformPropagation;
pub use traits::TransformPoint;
pub use transform::{GlobalTransform, Transform};
