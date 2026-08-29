//! Convenience `Command` / `EntityCommand` constructor functions.

use zlim_utils::debug::DebugLocation;

use crate::bundle::{Bundle, DataBundle};
use crate::command::{Command, EntityCommand};
use crate::entity::{Entities, EntityError, EntityId};
use crate::error::{IntoZlimResult, ZlimError};
use crate::message::{Message, MessageQueue};
use crate::ops::EntityOwned;
use crate::resource::Resource;
use crate::schedule::ScheduleLabel;
use crate::system::{IntoSystem, SystemHandle, SystemInput};
use crate::world::{FromWorld, World};

#[cold]
#[inline(never)]
fn bind(e: EntityError, c: DebugLocation) -> ZlimError {
    ZlimError::warning(format!("{e}\n\t{c}"))
}

#[inline(never)]
fn check_spawnable(tree: &Entities, id: EntityId, caller: DebugLocation) -> Result<(), ZlimError> {
    tree.check_spawnable(id).map_err(|e| bind(e, caller))
}

#[inline(never)]
fn check_contains(tree: &Entities, id: EntityId, caller: DebugLocation) -> Result<(), ZlimError> {
    match tree.get(id) {
        Ok(_) => Ok(()),
        Err(e) => Err(bind(e, caller)),
    }
}

/// A [`Command`] that spawns an empty entity at a specific [`EntityId`].
///
/// Returns an error if the target id is not spawnable (already in use
/// with a live generation).
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub(super) fn spawn_empty_at(entity: EntityId, parent: Option<EntityId>) -> impl Command {
    let caller = DebugLocation::caller();
    move |world: &mut World| -> Result<(), ZlimError> {
        check_spawnable(&world.entities, entity, caller)?;

        if let Some(c) = parent {
            check_contains(&world.entities, c, caller)?;
        }

        world.spawn_empty_at_with_caller(entity, parent, caller);
        Ok(())
    }
}

/// A [`Command`] that spawns a new entity at a specific [`EntityId`] id.
///
/// Returns an error if the target entity id cannot be used.
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub(super) fn spawn_at<B: Bundle>(
    bundle: B,
    entity: EntityId,
    parent: Option<EntityId>,
) -> impl Command {
    let caller = DebugLocation::caller();
    move |world: &mut World| -> Result<(), ZlimError> {
        check_spawnable(&world.entities, entity, caller)?;

        if let Some(c) = parent {
            check_contains(&world.entities, c, caller)?;
        }
        world.spawn_at_with_caller(bundle, entity, parent, caller);
        Ok(())
    }
}

/// A [`Command`] that consumes an iterator of [`DataBundle`]s to spawn a series of entities.
///
/// This is more efficient than spawning the entities individually.
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub(super) fn spawn_batch<I>(bundles_iter: I, parent: Option<EntityId>) -> impl Command
where
    I: IntoIterator + Send + Sync + 'static,
    I::Item: DataBundle,
{
    let caller = DebugLocation::caller();
    move |world: &mut World| -> Result<(), ZlimError> {
        if let Some(c) = parent {
            check_contains(&world.entities, c, caller)?;
        }
        world.spawn_batch_with_caller(bundles_iter, parent, caller);
        Ok(())
    }
}

/// A [`Command`] that despawns an entity.
///
/// Logs at warning level if the entity does not exist.
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub(super) fn despawn(entity: EntityId) -> impl Command {
    let caller = DebugLocation::caller();
    move |world: &mut World| -> Result<(), ZlimError> {
        world
            .despawn_with_caller(entity, caller)
            .map_err(|e| bind(e, caller))
    }
}

/// A [`Command`] that despawns an entity.
///
/// No-op if the entity does not exist.
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub(super) fn try_despawn(entity: EntityId) -> impl Command {
    let caller = DebugLocation::caller();
    move |world: &mut World| -> () {
        world.try_despawn_with_caller(entity, caller);
    }
}

/// A [`Command`] that despawns entities from an iterator.
///
/// Ignores entities that do not exist.
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub(super) fn despawn_batch<I>(entity_iter: I) -> impl Command
where
    I: IntoIterator<Item = EntityId> + Send + Sync + 'static,
{
    let caller = DebugLocation::caller();
    move |world: &mut World| -> () {
        for entity in entity_iter {
            world.try_despawn_with_caller(entity, caller);
        }
    }
}

/// A [`Command`] that initializes a [`Resource`] if it does not exist.
#[inline]
pub(super) fn init_resource<R: Resource + Send + FromWorld>() -> impl Command {
    |world: &mut World| {
        world.init_resource::<R>();
    }
}

/// A [`Command`] that inserts a [`Resource`] into the world.
#[inline]
pub(super) fn insert_resource<R: Resource + Send>(resource: R) -> impl Command {
    |world: &mut World| {
        world.insert_resource::<R>(resource);
    }
}

