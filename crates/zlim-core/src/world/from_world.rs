use super::{World, WorldId};

/// Constructs a value from immutable world context.
///
/// This is commonly used for resource-style initialization
/// paths that need to derive defaults from world state.
pub trait FromWorld: Sized + 'static {
    /// Creates `Self` using data available from [`World`].
    fn from_world(world: &mut World) -> Self;
}

impl<T: Default + 'static> FromWorld for T {
    #[inline]
    fn from_world(_world: &mut World) -> Self {
        T::default()
    }
}

impl FromWorld for WorldId {
    #[inline]
    fn from_world(world: &mut World) -> Self {
        world.id()
    }
}
