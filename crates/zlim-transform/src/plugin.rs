//! The [`TransformPlugin`] registering transform propagation.

use zlim_app::{App, MainSchedulePlugin, Plugin, PostStartup, PostUpdate};
use zlim_core::job::{JobId, JobLabel};
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
/// [`GlobalTransform`]: crate::GlobalTransform
/// [`Transform`]: crate::Transform
#[derive(Debug, Default, Clone, Copy)]
pub struct TransformPlugin;

impl Plugin for TransformPlugin {
    fn build(&self, app: &mut App) {
        if app.contains_plugin::<MainSchedulePlugin>() {
            app.add_plugin_order::<MainSchedulePlugin, Self>();
        }
    }

    fn apply(&self, app: &mut App) {
        if !app.contains_plugin::<MainSchedulePlugin>() {
            zlim_log::warn!(
                "`TransformPlugin` requires `MainSchedulePlugin` to define the \
                 `PostStartup` and `PostUpdate` schedules. Without it, systems \
                 inserted by TransformPlugin may not run."
            );
        }

        let world: &mut World = app.main_world_mut();
        world.init_resource::<TransformPropagateStrategy>();
        world.init_resource::<TransformChangeRoot>();

        install(world, PostStartup);
        install(world, PostUpdate);
    }
}

fn install(world: &mut World, schedule: impl zlim_core::schedule::ScheduleLabel) {
    let schedule = world.schedule_entry(schedule);

    schedule.insert::<TransformChangeDetection>(());
    schedule.insert::<TransformPropagation>(());

    schedule.insert_order(&[
        JobId::isolated(TransformChangeDetection::name()),
        JobId::isolated(TransformPropagation::name()),
    ]);
}
