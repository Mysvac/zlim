use core::ptr::NonNull;

use zlim_utils::mem::Bump;

use crate::component::ComponentId;
use crate::entity::{EntityId, EntityMap};
use crate::utils::Dropper;

pub struct SceneBump {
    pub bump: Bump,
    pub hierarchy: Vec<Vec<EntityId>>,
    pub entities: EntityMap<EntityBump>,
}

pub struct EntityBump {
    pub id: EntityId,
    pub meta: Vec<BumpMeta>,
}

pub struct BumpMeta {
    pub ptr: NonNull<u8>,
    pub size: usize,
    pub id: ComponentId,
    pub dropper: Option<Dropper>,
}
