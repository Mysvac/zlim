//! End-to-end tests for [`AssetServer`] loading through an in-memory source.
//!
//! Every test builds a real [`App`] whose default asset source is a
//! [`MemoryAssetReader`], registers the loaders declared in this file, and then
//! drives `App::update` until the server state it is waiting for appears. The
//! loads really run on the IO task pool, so nothing here reaches into the
//! server's internals.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use zlim_app::{App, Plugin, PluginExt};
use zlim_asset::assets::Assets;
use zlim_asset::error::{AssetLoadError, AssetSaveError, WaitForAssetError};
use zlim_asset::event::{AssetEvent, AssetLoadFailedEvent, ErasedAssetLoadFailedEvent};
use zlim_asset::handle::Handle;
use zlim_asset::ident::{AssetId, AssetSourceId, ErasedAssetId};
use zlim_asset::io::Reader;
use zlim_asset::io::Writer;
use zlim_asset::io::memory::{Dir, MemoryAssetReader, MemoryAssetWriter};
use zlim_asset::loaded::{LoadedFolder, LoadedUntypedAsset};
use zlim_asset::loader::{AssetLoader, LoadContext};
use zlim_asset::path::AssetPath;
use zlim_asset::plugin::{AppAssetExt, AssetPlugin};
use zlim_asset::saver::{AssetSaver, SavedAsset};
use zlim_asset::server::{AssetServer, LoadState};
use zlim_asset::source::AssetSourceBuilder;
use zlim_core::message::MessageQueue;
use zlim_path::TypePath;

// -----------------------------------------------------------------------------
// Harness
// -----------------------------------------------------------------------------

/// Frames a test is willing to drive before it calls the wait a timeout.
///
/// The loop is a hard bound, never a sleep: a load either shows up in the
/// server state within `MAX_FRAMES` `App::update` calls or the test fails with
/// the message it passed to [`drive_until`].
const MAX_FRAMES: usize = 1_000;

/// Drives `app` for at most [`MAX_FRAMES`] frames until `condition` holds.
#[track_caller]
fn drive_until(app: &mut App, what: &str, condition: impl Fn() -> bool) {
    for _ in 0..MAX_FRAMES {
        app.update();
        if condition() {
            return;
        }
        // Give the IO pool threads a chance to make progress; this is a yield,
        // not a time-based wait, so the test stays deterministic.
        std::thread::yield_now();
    }

    panic!("waited {MAX_FRAMES} frames for {what}, but it never happened");
}

/// Registers `reader` as the default asset source, bypassing the file system.
///
/// The source gets a writer over the same [`Dir`], so tests can also save assets.
struct MemorySourcePlugin(MemoryAssetReader);

impl Plugin for MemorySourcePlugin {
    fn build(&mut self, app: &mut App) {
        let reader_dir = self.0.root.clone();
        let writer_dir = self.0.root.clone();

        app.register_asset_source(
            AssetSourceId::Default,
            AssetSourceBuilder::new(move || {
                Box::new(MemoryAssetReader {
                    root: reader_dir.clone(),
                })
            })
            .with_writer(move || {
                Some(Box::new(MemoryAssetWriter {
                    root: writer_dir.clone(),
                })
                    as Box<dyn zlim_asset::io::ErasedAssetWriter>)
            }),
        );
    }

    fn apply(&mut self, _app: &mut App) {}
}

/// Registers the asset types and the loaders this file uses.
///
/// `AssetPlugin::apply_before` is what orders this plugin *after* `AssetPlugin`:
/// the apply order of plugins that do not order themselves relative to each
/// other is unspecified, and `init_asset` panics without the server.
struct RegisterPlugin;

impl Plugin for RegisterPlugin {
    fn build(&mut self, app: &mut App) {
        AssetPlugin::apply_before::<Self>(app);
    }

    fn apply(&mut self, app: &mut App) {
        app.init_asset::<LoadedFolder>()
            .init_asset::<LoadedUntypedAsset>()
            .register_asset_loader(LeafLoader)
            .register_asset_loader(FailLoader);
    }
}

/// Produces an empty [`LoadedFolder`]; the asset every `.leaf` file loads into.
#[derive(TypePath)]
struct LeafLoader;

impl AssetLoader for LeafLoader {
    type Asset = LoadedFolder;
    type Settings = ();

