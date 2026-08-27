//! [`FromWorld`] — value construction from immutable world context.

use super::{World, WorldId};

/// Constructs a value from immutable world context.
///
/// This is commonly used for resource-style initialization
/// paths that need to derive defaults from world state.
///
/// The trait is implemented automatically for every `Default` type,
/// so resource initialization can simply use `T::default()`;
/// override it when he default value depends on the world.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone, Default)]
/// struct Board {
///     width: u32,
///     height: u32,
/// }
///
/// // `FromWorld` is implemented for every `Default` type.
/// let mut world = World::alloc();
/// let board = Board::from_world(&world);
/// assert_eq!(board.width, 0);
/// assert_eq!(board.height, 0);
/// ```
///
/// [`World::id`]: crate::world::World::id
pub trait FromWorld: Sized + 'static {
    /// Creates `Self` using data available from [`World`].
    fn from_world(world: &World) -> Self;
}

impl<T: Default + 'static> FromWorld for T {
    #[inline]
    fn from_world(_world: &World) -> Self {
        T::default()
    }
}

impl FromWorld for WorldId {
    #[inline]
    fn from_world(world: &World) -> Self {
        world.id()
    }
}
