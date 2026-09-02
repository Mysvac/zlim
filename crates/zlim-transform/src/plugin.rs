//! The [`TransformPlugin`] registering transform propagation.

use zlim_app::{App, MainSchedulePlugin, Plugin, PostStartup, PostUpdate};
use zlim_core::job::{JobId, JobLabel};
use zlim_core::schedule::ScheduleLabel;
use zlim_core::world::World;

use crate::propagate::TransformChangeDetection;
use crate::propagate::TransformChangeRoot;
use crate::propagate::TransformPropagateStrategy;
use crate::propagate::TransformPropagation;

/// The transform propagation plugin.
///
/// Installs [`TransformChangeDetection`] and [`TransformPropagation`]
/// into the [`PostStartup`] and [`PostUpdate`] schedules, so that
/// [`GlobalTransform`] is kept in sync with the entity hierarchy and
/// local [`Transform`]s.
///
/// See [`TransformPropagateStrategy`] for optional configurations and
/// algorithm implementation.
///
/// [`GlobalTransform`]: crate::GlobalTransform
/// [`Transform`]: crate::Transform
#[derive(Debug, Default)]
pub struct TransformPlugin {
    pub strategy: TransformPropagateStrategy,
}

impl Plugin for TransformPlugin {
    fn build(&self, app: &mut App) {
        if app.contains_plugin::<MainSchedulePlugin>() {
            app.add_plugin_order::<MainSchedulePlugin, Self>();
        }
    }

    fn apply(&self, app: &mut App) {
        MainSchedulePlugin::warn_if_unset(app, "TransformPlugin");

        let world: &mut World = app.main_world_mut();

        if world.contains_resource::<TransformPropagateStrategy>() {
            zlim_log::warn!(
                "The old `TransformPropagateStrategy` has been overwritten by `TransformPlugin`.\n\
                If you need to configure the transform strategy during app initialization, please \
                modifying the strategy via `TransformPlugin` instead of manually inserting it."
            );
        }

        world.insert_resource::<TransformPropagateStrategy>(self.strategy);
        world.init_resource::<TransformChangeRoot>();

        install(world, PostStartup);
        install(world, PostUpdate);
    }
}

#[inline]
fn install(world: &mut World, schedule: impl ScheduleLabel) {
    let schedule = world.schedule_entry(schedule);

    schedule.insert::<TransformChangeDetection>(());
    schedule.insert::<TransformPropagation>(());

    schedule.insert_order(&[
        JobId::isolated(TransformChangeDetection::name()),
        JobId::isolated(TransformPropagation::name()),
    ]);
}
