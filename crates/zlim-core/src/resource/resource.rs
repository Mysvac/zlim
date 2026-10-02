//! The [`Resource`] trait.
#![expect(clippy::module_inception, reason = "For better structure.")]

use super::db::ResourceDB;
use super::register::register_base;

// -----------------------------------------------------------------------------
// Resource
// -----------------------------------------------------------------------------

/// A type that can be stored as a global resource in the ECS `World`.
///
/// A resource is a singleton value identified by its concrete Rust type.
/// At most one value of a given resource type can exist in a [`World`].
/// Thread-safety determines which access APIs are available:
///
/// - `Sync` resources can be read through [`Res`];
///
/// - `Send` resources can be written through [`ResMut`].
///
/// - `!Sync` resources must stay on the main thread and are read through
///   [`NonSend`].
///
/// - `!Send` resources must stay on the main thread and are written through
///   [`NonSendMut`].
///
/// # Derive Macro
///
/// For most resource types, prefer using the [Resource derive macro].
///
/// ```ignore
/// // Basic usage
/// #[derive(Resource)]
/// struct Foo;
/// ```
///
/// See [Resource derive macro] documentation for details.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(Resource)]
/// struct Score(u32);
///
/// let mut world = World::alloc();
///
/// // Insert a resource value; the type is registered on first use.
/// world.insert_resource(Score(100));
///
/// // Read it back through the world.
/// assert_eq!(world.get_resource::<Score>().unwrap().0, 100);
/// ```
///
/// [`World`]: crate::world::World
/// [`Res`]: crate::borrow::Res
/// [`ResMut`]: crate::borrow::ResMut
/// [`NonSend`]: crate::borrow::NonSend
/// [`NonSendMut`]: crate::borrow::NonSendMut
/// [Resource derive macro]: crate::derive::Resource
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a `Resource`",
    label = "invalid `Resource`",
    note = "consider annotating `{Self}` with `#[derive(Resource)]`"
)]
pub trait Resource: 'static + Sized {
    /// Registers this resource type in the global registry, returning its
    /// `&'static` [`ResourceDB`].
    ///
    /// Registration is lazy and idempotent: the first call registers the
    /// type, and every subsequent call returns the same [`ResourceDB`]
    /// without creating a duplicate.
    ///
    /// Defaults to a base registration **without** reflection support
    /// ([`register_base`]).  Resources derived with `#[resource(reflect)]`
    /// instead use [`register_reflect`], which additionally requires the
    /// resource to implement [`Reflect`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// #[derive(Resource)]
    /// struct Score(u32);
    ///
    /// let db = <Score as Resource>::REGISTER();
    /// assert_eq!(db.type_name, "Score");
    /// ```
    ///
    /// [`Reflect`]: zlim_reflect::Reflect
    /// [`register_base`]: crate::resource::register_base
    /// [`register_reflect`]: crate::resource::register_reflect
    /// [`ResourceDB`]: crate::resource::ResourceDB
    const REGISTER: fn() -> &'static ResourceDB = register_base::<Self>;
}
