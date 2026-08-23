//! Entity-cloner accessor implemented on `World`.

use crate::clone::EntityCloner;
use crate::world::World;

impl World {
    /// Returns a new [`EntityCloner`] bound to this world.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::derive::Component;
    ///
    /// #[derive(TypePath, Component, Clone, PartialEq, Debug)]
    /// struct Tag;
    ///
    /// let mut world = World::alloc();
    /// let original = world.spawn(Tag, None).id();
    ///
    /// let mut cloner = world.entity_cloner();
    /// let copy = cloner.spawn_clone(original, false);
    /// drop(cloner); // release the world borrow before re-acquiring a handle
    ///
    /// assert_eq!(world.entity_owned(copy).get::<Tag>(), Some(&Tag));
    /// ```
    #[inline]
    pub fn entity_cloner(&mut self) -> EntityCloner<'_> {
        EntityCloner::new(self)
    }
}
