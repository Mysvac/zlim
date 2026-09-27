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
    #[doc(inline)]
    pub use crate::propagate::TransformChangeDetection;
    #[doc(inline)]
    pub use crate::propagate::TransformPropagation;
}

/// The transform preludes.
pub mod prelude {
    #[doc(no_inline)]
    pub use crate::TransformPoint;
    #[doc(no_inline)]
    pub use crate::TransformPropagateStrategy;
    #[doc(no_inline)]
    pub use crate::{EntityCommandsTransformExt, EntityTransformExt};
    #[doc(no_inline)]
    pub use crate::{GlobalTransform, Transform};
}

/// The transform plugins.
pub mod plugins {
    #[doc(no_inline)]
    pub use crate::TransformPlugin;
}
