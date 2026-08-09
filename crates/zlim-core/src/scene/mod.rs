use core::ptr::NonNull;

use zlim_utils::mem::Bump;

use crate::component::ComponentId;
use crate::component::alias::WritterFunc;
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
    pub id: ComponentId,
    pub writter: WritterFunc,
    pub dropper: Option<Dropper>,
}