    const EXTENSIONS: &[&'static str] = &["leaf"];

    async fn load(
        &self,
        _reader: &mut dyn Reader,
        _context: &mut LoadContext<'_>,
        _settings: &Self::Settings,
    ) -> Result<Self::Asset, AssetLoadError> {
        Ok(LoadedFolder {
            handles: Vec::new(),
        })
    }
}

/// Always fails, so the server reports the asset as [`LoadState::Failed`].
#[derive(TypePath)]
struct FailLoader;

impl AssetLoader for FailLoader {
    type Asset = LoadedFolder;
    type Settings = ();

    const EXTENSIONS: &[&'static str] = &["fail"];

    async fn load(
        &self,
        _reader: &mut dyn Reader,
        _context: &mut LoadContext<'_>,
        _settings: &Self::Settings,
    ) -> Result<Self::Asset, AssetLoadError> {
        Err(AssetLoadError::from(String::from(
            "FailLoader always fails",
        )))
    }
}

/// Creates an in-memory source out of `files` and returns the built app.
fn test_app(files: &[(&str, &str)]) -> (App, AssetServer) {
    let (app, server, _dir) = test_app_with_dir(files);
    (app, server)
}

/// [`test_app`], plus the in-memory tree the source reads from and writes to.
fn test_app_with_dir(files: &[(&str, &str)]) -> (App, AssetServer, Dir) {
    let memory = MemoryAssetReader::default();
    for (path, contents) in files {
        memory.root.insert_asset_text(Path::new(path), contents);
    }
    let dir = memory.root.clone();

    let mut app = App::new();
    app.add_plugins((
        MemorySourcePlugin(memory),
        AssetPlugin {
            watch_for_changes_override: Some(false),
            ..AssetPlugin::default()
        },
        RegisterPlugin,
    ));
    app.build();

    let server = app.main_world().resource::<AssetServer>().clone();
    (app, server, dir)
}

/// Spawns `server.wait_for_asset(handle)` on the IO task pool.
fn spawn_wait(
    server: &AssetServer,
    handle: &Handle<LoadedFolder>,
) -> zlim_task::Task<Result<(), WaitForAssetError>> {
    let server = server.clone();
    let handle = handle.clone();

    zlim_task::IoTaskPool::get().spawn(async move { server.wait_for_asset(&handle).await })
}

/// Every `AssetEvent::FullyLoaded` id that is currently readable in `app`.
fn fully_loaded_ids(app: &App) -> Vec<AssetId<LoadedFolder>> {
    let Some(queue) = app
        .main_world()
        .get_resource::<MessageQueue<AssetEvent<LoadedFolder>>>()
    else {
        return Vec::new();
    };

    let mut ids = Vec::new();
    for index in queue.oldest_message_index()..queue.counter() {
        if let Some((_, AssetEvent::FullyLoaded { id })) = queue.get(index) {
            ids.push(*id);
        }
    }
    ids
}

/// The ids of the typed failure events written for `LoadedFolder`.
fn failed_ids(app: &App) -> Vec<AssetId<LoadedFolder>> {
    let Some(queue) = app
        .main_world()
        .get_resource::<MessageQueue<AssetLoadFailedEvent<LoadedFolder>>>()
    else {
        return Vec::new();
    };

    let mut ids = Vec::new();
    for index in queue.oldest_message_index()..queue.counter() {
        if let Some((_, event)) = queue.get(index) {
            ids.push(event.id);
        }
    }
    ids
}

/// The ids of the type-erased failure events.
fn erased_failed_ids(app: &App) -> Vec<ErasedAssetId> {
    let Some(queue) = app
        .main_world()
        .get_resource::<MessageQueue<ErasedAssetLoadFailedEvent>>()
    else {
        return Vec::new();
    };

    let mut ids = Vec::new();
    for index in queue.oldest_message_index()..queue.counter() {
        if let Some((_, event)) = queue.get(index) {
            ids.push(event.id);
        }
    }
    ids
}

// -----------------------------------------------------------------------------
// Typed load
// -----------------------------------------------------------------------------

/// End-to-end check of a successful load: nothing is loaded before the first
/// frame, and once the load lands the server reports the asset as loaded and fully
/// loaded, knows the path it came from, hands back the very handle the load
/// produced, and has the value in the asset store.
#[test]
fn typed_load_reaches_assets_and_the_server_state() {
    let (mut app, server) = test_app(&[("a.leaf", "")]);

    let handle = server.load::<LoadedFolder>("a.leaf");
    // A load is a request, not a synchronous call: it only completes once frames run.
    assert!(
        !server.is_loaded(handle.id()),
        "the asset must not report as loaded before a single frame was driven",
    );

    drive_until(&mut app, "`a.leaf` to load", || {
        server.is_fully_loaded(handle.id())
    });

    assert!(
        matches!(server.load_state(handle.id()), LoadState::Loaded),
        "`a.leaf` should be `LoadState::Loaded`, got {:?}",
        server.load_state(handle.id()),
    );
    assert!(
        server.is_loaded(handle.id()),
        "`is_loaded` should be true for a loaded asset",
    );
    assert!(
        server.is_fully_loaded(handle.id()),
        "`is_fully_loaded` should be true for an asset without dependencies",
    );
    assert!(
        server
            .get_recursive_dependency_load_state(handle.id())
            .is_some_and(|state| state.is_loaded()),
        "the recursive dependency state of `a.leaf` should be `Loaded`",
    );

    let path = server
        .get_path(handle.id())
        .expect("a loaded asset always has a path");
    assert_eq!(
        path.path(),
        Path::new("a.leaf"),
        "`get_path` should report the path the asset was loaded from",
    );

    let found = server
        .get_handle::<LoadedFolder>("a.leaf")
        .expect("`get_handle` should find the handle of a loaded asset");
    assert_eq!(
        found.id(),
        handle.id(),
        "`get_handle` should return the very handle the load produced",
    );

    let assets = app.main_world().resource::<Assets<LoadedFolder>>();
    let folder = assets
        .get(&handle)
        .expect("the loaded value should be stored in `Assets<LoadedFolder>`");
    assert!(
        folder.handles.is_empty(),
        "`LeafLoader` produces a folder without entries, got {} handles",
        folder.handles.len(),
    );
}

/// The same file requested as a different asset type than its loader produces:
/// the request cannot be satisfied, so the server reports a failed load and keeps
/// nothing under that handle.
#[test]
fn a_type_mismatch_is_reported_as_a_failed_load() {
    let (mut app, server) = test_app(&[("a.leaf", "")]);

    // `a.leaf` is produced by `LeafLoader`, which builds `LoadedFolder`s.
    let handle = server.load::<LoadedUntypedAsset>("a.leaf");

    drive_until(&mut app, "the mismatched load to fail", || {
        server.load_state(handle.id()).is_failed()
    });

    assert!(
        !server.is_loaded(handle.id()),
        "a load whose handle type does not match the loader's asset type must not be stored",
    );
}

// -----------------------------------------------------------------------------
// wait_for_asset
// -----------------------------------------------------------------------------

#[test]
fn wait_for_asset_succeeds_once_the_load_finished() {
    let (mut app, server) = test_app(&[("a.leaf", "")]);

    let handle = server.load::<LoadedFolder>("a.leaf");
    let task = spawn_wait(&server, &handle);

    drive_until(&mut app, "`wait_for_asset` to finish", || {
        task.is_finished()
    });

    let result = zlim_task::block_on(task);
    assert!(
        result.is_ok(),
        "`wait_for_asset` should succeed for a loadable asset, got {result:?}",
    );
}

/// Waiting is only meaningful for a load that is actually in flight: a handle that
/// was reserved but never requested is reported as `NotLoaded`, instead of being
/// waited on for a load that will never start.
#[test]
fn wait_for_asset_reports_not_loaded_for_an_unrequested_asset() {
    let (mut app, server) = test_app(&[]);

    // A reserved handle has an id but no load was ever requested for it.
    let handle = app
        .main_world()
        .resource::<Assets<LoadedFolder>>()
        .reserve_handle();
    assert!(
        !server.contains_by_path("a.leaf"),
        "the fixture must not know about any asset path",
    );

    let task = spawn_wait(&server, &handle);
    drive_until(&mut app, "`wait_for_asset` to give up", || {
        task.is_finished()
    });

    let result = zlim_task::block_on(task);
    assert!(
        matches!(result, Err(WaitForAssetError::NotLoaded)),
        "waiting for an asset that is not being loaded should report `NotLoaded`, got {result:?}",
    );
}

/// A loader error is passed on to the waiter as `Failed`, so a caller can tell a
/// load that went wrong apart from one that was never requested.
#[test]
fn wait_for_asset_reports_failed_for_a_failing_asset() {
    let (mut app, server) = test_app(&[("bad.fail", "")]);

    let handle = server.load::<LoadedFolder>("bad.fail");
    let task = spawn_wait(&server, &handle);

    drive_until(&mut app, "the failing load to settle", || {
        task.is_finished()
    });

    let result = zlim_task::block_on(task);
    assert!(
        matches!(result, Err(WaitForAssetError::Failed(_))),
        "a failing asset should report `WaitForAssetError::Failed`, got {result:?}",
    );
    assert!(
        server.load_state(handle.id()).is_failed(),
        "the failing asset should be `LoadState::Failed`, got {:?}",
        server.load_state(handle.id()),
    );
}

// -----------------------------------------------------------------------------
// Untyped & folder loads
// -----------------------------------------------------------------------------

/// An untyped load produces a wrapper asset whose inner handle can be narrowed to
/// the concrete type the loader built; the concrete asset is then loaded and
/// stored under that inner handle.
#[test]
fn load_untyped_wraps_the_handle_of_the_loaded_asset() {
    let (mut app, server) = test_app(&[("a.leaf", "")]);

    let handle = server.load_builder().load_untyped("a.leaf");
    drive_until(&mut app, "the untyped wrapper to load", || {
        server.is_loaded(handle.id())
    });

    let inner = {
        let untyped_assets = app.main_world().resource::<Assets<LoadedUntypedAsset>>();
        let untyped = untyped_assets
            .get(&handle)
            .expect("the loaded wrapper should be stored in `Assets<LoadedUntypedAsset>`");

        untyped
            .handle
            .clone()
            .try_with_type::<LoadedFolder>()
            .expect("the untyped load wraps the handle of the concrete asset")
    };

    assert!(
        server.is_loaded(inner.id()),
        "the wrapped concrete handle should be loaded as well",
    );
    assert!(
        app.main_world()
            .resource::<Assets<LoadedFolder>>()
            .get(&inner)
            .is_some(),
        "the wrapped handle should resolve in `Assets<LoadedFolder>`",
    );
}

/// A folder load picks up the files below it that some loader claims, at any
/// depth, and leaves the rest alone: `skip.unknown` has no loader, so it is
/// neither part of the folder nor loaded. The paths are compared as a set, which
/// keeps the check independent of the order the walk returns them in.
#[test]
fn load_folder_collects_loadable_files_only() {
    let (mut app, server) = test_app(&[
        ("folder/one.leaf", ""),
        ("folder/two.leaf", ""),
        ("folder/sub/three.leaf", ""),
        // No loader is registered for `unknown`, so this file is not part of the folder.
        ("folder/skip.unknown", ""),
    ]);

    let handle = server.load_folder("folder");
    drive_until(&mut app, "the folder to be fully loaded", || {
        server.is_fully_loaded(handle.id())
    });

    let paths: BTreeSet<PathBuf> = {
        let assets = app.main_world().resource::<Assets<LoadedFolder>>();
        let folder = assets
            .get(&handle)
            .expect("the loaded folder should be stored in `Assets<LoadedFolder>`");

        assert_eq!(
            folder.handles.len(),
            3,
            "the folder should contain exactly the three loadable files, got {:?}",
            folder
                .handles
                .iter()
                .map(|handle| handle.path().map(ToString::to_string))
                .collect::<Vec<_>>(),
        );

        for entry in &folder.handles {
            assert!(
                server.is_loaded(entry.id()),
                "every entry of a fully loaded folder should be loaded, {entry:?} is not",
            );
        }

        folder
            .handles
            .iter()
            .filter_map(|entry| entry.path().map(|path| path.path().to_owned()))
            .collect()
    };

    let expected: BTreeSet<PathBuf> = [
        Path::new("folder").join("one.leaf"),
        Path::new("folder").join("two.leaf"),
        Path::new("folder").join("sub").join("three.leaf"),
    ]
    .into_iter()
    .collect();

    assert_eq!(
        paths, expected,
        "the folder should contain exactly the loadable files, `skip.unknown` was skipped",
    );
}

// -----------------------------------------------------------------------------
// add
// -----------------------------------------------------------------------------

#[test]
fn add_publishes_the_asset() {
    let (mut app, server) = test_app(&[]);

    let handle = server.add(LoadedFolder {
        handles: Vec::new(),
    });

    drive_until(&mut app, "the published asset to be stored", || {
        server.is_loaded(handle.id())
    });

    let assets = app.main_world().resource::<Assets<LoadedFolder>>();
    assert!(
        assets.get(&handle).is_some(),
        "the asset added with `add` should be stored",
    );
    assert_eq!(assets.len(), 1, "only the published asset should be stored");
}

// -----------------------------------------------------------------------------
// save
// -----------------------------------------------------------------------------

/// Writes a fixed byte string, so the test can tell the saver's output apart.
#[derive(TypePath)]
struct FolderSaver;

impl AssetSaver for FolderSaver {
    type Asset = LoadedFolder;
    type Settings = ();
    type LoaderSettings = ();

