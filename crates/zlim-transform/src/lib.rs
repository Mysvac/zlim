#![doc = include_str!("../README.md")]

mod entity;
mod plugin;
mod propagate;
mod traits;
mod transform;

pub use crate::entity::{EntityCommandsTransformExt, EntityTransformExt};
pub use crate::plugin::TransformPlugin;
pub use crate::propagate::TransformChangeRoot;
pub use crate::propagate::TransformPropagateStrategy;
pub use crate::traits::TransformPoint;
pub use crate::transform::{GlobalTransform, Transform};

/// The transform jobs.
pub mod jobs {
    pub use crate::propagate::TransformChangeDetection;
    pub use crate::propagate::TransformPropagation;
}

/// The transform prelude.
pub mod prelude {
    #[doc(hidden)]
    pub use crate::entity::{EntityCommandsTransformExt, EntityTransformExt};
    #[doc(hidden)]
    pub use crate::plugin::TransformPlugin;
    #[doc(hidden)]
    pub use crate::propagate::TransformPropagateStrategy;
    #[doc(hidden)]
    pub use crate::traits::TransformPoint;
    #[doc(hidden)]
    pub use crate::transform::{GlobalTransform, Transform};
}