/// A [`Command`] that removes a [`Resource`] from the world.
#[inline]
pub(super) fn remove_resource<R: Resource + Send>() -> impl Command {
    |world: &mut World| {
        world.drop_resource::<R>();
    }
}

/// A [`Command`] that writes an arbitrary [`Message`].
///
/// Panics when applied if the message type is not registered.
#[inline]
pub(super) fn write_message<M: Message>(message: M) -> impl Command {
    move |world: &mut World| -> Result<(), ZlimError> {
        match world.get_resource_mut::<MessageQueue<M>>() {
            Some(mut queue) => {
                queue.write(message);
                Ok(())
            }
            None => {
                ::core::hint::cold_path();
                let error = format!("Missing MessageQueue<{}>", M::type_path());
                Err(ZlimError::panic(error))
            }
        }
    }
}

/// A [`Command`] that runs the schedule corresponding to the given [`ScheduleLabel`].
#[inline]
pub(super) fn run_schedule(label: impl ScheduleLabel) -> impl Command {
    let label = label.intern();
    move |world: &mut World| world.run_schedule(label)
}

/// A [`Command`] that inserts a system into the world's cache, so it can
/// later be run by [`Commands::invoke_handle`] or
/// [`World::invoke_handle`].
///
/// [`Commands::invoke_handle`]: super::Commands::invoke_handle
/// [`World::invoke_handle`]: crate::world::World::invoke_handle
#[inline]
pub(super) fn insert_system<I, O, M>(
    system: impl IntoSystem<I, O, M> + Send + 'static,
) -> impl Command
where
    I: SystemInput + 'static,
    O: 'static,
    M: 'static,
{
    move |world: &mut World| {
        world.insert_system(system);
    }
}

/// A [`Command`] that removes a system from the world's cache.
///
/// Does nothing if the handle was never inserted (or was already removed).
#[inline]
pub(super) fn remove_system<I, O>(handle: SystemHandle<I, O>) -> impl Command
where
    I: SystemInput + 'static,
    O: 'static,
{
    move |world: &mut World| {
        world.remove_system(handle);
    }
}

/// A [`Command`] that runs the given system, caching its instance.
///
/// A cached instance with the same type identity is reused when applied
/// (see [`World::invoke`]).
#[inline]
pub(super) fn invoke<I, O, M>(
    system: impl IntoSystem<I, O, M> + Send + 'static,
    input: I::Data<'static>,
) -> impl Command
where
    I: SystemInput<Data<'static>: Send> + Send + 'static,
    O: IntoZlimResult<()> + Send + 'static,
    M: 'static,
{
    move |world: &mut World| -> Result<(), ZlimError> {
        match world.invoke::<I, O, M>(system, input) {
            Ok(ret) => ret.into_zlim_result(),
            Err(e) => Err(ZlimError::from(e)),
        }
    }
}

/// A [`Command`] that runs the cached system identified by `handle`.
///
/// The system must have been inserted first (e.g. via
/// [`Commands::insert_system`]); the command fails with
/// [`SystemError::Unregistered`] otherwise.
///
/// [`Commands::insert_system`]: super::Commands::insert_system
/// [`SystemError::Unregistered`]: crate::system::SystemError::Unregistered
#[inline]
pub(super) fn invoke_handle<I, O>(
    handle: SystemHandle<I, O>,
    input: I::Data<'static>,
) -> impl Command
where
    I: SystemInput<Data<'static>: Send> + Send + 'static,
    O: IntoZlimResult<()> + Send + 'static,
{
    move |world: &mut World| -> Result<(), ZlimError> {
        match world.invoke_handle::<I, O>(handle, input) {
            Ok(ret) => ret.into_zlim_result(),
            Err(e) => Err(ZlimError::from(e)),
        }
    }
}

/// A [`Command`] that runs the given system once, without caching.
///
/// A fresh instance is built, initialized, executed, and discarded when
/// applied (see [`World::invoke_once`]).
#[inline]
pub(super) fn invoke_once<I, O, M>(
    system: impl IntoSystem<I, O, M> + Send + 'static,
    input: I::Data<'static>,
) -> impl Command
where
    I: SystemInput<Data<'static>: Send> + Send + 'static,
    O: IntoZlimResult<()> + Send + 'static,
    M: 'static,
{
    move |world: &mut World| -> Result<(), ZlimError> {
        match world.invoke_once::<I, O, M>(system, input) {
            Ok(ret) => ret.into_zlim_result(),
            Err(e) => Err(ZlimError::from(e)),
        }
    }
}

// -----------------------------------------------------------------------------
// pre-defined EntityCommand

