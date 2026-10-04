//! The scene plugin: what installs the scene assets, the queue, and the job that answers it.
//!
//! The job itself lives in the `spawn` module, next to the queue it walks; this module only wires it
//! up.

use zlim_app::{App, MainSchedulePlugin, Plugin, PluginExt, SpawnScene};
use zlim_asset::plugin::{AppAssetExt, AssetPlugin};

use crate::patch::{SceneListPatch, ScenePatch};
use crate::spawn::{HandleSceneSpawn, SceneQueue};

// -----------------------------------------------------------------------------
// ScenePlugin

/// Registers the scene assets, the queue, and the job that builds what the queue holds.
///
/// Add it after [`AssetPlugin`], which provides the storage the patches
/// go into and the server whose loads they await.
///
/// # Panics
///
/// Panics during [`Plugin::apply`] if [`AssetPlugin`] is missing.
#[derive(Debug, Default)]
pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&mut self, app: &mut App) {
        MainSchedulePlugin::apply_before::<Self>(app);
        AssetPlugin::apply_before::<Self>(app);
    }

    fn apply(&mut self, app: &mut App) {
        MainSchedulePlugin::warn_if_unset(app, "ScenePlugin");

        app.init_asset::<ScenePatch>();
        app.init_asset::<SceneListPatch>();
        app.init_resource::<SceneQueue>();
        app.add_job::<HandleSceneSpawn>(SpawnScene, ());
    }
}
