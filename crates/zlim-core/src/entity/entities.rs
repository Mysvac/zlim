use zlim_utils::hash::{HashMap, NoopState};
use zlim_utils::str::HashStr;

use super::EntityIndex;

use super::{EntityId, EntityVersion};

#[derive(Debug, Clone, Copy, Hash)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub struct EntityLocation {
    // ModelId
    // ModelRow
}

pub struct EntityInfo {
    pub name: HashStr,
    pub depth: u32,
    pub version: EntityVersion,
    pub child_of: Option<EntityId>,
    pub children: Vec<EntityId>,
    pub location: Option<EntityLocation>,
}

pub struct Entities {
    pub entities: Vec<EntityInfo>,
    pub mapper: HashMap<HashStr, EntityIndex, NoopState>,
}