/// An [`EntityCommand`] that inserts a [`Bundle`] into an entity.
///
/// # Examples
///
/// ```rust
/// # use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
/// struct Health(u32);
///
/// let mut world = World::alloc();
/// let entity = world.spawn((), None).id();
///
/// let mut commands = world.commands();
/// commands.with_entity(entity).insert(Health(100));
/// ::core::mem::drop(commands);
/// world.flush();
///
/// assert_eq!(
///     world.get_entity(entity).ok().and_then(|e| e.get::<Health>().cloned()),
///     Some(Health(100))
/// );
/// ```
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub(super) fn insert(bundle: impl Bundle) -> impl EntityCommand {
    let caller = DebugLocation::caller();
    move |mut entity: EntityOwned| -> Result<(), ZlimError> {
        match entity.insert_with_caller(bundle, caller) {
            Ok(_) => Ok(()),
            Err(e) => Err(bind(e, caller)),
        }
    }
}

/// An [`EntityCommand`] that inserts a [`Bundle`] into an entity if missing.
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub(super) fn insert_if_new<T: DataBundle>(
    bundle: impl FnOnce() -> T + Send + 'static,
) -> impl EntityCommand {
    let caller = DebugLocation::caller();
    move |mut entity: EntityOwned| -> Result<(), ZlimError> {
        match entity.insert_if_new_with_caller(bundle, caller) {
            Ok(_) => Ok(()),
            Err(e) => Err(bind(e, caller)),
        }
    }
}

/// An [`EntityCommand`] that removes a bundle's explicit components from an entity.
///
/// Only removes the parts that exist and are not depended on by the
/// remaining components.
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub(super) fn remove<T: DataBundle>() -> impl EntityCommand {
    let caller = DebugLocation::caller();
    move |mut entity: EntityOwned| -> Result<(), ZlimError> {
        match entity.remove_explicit_with_caller::<T>(caller) {
            Ok(_) => Ok(()),
            Err(e) => Err(bind(e, caller)),
        }
    }
}

/// An [`EntityCommand`] that removes a bundle's explicit components from an entity.
///
/// Only removes the parts that exist and are not depended on by the
/// remaining components.
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub(super) fn remove_explicit<T: DataBundle>() -> impl EntityCommand {
    let caller = DebugLocation::caller();
    move |mut entity: EntityOwned| -> Result<(), ZlimError> {
        match entity.remove_explicit_with_caller::<T>(caller) {
            Ok(_) => Ok(()),
            Err(e) => Err(bind(e, caller)),
        }
    }
}

/// An [`EntityCommand`] that removes a bundle's components from an entity.
///
/// The required components included in the bundle will also be removed
/// (if they are not dependent on other components).
///
/// Only removes the parts that exist and are not depended on by the
/// remaining components.
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub(super) fn remove_required<T: DataBundle>() -> impl EntityCommand {
    let caller = DebugLocation::caller();
    move |mut entity: EntityOwned| -> Result<(), ZlimError> {
        match entity.remove_required_with_caller::<T>(caller) {
            Ok(_) => Ok(()),
            Err(e) => Err(bind(e, caller)),
        }
    }
}

/// An [`EntityCommand`] that clears all components from an entity.
///
/// The entity is moved to the empty archetype; its sub entities are left
/// untouched.
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub(super) fn clear() -> impl EntityCommand {
    let caller = DebugLocation::caller();
    move |mut entity: EntityOwned| -> Result<(), ZlimError> {
        match entity.clear_with_caller(caller) {
            Ok(_) => Ok(()),
            Err(e) => Err(bind(e, caller)),
        }
    }
}

/// An [`EntityCommand`] that clones an entity.
///
/// - If `recursive` is set to true, it will recursively clone sub entities.
/// - If `recursive` is set to false, it will clone self and skip all sub entities.
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub(super) fn clone(recursive: bool) -> impl EntityCommand {
    let caller = DebugLocation::caller();
    move |mut entity: EntityOwned| -> Result<(), ZlimError> {
        match entity.clone_with_caller(recursive, caller) {
            Ok(_) => Ok(()),
            Err(e) => Err(bind(e, caller)),
        }
    }
}

/// An [`EntityCommand`] that reparent an entity.
///
/// - Pass `None` to detach this entity from its current parent
///   (making it a root entity).
///
/// - Pass `Some(id)` to make `id` the new parent.
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub(super) fn reparent(parent: Option<EntityId>) -> impl EntityCommand {
    let caller = DebugLocation::caller();
    move |mut entity: EntityOwned| -> Result<(), ZlimError> {
        match entity.reparent(parent) {
            Ok(_) => Ok(()),
            Err(e) => Err(bind(e, caller)),
        }
    }
}

// -----------------------------------------------------------------------------
