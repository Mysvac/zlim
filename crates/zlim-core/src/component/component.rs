//! The [`Component`] trait.
#![expect(clippy::module_inception, reason = "For better structure.")]

use zlim_reflect::{Reflect, TypePath};

use super::db::ComponentDB;
use super::hook::ComponentHook;
use super::register::register_base;
use super::required::Required;
use crate::clone::ComponentCloner;
use crate::entity::EntityMapper;
use crate::utils::Dropper;

// -----------------------------------------------------------------------------
// Component
// -----------------------------------------------------------------------------

/// The core trait for all component types.
///
/// Any type stored in ECS component storage must implement this trait.
///
/// `Component` describes runtime metadata that drives how ECS stores and
/// manages values of this type: memory layout, clone and drop behavior,
/// lifecycle hooks, and reflected field access.
///
/// # Derive Macro
///
/// Most users should not implement this trait manually. Prefer deriving it
/// with the [Component derive macro], which sets sensible defaults and
/// validates options.
///
/// ```rust
/// use zlim_core::prelude::*;
/// use std::collections::BTreeSet;
///
/// // Basic usage. Deriving also auto-submits the type for bulk
/// // registration (see `register_component!`).
/// #[derive(TypePath, Component, Clone)]
/// struct Position {
///     x: f32,
///     y: f32,
/// }
///
/// // Expose fields to the editor through reflection:
/// #[derive(TypePath, Component, Clone)]
/// struct Transform {
///     #[editor(get, set)]
///     x: f32,
///     #[editor(get, set)]
///     y: f32,
///     #[editor(get, set)]
///     z: f32,
/// }
///
/// // Declare *required* components: `Visibility` is registered, collected,
/// // and written automatically whenever `Transform3D` is spawned/inserted.
/// #[derive(TypePath, Component, Clone, Default)]
/// struct Visibility;
///
/// #[derive(TypePath, Component, Clone, Default)]
/// #[require(Visibility)]
/// struct Transform3D;
///
/// // Components containing entities are remapped when cloned into another
/// // world; `#[entities]` generates the remapping code for you:
/// #[derive(TypePath, Component, Clone)]
/// struct Linked {
///     #[entities]
///     linked_entities: BTreeSet<EntityId>,
/// }
///
/// // Spawning a component also registers and writes its required
/// // components:
/// let mut world = World::alloc();
/// let entity = world.spawn(Transform3D, None);
/// assert!(entity.get::<Visibility>().is_some());
/// ```
///
/// See the [Component derive macro] documentation for details.
///
/// # Safety
///
/// Implementing this trait promises that the type can be stored behind the
/// ECS' type-erased component storage. If you override [`Self::DROPPER`],
/// it must match the implementor's actual layout and drop behavior.
///
/// [Component derive macro]: crate::derive::Component
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a `Component`",
    label = "invalid `Component`",
    note = "consider annotating `{Self}` with `#[derive(Component)]`"
)]
pub trait Component: TypePath + Send + Sync + Sized {
    /// Required components that must be present on any entity with this
    /// component.
    ///
    /// Required components are auto-registered with this component, added to
    /// the entity's table when this component is spawned or inserted, and
    /// initialised with their [`Default`] value when not provided explicitly.
    /// Every required component must implement [`Default`].
    ///
    /// Defaults to `None` (no required components).  The derive macro sets
    /// this through `#[require(...)]`, which builds a [`Required`] v-table
    /// from a [`RequiredComponents`] type.
    ///
    /// [`RequiredComponents`]: crate::component::RequiredComponents
    const REQUIRED: Option<Required> = None;

