//! Per-frame entity counting.
//!
//! [`EntityCountPlugin`] tracks the number of alive entities of the main
//! world in the [`EntityCount`] resource, refreshed once per frame in
//! `PreUpdate`.  [`EntityCountDiagnosticsPlugin`] additionally feeds that
//! count into the `entity_count` diagnostic.

use zlim_app::{App, MainSchedulePlugin, Plugin, PreUpdate, Update};
use zlim_core::borrow::{Res, ResMut};
use zlim_core::derive::Resource;
use zlim_core::job_fn;
use zlim_core::world::World;
use zlim_reflect::derive::TypePath;

use core::sync::atomic::{AtomicU32, Ordering};

use crate::{AppDiagnosticExt, DEFAULT_MAX_HISTORY_LENGTH, DiagnosticsPlugin};
use crate::{Diagnostic, DiagnosticPath, Diagnostics};

// -----------------------------------------------------------------------------
// EntityCount & EntityCountPlugin

/// The number of currently alive entities, tracked once per frame.
///
/// [`EntityCount`] stores the main world's entity count as an atomic `u32`.
/// [`UpdateEntityCount`] refreshes it in `PreUpdate`; later stages (e.g.
/// `Update`) read it through [`EntityCount::get`].
///
/// Both the store and the load use [`Ordering::Relaxed`] — no stronger
/// ordering is required, because the write and the reads are ordered by the
/// schedule itself (the counting job runs before the readers every frame).
/// See the `entity_count_relaxed_order_v1` / `_v2` tests.
#[derive(TypePath, Debug, Default, Resource)]
pub struct EntityCount(AtomicU32);

impl EntityCount {
    /// Returns the entity count stored by the latest [`UpdateEntityCount`]
    /// run.
    ///
    /// Uses a relaxed load; the value is only meaningful after
    /// [`UpdateEntityCount`] has run at least once in the current frame.
    pub fn get(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Refreshes [`EntityCount`] with the world's current entity count.
///
/// Runs once per frame in `PreUpdate`, storing the count with a relaxed
/// store so that jobs in later stages always read this frame's value.
#[job_fn(type = UpdateEntityCount, name = "zlim_diagnostic::UpdateEntityCount")]
fn update_entity_count(world: &World, count: Res<EntityCount>) {
    let num: usize = world.entity_count(); // EntityIndex < u32::MAX
    count.0.store(num as u32, Ordering::Relaxed);
}

/// Tracks the entity count of the main world.
///
/// Registers the [`EntityCount`] resource and inserts the
/// [`UpdateEntityCount`] job into `PreUpdate`, so that systems/jobs running
/// in later stages (e.g. `Update`) can read an up-to-date count through
/// [`EntityCount::get`].
///
/// This plugin only maintains the [`EntityCount`] resource; pair it with
/// [`EntityCountDiagnosticsPlugin`] to expose the count as a diagnostic.
///
/// Requires the schedules defined by [`MainSchedulePlugin`] — usually
/// already present through [`App::new`].
///
/// [`MainSchedulePlugin`]: zlim_app::MainSchedulePlugin
/// [`App::new`]: zlim_app::App::new
#[derive(Debug, Default)]
pub struct EntityCountPlugin;

impl Plugin for EntityCountPlugin {
    fn build(&self, app: &mut App) {
        MainSchedulePlugin::apply_before::<Self>(app);
    }

    fn apply(&self, app: &mut App) {
        MainSchedulePlugin::warn_if_unset(app, "EntityCountPlugin");

        let world = app.main_world_mut();

        world.init_resource::<EntityCount>();
        world
            .schedule_entry(PreUpdate)
            .insert::<UpdateEntityCount>(());
    }
}

// -----------------------------------------------------------------------------
// EntityCountDiagnosticsPlugin

/// Adds the `entity_count` diagnostic to an app.
///
/// Depends on [`EntityCountPlugin`] (added automatically if missing) for the
/// per-frame [`EntityCount`] resource, and pushes the current count into the
/// [`Diagnostics`] resource once per frame in `Update`.
#[derive(Debug)]
pub struct EntityCountDiagnosticsPlugin {
    /// Number of samples kept in history.
    pub max_history_length: usize,
}

impl Default for EntityCountDiagnosticsPlugin {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_HISTORY_LENGTH)
    }
}

