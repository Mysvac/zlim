//! Single-resource storage for the ECS world.
//!
//! # Overview
//!
//! Each resource type in a [`World`] owns exactly one [`Slot`].  The
//! [`Slots`] collection holds all slots in a sparse index, keyed
//! by [`ResourceId`].
//!
//! | Type | Role |
//! |------|------|
//! | [`Slot`] | Raw storage for one resource instance — manages memory, initialisation state, and change-detection ticks. |
//! | [`Slots`] | Sparse collection of all slots, with O(1) lookup by ID or TypeId.  Each slot is lazily allocated on first use. |
//!
//! # Change detection
//!
//! Every slot stores an `added` and `changed` tick, mirroring the change
//! tracking in [`Column`] for components.  Systems can detect resource
//! mutations by comparing their private `last_run` baseline against these
//! stored ticks.
//!
//! # Example
//!
//! ```rust
//! use zlim_core::prelude::*;
//! use core::any::TypeId;
//! use zlim_reflect::derive::TypePath;
//!
//! #[derive(TypePath, Resource)]
//! struct Health(u32);
//!
//! let mut world = World::alloc();
//!
//! // Inserting a resource lazily prepares its slot:
//! world.insert_resource(Health(100));
//!
//! // Prepared slots are reachable from the world for inspection:
//! let slot = world.slots().get_by_type(TypeId::of::<Health>()).unwrap();
//! assert!(slot.is_present());
//! ```
//!
//! [`World`]: crate::world::World
//! [`ResourceId`]: crate::resource::ResourceId
//! [`Column`]: crate::table::Column

mod slot;
mod slots;

pub use slot::Slot;
pub use slots::Slots;
