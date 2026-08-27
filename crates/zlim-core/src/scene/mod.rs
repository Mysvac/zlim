//! Scene bump-storage (placeholder).
//!
//! This module holds experimental data structures for staging scene data
//! (entities and their components) in a bump allocator before committing them
//! to a [`World`]. The API is provisional and subject to change.
//!
//! [`World`]: crate::world::World
//!
//! TODO!

use core::ptr::NonNull;

use zlim_ptr::OwningPtr;
use zlim_utils::mem::Bump;

use crate::component::ComponentId;
use crate::entity::{EntityId, EntityMap};
use crate::resource::ResourceId;
use crate::utils::Dropper;

pub struct DynamicWorld {
    pub bump: Bump,
    pub resources: Vec<ResourceBump>,
    pub entities: EntityMap<EntityBump>,
}

pub struct EntityBump {
    pub id: EntityId,
    pub parent: Option<EntityId>,
    pub components: Vec<ComponentBump>,
}

pub struct ResourceBump {
    pub ptr: NonNull<u8>,
    pub size: usize,
    pub id: ResourceId,
    pub dropper: Option<Dropper>,
}

pub struct ComponentBump {
    pub ptr: NonNull<u8>,
    pub size: usize,
    pub id: ComponentId,
    pub dropper: Option<Dropper>,
}

impl Drop for DynamicWorld {
    fn drop(&mut self) {
        ::core::mem::drop(::core::mem::take(&mut self.entities));
        ::core::mem::drop(::core::mem::take(&mut self.resources));
    }
}

impl Drop for ResourceBump {
    fn drop(&mut self) {
        if let Some(dropper) = self.dropper {
            unsafe {
                dropper.call(OwningPtr::new(self.ptr));
            }
        }
    }
}

impl Drop for ComponentBump {
    fn drop(&mut self) {
        if let Some(dropper) = self.dropper {
            unsafe {
                dropper.call(OwningPtr::new(self.ptr));
            }
        }
    }
}
