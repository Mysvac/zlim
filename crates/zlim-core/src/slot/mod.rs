//! Single-resource storage for the ECS world.
//!
//! # Overview
//!
//! Each resource type in a [`World`] owns exactly one [`Slot`].  The
//! [`ResourceSlots`] collection holds all slots in a sparse index, keyed
//! by [`ResourceId`].
//!
//! | Type | Role |
//! |------|------|
//! | [`Slot`] | Raw storage for one resource instance — manages memory, initialisation state, and change-detection ticks. |
//! | [`ResourceSlots`] | Sparse collection of all slots, with O(1) lookup by ID or TypeId.  Each slot is lazily allocated on first use. |
//!
//! # Change detection
//!
//! Every slot stores an `added` and `changed` tick, mirroring the change
//! tracking in [`Column`] for components.  Systems can detect resource
//! mutations by comparing their private `last_run` baseline against these
//! stored ticks.
//!
//! [`World`]: crate::world::World
//! [`ResourceId`]: crate::resource::ResourceId
//! [`Column`]: crate::table::Column

mod slot;
mod slots;

pub use slot::Slot;
pub use slots::ResourceSlots;
