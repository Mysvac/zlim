//! Component lifecycle hooks.
//!
//! [`HookContext`] describes which component and entity triggered a hook;
//! [`ComponentHook`] is the function pointer type invoked by the storage, and
//! documents the six hooks and the order they run in.

use zlim_utils::debug::DebugLocation;

use super::ComponentId;
use crate::entity::EntityId;
use crate::world::DeferredWorld;

// -----------------------------------------------------------------------------
// HookContext
// -----------------------------------------------------------------------------

/// Context passed to [`Component`] lifecycle hooks.
///
/// Identifies which component type triggered the hook (`id`), which entity
/// it belongs to (`entity`), and the source location that caused the hook
/// to fire (`caller`).
///
/// [`Component`]: crate::component::Component
#[derive(Debug, Clone, Copy)]
pub struct HookContext {
    /// The [`ComponentId`] of the component that triggered the hook.
    pub id: ComponentId,
    /// The [`EntityId`] of the entity the component belongs to.
    pub entity: EntityId,
    /// The source location (`file:line:column`) where the hook was triggered.
    pub caller: DebugLocation,
}

// -----------------------------------------------------------------------------
// ComponentHook
// -----------------------------------------------------------------------------

/// A lifecycle hook for [`Component`]s.
///
/// A function pointer that receives deferred world access along with a
/// [`HookContext`] describing the triggering component, entity, and location.
///
/// Hooks are attached through the derive macro's
/// `#[component(on_add = ..., on_insert = ..., ...)]` attributes, or by
/// setting the corresponding [`Component`] constants manually.
///
/// # The hooks
///
/// A component has six hooks, three for its initialization and three for its
/// teardown:
///
/// | Hook | Runs when |
/// |------|-----------|
/// | [`on_add`] | the component is **first** added to an entity — a spawn, or a brand-new component type |
/// | [`on_clone`] | the component instance is created by cloning another — an entity clone |
/// | [`on_insert`] | **every** insertion, including an update of a component the entity already had |
/// | [`on_despawn`] | the *entity* is despawned |
/// | [`on_remove`] | the component is removed, or the entity is despawned |
/// | [`on_discard`] | the component value is discarded |
///
/// # Initialization order
///
/// The initialization hooks follow the same shape as the teardown ones: the most
/// specific hook runs first, and the general one (`on_insert`) last, so that by
/// the time it runs the component is fully set up.
///
/// | Path | Hooks, in order |
/// |------|-----------------|
/// | entity spawn | `on_add`, `on_insert` |
/// | entity clone | `on_clone`, `on_add`, `on_insert` |
/// | component insert, new type | `on_add`, `on_insert` |
/// | component insert, already present | `on_discard`, `on_insert` |
///
/// `on_add` fires only when the component type was not on the entity before, so
/// re-inserting a component the entity already has never runs it. `on_clone`
/// fires only for an entity clone, and before the other two, because the cloned
/// value has just been created from another.
///
/// ## Replacing a component
///
/// Inserting over a component the entity already has is a **replace**, and it is
/// the one initialization path that also runs a teardown hook: the old value is
/// discarded before the new one is inserted, so the sequence is `on_discard`,
/// `on_insert`.
///
/// No `on_remove` runs — the component type is still there afterwards, it was
/// only its value that changed. `on_discard` is therefore the hook to use when
/// something owned by the old value (a registration, a handle, a subscription)
/// has to be undone on replace, and `on_insert` is the hook that then sets the
/// new value up.
///
/// # Teardown order
///
/// Despawning an entity runs all three teardown hooks, in the order `on_despawn`,
/// `on_remove`, `on_discard`. `on_despawn` comes first on purpose. It is the only
/// one of the three that knows the whole entity is going away, so it is the hook
/// that can do the work which is only meaningful for a despawn — while the
/// component, and the entity's other components, can still be read. Everything
/// after it is the ordinary removal path, which runs for a component removal just
/// as it does for a despawn.
///
/// Putting the despawn-specific work first has two effects worth knowing about:
///
/// - A user removing an entity gets to fire their `on_despawn` before anything
///   is torn down, and can act on the removal there rather than spreading it
///   across the later hooks;
/// - Whatever `on_despawn` has already dealt with, the later `on_discard` does
///   not have to do again, so the work left for the discard path can be
///   smaller.
///
/// The other paths run only the teardown hooks that apply to them, and all of
/// them run `on_remove` before `on_discard`:
///
/// | Path | Hooks, in order |
/// |------|-----------------|
/// | entity despawn | `on_despawn`, `on_remove`, `on_discard` |
/// | entity clear | `on_remove`, `on_discard` |
/// | component remove | `on_remove`, `on_discard` |
///
/// `on_despawn` runs only when the *entity* is despawned; removing or clearing
/// a component is not a despawn. A replace is not listed here because it removes
/// nothing — it only discards the old value, which is why it appears with the
/// initialization order above.
///
/// # Example
///
/// ```rust
/// use zlim_reflect::TypePath;
/// use zlim_core::prelude::*;
/// use core::sync::atomic::{AtomicUsize, Ordering};
///
/// // Count hook invocations so the example can assert the hook fires.
/// static INSERTS: AtomicUsize = AtomicUsize::new(0);
///
/// fn on_insert(world: DeferredWorld, ctx: HookContext) {
///     zlim_log::info!("component {:?} inserted on entity {:?}", ctx.id, ctx.entity);
///     // `world` derefs to `&World`, so read-only access is available.
///     let _count = world.entity_count();
///     INSERTS.fetch_add(1, Ordering::Relaxed);
/// }
///
/// #[derive(Component, Clone, Debug, PartialEq)]
/// #[component(on_insert = on_insert)]
/// struct Health {
///     value: f32,
/// }
///
/// let mut world = World::alloc();
/// let entity = world.spawn(Health { value: 100.0 }, None);
///
/// // Spawning the entity ran the `on_insert` hook exactly once:
/// assert_eq!(INSERTS.load(Ordering::Relaxed), 1);
/// assert_eq!(entity.get::<Health>(), Some(&Health { value: 100.0 }));
/// ```
///
/// [`on_add`]: crate::component::Component::ON_ADD
/// [`on_clone`]: crate::component::Component::ON_CLONE
/// [`on_insert`]: crate::component::Component::ON_INSERT
/// [`on_despawn`]: crate::component::Component::ON_DESPAWN
/// [`on_remove`]: crate::component::Component::ON_REMOVE
/// [`on_discard`]: crate::component::Component::ON_DISCARD
/// [`Component`]: crate::component::Component
pub type ComponentHook = fn(DeferredWorld, HookContext);