    /// Registers this component type in the global registry, returning its
    /// `&'static` [`ComponentDB`].
    ///
    /// Registration is lazy and idempotent: the first call registers the
    /// type, and every subsequent call returns the same [`ComponentDB`]
    /// without creating a duplicate.
    ///
    /// The default implementation performs a base registration **without**
    /// serialization support ([`register_base`]).  Components derived with
    /// `#[component(serialize)]` override this to use
    /// [`register_serializable`] and additionally require the component to
    /// implement `Serialize` and `Deserialize`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_reflect::derive::TypePath;
    ///
    /// #[derive(TypePath, Component, Clone)]
    /// struct Position;
    ///
    /// let db = Position::register();
    /// assert_eq!(db.type_name, "Position");
    /// // Registering twice returns the same entry:
    /// assert!(core::ptr::eq(db, Position::register()));
    /// ```
    ///
    /// [`register_serializable`]: crate::component::register_serializable
    #[inline(always)]
    fn register() -> &'static ComponentDB {
        register_base::<Self>()
    }

    /// When `true`, this component is registered with serialization support.
    ///
    /// Set by `#[derive(Component)]` when annotated with
    /// `#[component(serialize)]`; the registration then fills the
    /// [`ComponentDB::serialize`] / [`ComponentDB::deserialize`] function
    /// pointers so the component can be serialized into scenes.
    ///
    /// Defaults to `false`.
    const SERIALIZE: bool = false;

    /// When `true`, this component contains no entity references, so entity
    /// remapping ([`map_entities`](Self::map_entities)) can be skipped when
    /// the component is cloned into another world.
    ///
    /// `#[derive(Component)]` sets this to `true` automatically unless the
    /// type has `#[entities]` fields or a custom
    /// `#[component(map_entities = ...)]` function.
    ///
    /// Defaults to `false` for manual implementations.
    const NO_ENTITY: bool = false;

    /// An optional function pointer to drop the component when it is deallocated.
    ///
    /// Defaults to `Some(Dropper::of::<Self>())` which calls [`drop`] on `Self`.
    const DROPPER: Option<Dropper> = Dropper::of::<Self>();

    /// The cloning strategy for this component.
    ///
    /// `#[derive(Component)]` sets this to `clonable::<Self>()` by default;
    /// `#[component(copy)]` selects `copyable::<Self>()`, and
    /// `#[component(cloner = path::function)]` selects a custom cloner.
    const CLONER: ComponentCloner;

    /// Hook invoked when the component is **first** added to an entity
    /// (i.e. on entity spawn, or when a brand-new component type is
    /// inserted).
    ///
    /// Called after the component has been written to storage, before
    /// `on_insert`.
    const ON_ADD: Option<ComponentHook> = None;

    /// Hook invoked when this component instance is created by cloning
    /// another (i.e. entity clone).
    ///
    /// Called after entity cloning is complete, before `on_add` and
    /// `on_insert`.
    const ON_CLONE: Option<ComponentHook> = None;

    /// Hook invoked on every insertion, including updates to an entity that
    /// already had this component type (i.e. entity spawn, clone, or
    /// component insert).
    ///
    /// Called after component initialization is complete, after `on_add`.
    const ON_INSERT: Option<ComponentHook> = None;

    /// Hook invoked when the component is removed from an entity (i.e.
    /// component remove or entity despawn).
    ///
    /// Called before the component is actually removed, after `on_discard`.
    const ON_REMOVE: Option<ComponentHook> = None;

    /// Hook invoked when the component value is discarded (i.e. component
    /// replace, remove, or entity despawn).
    ///
    /// Called before the component is actually removed, before `on_remove`
    /// and `on_despawn`.
    const ON_DISCARD: Option<ComponentHook> = None;

    /// Hook invoked when the owning entity is despawned (i.e. entity
    /// despawn).
    ///
    /// Called before the component is actually dropped, after `on_discard`
    /// and `on_remove`.
    const ON_DESPAWN: Option<ComponentHook> = None;

    /// Names of the fields readable through [`get_field`](Self::get_field),
    /// in declaration order. Defaults to empty.
    const GETTER: &'static [&'static str] = &[];

    /// Names of the fields writable through [`set_field`](Self::set_field).
    /// Defaults to empty.
    const SETTER: &'static [&'static str] = &[];

    /// Returns a reflected reference to the named field, if it exists.
    ///
    /// The default implementation exposes no fields and always returns
    /// `None`. `#[derive(Component)]` exposes fields through the
    /// `#[editor(get, set)]` attribute (see [`GETTER`](Self::GETTER)).
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_reflect::derive::TypePath;
    ///
    /// #[derive(TypePath, Component, Clone)]
    /// struct Transform {
    ///     #[editor(get, set)]
    ///     x: f32,
    /// }
    ///
    /// let transform = Transform { x: 1.5 };
    /// let field = transform.get_field("x").unwrap();
    /// assert_eq!(field.downcast_ref::<f32>(), Some(&1.5));
    /// ```
    fn get_field<'a>(&'a self, _name: &str) -> Option<&'a dyn Reflect> {
        None
    }

    /// Assigns a reflected value to the named field.
    ///
    /// Returns `Err(message)` if `name` does not match any setter field of
    /// this type, or if applying `value` through reflection fails.
    ///
    /// The default implementation exposes no fields and always returns
    /// `Err` (see [`SETTER`](Self::SETTER)).
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_reflect::derive::TypePath;
    ///
    /// #[derive(TypePath, Component, Clone)]
    /// struct Transform {
    ///     #[editor(get, set)]
    ///     x: f32,
    /// }
    ///
    /// let mut transform = Transform { x: 0.0 };
    /// transform.set_field("x", &1.5).unwrap();
    /// assert_eq!(
    ///     transform.get_field("x").unwrap().downcast_ref::<f32>(),
    ///     Some(&1.5),
    /// );
    /// ```
    fn set_field(&mut self, _name: &str, _value: &dyn Reflect) -> Result<(), String> {
        let ty = Self::type_path();
        Err(format!("Component `{ty}` exposes no fields"))
    }

    /// Remaps entity references inside this component.
    ///
    /// Called during entity cloning / scene instantiation, when the
    /// component's [`EntityId`] values must be translated to the target
    /// world's entities. The default implementation is a no-op. Override
    /// this if your component stores [`EntityId`] values that need
    /// remapping; `#[derive(Component)]` generates it automatically from
    /// `#[entities]` fields.
    ///
    /// [`EntityId`]: crate::entity::EntityId
    #[inline(always)]
    fn map_entities<M: EntityMapper>(&mut self, _: &mut M) {}
}

// -----------------------------------------------------------------------------