    const EXTENSIONS: &[&'static str] = &["folder"];

    async fn save(
        &self,
        writer: &mut dyn Writer,
        _path: &AssetPath<'static>,
        _asset: SavedAsset<'_, Self::Asset>,
        _settings: &Self::Settings,
    ) -> Result<(), AssetSaveError> {
        writer
            .write_all_bytes(b"saved")
            .await
            .map_err(|error| AssetSaveError::from(error.to_string()))?;

        Ok(())
    }

    async fn build_settings(
        &self,
        _path: &AssetPath<'static>,
        _asset: SavedAsset<'_, Self::Asset>,
        _settings: &Self::Settings,
    ) -> Result<Self::LoaderSettings, AssetSaveError> {
        Ok(())
    }
}

/// Saving is deferred: the call only queues a command, and the next frame is what
/// writes the bytes. A plain save writes the asset only, so no `.meta` shows up
/// beside it.
#[test]
fn save_writes_the_asset_bytes_when_the_frame_runs() {
    let (mut app, server, dir) = test_app_with_dir(&[]);
    server.register_saver(FolderSaver);

    // `add` publishes the asset into `Assets<LoadedFolder>` in the same frame.
    let handle = server.add(LoadedFolder {
        handles: Vec::new(),
    });
    drive_until(&mut app, "the asset to be published", || {
        server.is_loaded(handle.id())
    });

    // `save` only queues a command; the job the plugin schedules is what writes it. No frame has
    // run since, so nothing can be on disk yet.
    server.save("saved.folder", handle.clone());
    assert!(
        dir.get_asset(Path::new("saved.folder")).is_none(),
        "`AssetServer::save` should only queue the command",
    );

    app.update();

    assert_eq!(
        dir.get_asset(Path::new("saved.folder"))
            .expect("the save job should have written the bytes")
            .value(),
        b"saved",
    );
    assert!(
        dir.get_meta(Path::new("saved.folder")).is_none(),
        "a plain save writes the asset bytes and no `.meta`",
    );
}

// -----------------------------------------------------------------------------
// Events
// -----------------------------------------------------------------------------

/// Polls the message queue on every frame rather than waiting for the load and
/// looking afterwards: the success event stays readable for only a frame or two,
/// so a check made later would miss it.
#[test]
fn a_successful_load_writes_a_fully_loaded_event() {
    let (mut app, server) = test_app(&[("a.leaf", "")]);

    let handle = server.load::<LoadedFolder>("a.leaf");
    let mut seen: Vec<AssetId<LoadedFolder>> = Vec::new();

    for _ in 0..MAX_FRAMES {
        app.update();
        // The queue is double buffered: a message written this frame is dropped
        // after the next two rotations, so every frame is inspected.
        seen.extend(fully_loaded_ids(&app));

        if seen.contains(&handle.id()) {
            break;
        }
        std::thread::yield_now();
    }

    assert!(
        seen.contains(&handle.id()),
        "no `AssetEvent::FullyLoaded` was written for {handle:?} within {MAX_FRAMES} frames; \
         ids seen: {seen:?}",
    );
}

/// A failed load is reported through both the typed and the type-erased failure event, which is how
/// an app reacts to a load it cannot wait for.
#[test]
fn a_failed_load_writes_the_failure_events() {
    let (mut app, server) = test_app(&[("a.fail", "")]);

    let handle = server.load::<LoadedFolder>("a.fail");
    let erased_id = handle.id().erased();

    let mut typed: Vec<AssetId<LoadedFolder>> = Vec::new();
    let mut erased: Vec<ErasedAssetId> = Vec::new();

    for _ in 0..MAX_FRAMES {
        app.update();
        // Both queues are double buffered, so every frame is inspected.
        typed.extend(failed_ids(&app));
        erased.extend(erased_failed_ids(&app));

        if typed.contains(&handle.id()) && erased.contains(&erased_id) {
            break;
        }
        std::thread::yield_now();
    }

    assert!(
        typed.contains(&handle.id()),
        "no typed `AssetLoadFailedEvent` was written for {handle:?}; ids seen: {typed:?}",
    );
    assert!(
        erased.contains(&erased_id),
        "no `ErasedAssetLoadFailedEvent` was written for {erased_id:?}; ids seen: {erased:?}",
    );
}
