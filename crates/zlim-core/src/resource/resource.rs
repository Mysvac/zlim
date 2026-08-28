//! The [`Resource`] trait.
#![expect(clippy::module_inception, reason = "For better structure.")]

use zlim_reflect::TypePath;

use super::db::ResourceDB;
use super::register::register_base;
use crate::utils::Dropper;

// -----------------------------------------------------------------------------
// Resource
// -----------------------------------------------------------------------------

/// A type that can be stored as a global resource in the ECS `World`.
///
/// A resource is a singleton value identified by its concrete Rust type.
/// At most one value of a given resource type can exist in a [`World`].
/// Thread-safety determines which access APIs are available:
///
/// - `Sync` resources can be read through [`Res`]; `Send` resources can be
///   written through [`ResMut`].
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
/// #[derive(TypePath, Resource)]
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
/// #[derive(TypePath, Resource)]
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
/// # Safety
///
/// Implementing this trait promises that the type can be stored behind the
/// ECS' type-erased resource storage. If you override [`Self::DROPPER`], it
/// must match the implementor's actual layout and drop behavior.
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
pub trait Resource: TypePath + Sized {
    /// The dropper function for this type, if it is not trivially droppable.
    ///
    /// Set to `Some(...)` when the type [`needs_drop`].
    ///
    /// [`needs_drop`]: core::mem::needs_drop
    const DROPPER: Option<Dropper> = Dropper::of::<Self>();

    /// When `true`, this resource is registered with serialization support.
    ///
    /// Set by `#[derive(Resource)]` when annotated with
    /// `#[resource(serialize)]`; the registration then fills the
    /// [`ResourceDB::serialize`] / [`ResourceDB::deserialize`] function
    /// pointers so the resource can be serialized into scenes.
    ///
    /// Defaults to `false`.
    const SERIALIZE: bool = false;

    /// Registers this resource type in the global registry, returning its
    /// `&'static` [`ResourceDB`].
    ///
    /// Registration is idempotent: calling it again returns the same
    /// metadata.  The default implementation performs a base registration
    /// **without** serialization support ([`register_base`]).  Resources
    /// derived with `#[resource(serialize)]` override this to use
    /// [`register_serializable`] and additionally require the type to
    /// implement `Serialize` and `Deserialize`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_reflect::derive::TypePath;
    ///
    /// #[derive(TypePath, Resource)]
    /// struct Score(u32);
    ///
    /// let db = <Score as Resource>::register();
    /// assert_eq!(db.type_name, "Score");
    /// ```
    ///
    /// [`register_base`]: crate::resource::register_base
    /// [`register_serializable`]: crate::resource::register_serializable
    /// [`ResourceDB`]: crate::resource::ResourceDB
    #[inline(always)]
    fn register() -> &'static ResourceDB {
        register_base::<Self>()
    }
}