impl EntityCountDiagnosticsPlugin {
    /// The [`DiagnosticPath`] of the `entity_count` diagnostic.
    pub const ENTITY_COUNT: DiagnosticPath = DiagnosticPath::new("entity_count");

    /// Creates a plugin using the provided history length.
    pub const fn new(max_history_length: usize) -> Self {
        Self { max_history_length }
    }
}

/// Pushes the current [`EntityCount`] into the `entity_count` diagnostic.
///
/// Runs once per frame in `Update`, after [`UpdateEntityCount`] has refreshed
/// the count in `PreUpdate`.
#[job_fn(type = UpdateEntityCountDiagnostics, name = "zlim_diagnostic::UpdateEntityCountDiagnostics")]
fn diagnostic_system(count: Res<EntityCount>, mut store: ResMut<Diagnostics>) {
    let value: f64 = count.get() as f64;
    store.add_measurement(&EntityCountDiagnosticsPlugin::ENTITY_COUNT, || value);
}

impl Plugin for EntityCountDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        if !app.contains_plugin::<EntityCountPlugin>() {
            app.add_plugins(EntityCountPlugin);
        }
        if !app.contains_plugin::<DiagnosticsPlugin>() {
            app.add_plugins(DiagnosticsPlugin);
        }
        MainSchedulePlugin::apply_before::<Self>(app);
    }

    fn apply(&self, app: &mut App) {
        MainSchedulePlugin::warn_if_unset(app, "EntityCountDiagnosticsPlugin");

        let diag =
            Diagnostic::new(Self::ENTITY_COUNT).with_max_history_length(self.max_history_length);

        app.register_diagnostic(diag)
            .schedule_entry(Update)
            .insert::<UpdateEntityCountDiagnostics>(());
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use zlim_app::{PreUpdate, Update};
    use zlim_core::borrow::Res;
    use zlim_core::command::Commands;
    use zlim_core::job::{JobId, JobLabel};
    use zlim_core::system::Local;

    /// Verifies that [`EntityCount`]'s `Ordering::Relaxed` access is
    /// sufficient: [`UpdateEntityCount`] refreshes the count in `PreUpdate`
    /// and this job reads it in `Update` (a strictly later stage), so the
    /// relaxed load always observes the latest write of the current frame.
    ///
    /// The first run records the current count; every following run asserts
    /// the count still matches (no entity was spawned in between), then
    /// spawns one entity and bumps the expectation — proving the relaxed
    /// read tracks frame-to-frame entity changes exactly.
    #[job_fn(type = AssertEntityCount, name = "zlim_diagnostic::test::AssertEntityCount")]
    fn assert_entity_count(mut num: Local<u32>, count: Res<EntityCount>, mut cmd: Commands) {
        if *num == 0 {
            *num = count.get();
        } else {
            assert_eq!(count.get(), *num);
            cmd.spawn_empty(None);
            *num += 1;
        }
    }

    #[test]
    fn entity_count_relaxed_order_v1() {
        let mut app = App::new();

        app.add_plugins(EntityCountPlugin)
            .build()
            .main_world_mut()
            .schedule_entry(Update)
            .insert::<AssertEntityCount>(());

        for _ in 0..100 {
            app.update();
        }
    }

    #[test]
    fn entity_count_relaxed_order_v2() {
        let mut app = App::new();

        let schedule = app
            .add_plugins(EntityCountPlugin)
            .build()
            .main_world_mut()
            .schedule_entry(PreUpdate);
        schedule.insert::<AssertEntityCount>(());
        schedule.insert_relaxed_order(&[
            JobId::isolated(UpdateEntityCount::name()),
            JobId::isolated(AssertEntityCount::name()),
        ]);

        for _ in 0..100 {
            app.update();
        }
    }
}
