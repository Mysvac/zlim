use crate::{GlobalTransform, Transform};

use zlim_core::command::{EntityCommand, EntityCommands};
use zlim_core::entity::{EntityError, EntityId};
use zlim_core::error::ZlimError;
use zlim_core::ops::EntityOwned;
use zlim_core::tick::DetectChangesMut;

// -----------------------------------------------------------------------------
// EntityTransformExt

#[inline(never)]
fn reparent_in_place<'a, 'b>(
    entity: &'a mut EntityOwned<'b>,
    parent: Option<EntityId>,
) -> Result<&'a mut EntityOwned<'b>, EntityError> {
    entity.reparent(parent)?;
    match parent {
        Some(parent) => {
            let parent_transform = entity
                .world()
                // At present, reparent will not trigger any hook,
                // So, if it succeeds, the entity should exist.
                .entity_ref(parent)
                .get::<GlobalTransform>();
            let Some(parent_transform) = parent_transform else {
                return Ok(entity);
            };
            let Some(global_transform) = entity.get::<GlobalTransform>() else {
                return Ok(entity);
            };

            let new_transform = global_transform.reparented_to(parent_transform);
            let Some(mut transform) = entity.get_mut::<Transform>() else {
                return Ok(entity);
            };
            *transform.bypass() = new_transform;
        }
        None => {
            let Some(global_transform) = entity.get::<GlobalTransform>() else {
                return Ok(entity);
            };
            let new_transform = global_transform.compute_transform();
            let Some(mut transform) = entity.get_mut::<Transform>() else {
                return Ok(entity);
            };
            *transform.bypass() = new_transform;
        }
    }
    Ok(entity)
}

pub trait EntityTransformExt {
    fn reparent_in_place(&mut self, parent: Option<EntityId>) -> Result<&mut Self, EntityError>;
}

impl EntityTransformExt for EntityOwned<'_> {
    /// Changes the entity's parent while automatically adjusting the [`Transform`]
    /// so that the [`GlobalTransform`] remains unchanged.
    ///
    /// Note that this function does **not** mark the `Transform` component as [changed],
    /// even though the transform value itself may have been modified.
    ///
    /// If the entity does not have a [`Transform`] component, no additional operations are performed.
    ///
    /// [changed]: zlim_core::tick::DetectChanges
    #[inline]
    fn reparent_in_place(&mut self, parent: Option<EntityId>) -> Result<&mut Self, EntityError> {
        reparent_in_place(self, parent)
    }
}

// -----------------------------------------------------------------------------
// EntityCommandsTransformExt

#[inline]
fn reparent_command(parent: Option<EntityId>) -> impl EntityCommand {
    move |mut entity: EntityOwned| -> Result<(), ZlimError> {
        match reparent_in_place(&mut entity, parent) {
            Ok(_) => Ok(()),
            Err(e) => Err(ZlimError::from(e)),
        }
    }
}

pub trait EntityCommandsTransformExt {
    fn reparent_in_place(&mut self, parent: Option<EntityId>) -> &mut Self;

    fn try_reparent_in_place(&mut self, parent: Option<EntityId>) -> &mut Self;
}

impl EntityCommandsTransformExt for EntityCommands<'_> {
    /// Changes the entity's parent while automatically adjusting the [`Transform`]
    /// so that the [`GlobalTransform`] remains unchanged.
    ///
    /// Note that this function does **not** mark the `Transform` component as [changed],
    /// even though the transform value itself may have been modified.
    ///
    /// If the entity does not have a [`Transform`] component, no additional operations are performed.
    ///
    /// If the entity (or parent) does not exist, this command will log a warning.
    ///
    /// Note that if the parent entity does not exist, the operation will not take effect.
    ///
    /// [changed]: zlim_core::tick::DetectChanges
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    fn reparent_in_place(&mut self, parent: Option<EntityId>) -> &mut Self {
        self.queue(reparent_command(parent))
    }

    /// Changes the entity's parent while automatically adjusting the [`Transform`]
    /// so that the [`GlobalTransform`] remains unchanged.
    ///
    /// Note that this function does **not** mark the `Transform` component as [changed],
    /// even though the transform value itself may have been modified.
    ///
    /// If the entity does not have a [`Transform`] component, no additional operations are performed.
    ///
    /// If parent entity does not exist, the operation will not take effect.
    ///
    /// Errors are ignored if the entity is despawned before command execution,
    /// or the parent entity is already despawned.
    ///
    /// [changed]: zlim_core::tick::DetectChanges
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    fn try_reparent_in_place(&mut self, parent: Option<EntityId>) -> &mut Self {
        self.queue_silenced(reparent_command(parent))
    }
}
