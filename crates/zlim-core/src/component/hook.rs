//! Component lifecycle hooks.
//!
//! [`HookContext`] describes which component and entity triggered a hook;
//! [`ComponentHook`] is the function pointer type invoked by the storage.

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
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_reflect::derive::TypePath;
/// use std::sync::atomic::{AtomicUsize, Ordering};
///
/// // Count hook invocations so the example can assert the hook fires.
/// static INSERTS: AtomicUsize = AtomicUsize::new(0);
///
/// fn on_insert(world: DeferredWorld, ctx: HookContext) {
///     log::info!("component {:?} inserted on entity {:?}", ctx.id, ctx.entity);
///     // `world` derefs to `&World`, so read-only access is available.
///     let _count = world.entity_count();
///     INSERTS.fetch_add(1, Ordering::Relaxed);
/// }
///
/// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
/// #[component(on_insert = on_insert)]
/// struct Health {
///     value: f32,
/// }
///
/// let mut world = World::alloc();
/// let entity = world.spawn(Health { value: 100.0 }, None);
/// // Spawning the entity ran the `on_insert` hook exactly once:
/// assert_eq!(INSERTS.load(Ordering::Relaxed), 1);
/// assert_eq!(entity.get::<Health>(), Some(&Health { value: 100.0 }));
/// ```
///
/// [`Component`]: crate::component::Component
pub type ComponentHook = fn(DeferredWorld, HookContext);
