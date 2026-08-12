use crate::clone::EntityCloner;
use crate::world::World;

impl World {
    #[inline]
    pub fn entity_cloner(&mut self) -> EntityCloner<'_> {
        EntityCloner::new(self)
    }
}
