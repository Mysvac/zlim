//! [`FromWorld`] — value construction from immutable world context.

use super::{World, WorldId};

/// Constructs a value from immutable world context.
///
/// This is commonly used for resource-style initialization
/// paths that need to derive defaults from world state.
///
/// The trait is implemented automatically for every `Default` type, so
/// resource initialization can simply use `T::default()`; override it when
/// the default value depends on the world (e.g. on another resource or on
/// [`World::id`]).
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_core::world::{FromWorld, WorldId};
/// use zlim_reflect::derive::TypePath;
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
///
/// // For `WorldId`, `from_world` returns the id of the given world.
/// let id = WorldId::from_world(&world);
/// assert_eq!(id, world.id());
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
