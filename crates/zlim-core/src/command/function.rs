use zlim_utils::debug::DebugLocation;

use crate::bundle::{Bundle, DataBundle};
use crate::command::Command;
use crate::entity::{EntityError, EntityId, EntityTree};
use crate::error::ZlimError;
use crate::resource::Resource;
use crate::world::{FromWorld, World};

#[cold]
#[inline(always)]
fn bind(e: EntityError, c: DebugLocation) -> ZlimError {
    ZlimError::warning(format!("{e}\n\t{c}"))
}

#[inline(never)]
fn check_spawnable(
    tree: &EntityTree,
    id: EntityId,
    caller: DebugLocation,
) -> Result<(), ZlimError> {
    tree.check_spawnable(id).map_err(|e| bind(e, caller))
}

#[inline(never)]
fn check_contains(tree: &EntityTree, id: EntityId, caller: DebugLocation) -> Result<(), ZlimError> {
    match tree.get(id) {
        Ok(_) => Ok(()),
        Err(e) => Err(bind(e, caller)),
    }
}

/// A [`Command`] that spawns an empty entity.
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub fn spawn_empty(child_of: Option<EntityId>) -> impl Command {
    let caller = DebugLocation::caller();
    move |world: &mut World| -> Result<(), ZlimError> {
        if let Some(c) = child_of {
            check_contains(&world.entities, c, caller)?;
        }

        world.spawn_empty_with_caller(child_of, caller);
        Ok(())
    }
}

/// A [`Command`] that spawns an empty entity at a specific [`EntityId`].
///
/// Returns an error if the target id is not spawnable (already in use
/// with a live generation).
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub fn spawn_empty_at(entity: EntityId, child_of: Option<EntityId>) -> impl Command {
    let caller = DebugLocation::caller();
    move |world: &mut World| -> Result<(), ZlimError> {
        check_spawnable(&world.entities, entity, caller)?;

        if let Some(c) = child_of {
            check_contains(&world.entities, c, caller)?;
        }

        world.spawn_empty_at_with_caller(entity, child_of, caller);
        Ok(())
    }
}

/// A [`Command`] that spawns a new entity from a [`Bundle`].
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub fn spawn<B: Bundle>(bundle: B, child_of: Option<EntityId>) -> impl Command {
    let caller = DebugLocation::caller();
    move |world: &mut World| -> Result<(), ZlimError> {
        if let Some(c) = child_of {
            check_contains(&world.entities, c, caller)?;
        }
        world.spawn_with_caller(bundle, child_of, caller);
        Ok(())
    }
}

/// A [`Command`] that spawns a new entity at a specific [`EntityId`] id.
///
/// Returns an error if the target entity id cannot be used.
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub fn spawn_at<B: Bundle>(
    bundle: B,
    entity: EntityId,
    child_of: Option<EntityId>,
) -> impl Command {
    let caller = DebugLocation::caller();
    move |world: &mut World| -> Result<(), ZlimError> {
        check_spawnable(&world.entities, entity, caller)?;

        if let Some(c) = child_of {
            check_contains(&world.entities, c, caller)?;
        }
        world.spawn_at_with_caller(bundle, entity, child_of, caller);
        Ok(())
    }
}

/// A [`Command`] that consumes an iterator of [`DataBundle`]s to spawn a series of entities.
///
/// This is more efficient than spawning the entities individually.
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub fn spawn_batch<I>(bundles_iter: I, child_of: Option<EntityId>) -> impl Command
where
    I: IntoIterator + Send + Sync + 'static,
    I::Item: DataBundle,
{
    let caller = DebugLocation::caller();
    move |world: &mut World| -> Result<(), ZlimError> {
        if let Some(c) = child_of {
            check_contains(&world.entities, c, caller)?;
        }
        world.spawn_batch_with_caller(bundles_iter, child_of, caller);
        Ok(())
    }
}

/// A [`Command`] that despawns an entity.
///
/// Logs at warning level if the entity does not exist.
#[inline]
#[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
pub fn despawn(entity: EntityId) -> impl Command {
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
pub fn try_despawn(entity: EntityId) -> impl Command {
    let caller = DebugLocation::caller();
    move |world: &mut World| -> () {
        world.try_despawn_with_caller(entity, caller);
    }
}

/// A [`Command`] that initializes a [`Resource`] if it does not exist.
#[inline]
pub fn init_resource<R: Resource + Send + FromWorld>() -> impl Command {
    |world: &mut World| {
        world.init_resource::<R>();
    }
}

/// A [`Command`] that initializes a non-send [`Resource`] if it does not exist.
///
/// The command will be sent to the main thread for execution to ensure safety.
#[inline]
pub fn init_non_send<R: Resource + FromWorld>() -> impl Command {
    |world: &mut World| {
        world.init_non_send::<R>();
    }
}

/// A [`Command`] that inserts a [`Resource`] into the world.
#[inline]
pub fn insert_resource<R: Resource + Send>(resource: R) -> impl Command {
    |world: &mut World| {
        world.insert_resource::<R>(resource);
    }
}

/// A [`Command`] that removes a [`Resource`] from the world.
#[inline]
pub fn remove_resource<R: Resource + Send>() -> impl Command {
    |world: &mut World| {
        world.drop_resource::<R>();
    }
}
