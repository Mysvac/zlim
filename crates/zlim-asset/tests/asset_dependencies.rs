//! Tests for asset dependencies, loader file dependencies and labeled sub-assets.
//!
//! The loaders here are deliberately data driven: the content of a `.dep` file
//! names the asset it depends on, so one loader can drive every dependency
//! scenario without registering a loader per case.

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use zlim_app::{App, Plugin, PluginExt};
use zlim_asset::assets::Assets;
use zlim_asset::error::{AssetLoadError, WaitForAssetError};
use zlim_asset::event::AssetSourceEvent;
use zlim_asset::handle::Handle;
use zlim_asset::ident::AssetSourceId;
use zlim_asset::io::Reader;
use zlim_asset::io::memory::MemoryAssetReader;
use zlim_asset::io::watcher::AssetWatcher;
use zlim_asset::loaded::LoadedFolder;
use zlim_asset::loader::{AssetLoader, LoadContext};
use zlim_asset::plugin::{AppAssetExt, AssetPlugin};
use zlim_asset::server::{AssetServer, LoadState};
use zlim_asset::source::AssetSourceBuilder;
use zlim_core::error::ZlimError;
use zlim_path::TypePath;
use zlim_utils::mpmc::Sender;

// -----------------------------------------------------------------------------
// Harness
// -----------------------------------------------------------------------------

/// Frames a test is willing to drive before it calls the wait a timeout.
const MAX_FRAMES: usize = 1_000;

/// Drives `app` for at most [`MAX_FRAMES`] frames until `condition` holds.
#[track_caller]
fn drive_until(app: &mut App, what: &str, condition: impl Fn() -> bool) {
    for _ in 0..MAX_FRAMES {
        app.update();
        if condition() {
            return;
        }
        std::thread::yield_now();
    }

    panic!("waited {MAX_FRAMES} frames for {what}, but it never happened");
}

/// [`drive_until`] for a condition that has to look at the world itself.
#[track_caller]
fn drive_until_world(app: &mut App, what: &str, condition: impl Fn(&App) -> bool) {
    for _ in 0..MAX_FRAMES {
        app.update();
        if condition(app) {
            return;
        }
        std::thread::yield_now();
    }

    panic!("waited {MAX_FRAMES} frames for {what}, but it never happened");
}

/// Registers `reader` as the default asset source, bypassing the file system.
struct MemorySourcePlugin(MemoryAssetReader);

impl Plugin for MemorySourcePlugin {
    fn build(&mut self, app: &mut App) {
        let reader = self.0.clone();
        app.register_asset_source(
            AssetSourceId::Default,
            AssetSourceBuilder::new(move || Box::new(reader.clone())),
        );
    }

    fn apply(&mut self, _app: &mut App) {}
}

/// The state the loaders of this file share with the test that drives them.
#[derive(Default)]
struct SideState {
    /// How often the side-file loader ran.
    runs: AtomicUsize,
    /// The bytes the last run read through `LoadContext::read_asset_bytes`.
    bytes: Mutex<Vec<u8>>,
}

/// Registers the asset types and the loaders this file uses.
struct RegisterPlugin {
    /// Released by the test to let [`GateLoader`] finish.
    gate: Arc<AtomicBool>,
    /// Shared with [`SideReaderLoader`].
    side: Arc<SideState>,
}

impl Plugin for RegisterPlugin {
    fn build(&mut self, app: &mut App) {
        AssetPlugin::apply_before::<Self>(app);
    }

    fn apply(&mut self, app: &mut App) {
        app.init_asset::<LoadedFolder>()
            .register_asset_loader(LeafLoader)
            .register_asset_loader(DependentLoader)
            .register_asset_loader(GateLoader(self.gate.clone()))
            .register_asset_loader(PanicLoader)
            .register_asset_loader(ErrLoader)
            .register_asset_loader(SideReaderLoader(self.side.clone()))
            .register_asset_loader(LabeledLoader)
            .register_asset_loader(UnlabeledLoader);
    }
}

/// Reads the whole asset as UTF-8 text, trimmed.
async fn read_text(reader: &mut dyn Reader) -> Result<String, ZlimError> {
    let mut bytes = Vec::new();
    reader
        .read_all_bytes(&mut bytes)
        .await
        .map_err(|error| ZlimError::error(error))?;

    Ok(String::from_utf8_lossy(&bytes).trim().to_owned())
}

/// An asset without dependencies.
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

/// Loads the asset named by the file's content and embeds its handle.
#[derive(TypePath)]
struct DependentLoader;

impl AssetLoader for DependentLoader {
    type Asset = LoadedFolder;
    type Settings = ();

    const EXTENSIONS: &[&'static str] = &["dep"];

    async fn load(
        &self,
        reader: &mut dyn Reader,
        context: &mut LoadContext<'_>,
        _settings: &Self::Settings,
    ) -> Result<Self::Asset, AssetLoadError> {
        let dep_path = read_text(reader).await?;
        let dep: Handle<LoadedFolder> = context.load(&dep_path);

        // Embedding the handle also records the dependency through
        // `LoadedFolder`'s derived `VisitAssetDependencies`.
        Ok(LoadedFolder {
            handles: vec![dep.erased()],
        })
    }
}

/// Stays in `Loading` until the test opens the gate.
///
/// The loop yields to the task pool instead of sleeping, so the dependency is
/// held open for as long as the test needs without depending on wall time.
#[derive(TypePath)]
struct GateLoader(Arc<AtomicBool>);

impl AssetLoader for GateLoader {
    type Asset = LoadedFolder;
    type Settings = ();

    const EXTENSIONS: &[&'static str] = &["gate"];

    async fn load(
        &self,
        _reader: &mut dyn Reader,
        _context: &mut LoadContext<'_>,
        _settings: &Self::Settings,
    ) -> Result<Self::Asset, AssetLoadError> {
        // A safety bound only: the test always opens the gate.
        const MAX_SPINS: usize = 1 << 26;

        for _ in 0..MAX_SPINS {
            if self.0.load(Ordering::Acquire) {
                return Ok(LoadedFolder {
                    handles: Vec::new(),
                });
            }
            zlim_task::yield_now().await;
        }

        panic!("the test never opened the gate within {MAX_SPINS} yields");
    }
}

/// Always panics, exercising the loader-panic path.
#[derive(TypePath)]
struct PanicLoader;

impl AssetLoader for PanicLoader {
    type Asset = LoadedFolder;
    type Settings = ();

    const EXTENSIONS: &[&'static str] = &["panic"];

    async fn load(
        &self,
        _reader: &mut dyn Reader,
        _context: &mut LoadContext<'_>,
        _settings: &Self::Settings,
    ) -> Result<Self::Asset, AssetLoadError> {
        panic!("PanicLoader always panics; this is the failing-dependency scenario")
    }
}

/// Always returns an error, exercising the loader-error path.
#[derive(TypePath)]
struct ErrLoader;

impl AssetLoader for ErrLoader {
    type Asset = LoadedFolder;
    type Settings = ();

    const EXTENSIONS: &[&'static str] = &["err"];

    async fn load(
        &self,
        _reader: &mut dyn Reader,
        _context: &mut LoadContext<'_>,
        _settings: &Self::Settings,
    ) -> Result<Self::Asset, AssetLoadError> {
        Err(AssetLoadError::from(String::from("ErrLoader always fails")))
    }
}

/// Reads a second file through the loader context and records the run.
#[derive(TypePath)]
struct SideReaderLoader(Arc<SideState>);

impl AssetLoader for SideReaderLoader {
    type Asset = LoadedFolder;
    type Settings = ();

    const EXTENSIONS: &[&'static str] = &["side"];

    async fn load(
        &self,
        _reader: &mut dyn Reader,
        context: &mut LoadContext<'_>,
        _settings: &Self::Settings,
    ) -> Result<Self::Asset, AssetLoadError> {
        self.0.runs.fetch_add(1, Ordering::AcqRel);

        // The side file's bytes are a *loader* dependency of this asset.
        let bytes = context
            .read_asset_bytes("side.data")
            .await
            .map_err(|error| ZlimError::error(error))?;

        *self
            .0
            .bytes
            .lock()
            .expect("the test never poisons this mutex") = bytes;

        Ok(LoadedFolder {
            handles: Vec::new(),
        })
    }
}

/// Registers a sub-asset under the label `part`.
#[derive(TypePath)]
struct LabeledLoader;

impl AssetLoader for LabeledLoader {
    type Asset = LoadedFolder;
    type Settings = ();

    const EXTENSIONS: &[&'static str] = &["label"];

    async fn load(
        &self,
        _reader: &mut dyn Reader,
        context: &mut LoadContext<'_>,
        _settings: &Self::Settings,
    ) -> Result<Self::Asset, AssetLoadError> {
        context.add_labeled_asset(
            "part",
            LoadedFolder {
                handles: Vec::new(),
            },
        );

        Ok(LoadedFolder {
            handles: Vec::new(),
        })
    }
}

/// Registers no sub-asset at all.
#[derive(TypePath)]
struct UnlabeledLoader;

impl AssetLoader for UnlabeledLoader {
    type Asset = LoadedFolder;
    type Settings = ();

    const EXTENSIONS: &[&'static str] = &["nolabel"];

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

/// Creates an in-memory source out of `files` and returns the built app.
fn test_app(files: &[(&str, &str)]) -> (App, AssetServer, Arc<AtomicBool>, Arc<SideState>) {
    let memory = MemoryAssetReader::default();
    for (path, contents) in files {
        memory.root.insert_asset_text(Path::new(path), contents);
    }

    let gate = Arc::new(AtomicBool::new(false));
    let side = Arc::new(SideState::default());

    let mut app = App::new();
    app.add_plugins((
        MemorySourcePlugin(memory),
        AssetPlugin {
            watch_for_changes_override: Some(false),
            ..AssetPlugin::default()
        },
        RegisterPlugin {
            gate: gate.clone(),
            side: side.clone(),
        },
    ));
    app.build();

    let server = app.main_world().resource::<AssetServer>().clone();
    (app, server, gate, side)
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

// -----------------------------------------------------------------------------
// Dependencies
// -----------------------------------------------------------------------------

/// Loads a file whose content names another file and checks the whole chain: the
/// parent ends up loaded with its dependency, it embeds exactly the dependency's
/// handle, and the dependency is loaded in its own right. The content-driven
/// `DependentLoader` is what makes this a real dependency rather than a
/// hand-built fixture.
#[test]
fn a_parent_waits_for_its_dependency() {
    // `parent.dep` names `dep_leaf.leaf` as its dependency, so the parent cannot
    // finish before the dependency does.
    let (mut app, server, _gate, _side) =
        test_app(&[("parent.dep", "dep_leaf.leaf"), ("dep_leaf.leaf", "")]);

    let handle = server.load::<LoadedFolder>("parent.dep");

    // "Fully" loaded is the stricter state: it includes the dependencies.
    drive_until(
        &mut app,
        "the parent to be loaded with its dependency",
        || server.is_fully_loaded(handle.id()),
    );

    let parent = app
        .main_world()
        .resource::<Assets<LoadedFolder>>()
        .get(&handle)
        .expect("the parent should be stored")
        .handles
        .clone();
    assert_eq!(
        parent.len(),
        1,
        "the parent should embed exactly one dependency handle",
    );

    let dep = server
        .get_handle::<LoadedFolder>("dep_leaf.leaf")
        .expect("the dependency should be known to the server");
    assert_eq!(
        parent[0].id(),
        dep.id(),
        "the embedded handle should be the dependency's handle",
    );
    assert!(
        server.is_fully_loaded(dep.id()),
        "the dependency itself should be loaded with its (empty) dependencies",
    );
}

/// Parks the dependency in `Loading` behind the gate and checks the states
/// separately: the parent's own load is done, both its direct and its recursive
/// dependency state still report `Loading`, and only opening the gate turns the
/// parent into one that is loaded with its dependencies.
#[test]
fn a_parent_is_not_loaded_with_dependencies_until_the_dependency_finished() {
    // Two loads have to be in flight at once for the state this test asserts: the parent has to
    // finish while its gated dependency is still `Loading`. With the single-threaded task pool the
    // driver runs one load to completion before the next, so the gated dependency never finishes and
    // the parent never gets there; there is no concurrent state to observe.
    if zlim_task::cfg::single_thread!() {
        // This does *not* mean single-threaded is wrong: the test would pass there too.
        // It is only the way this test parks the dependency behind the gate that stops
        // producing a state worth asserting on.
        return;
    }

    let (mut app, server, gate, _side) =
        test_app(&[("parent.dep", "gate.gate"), ("gate.gate", "")]);

    let handle = server.load::<LoadedFolder>("parent.dep");

    // The parent's own load returns immediately; the gated dependency stays in
    // `Loading` until the test opens the gate below.
    drive_until(&mut app, "the parent to finish its own load", || {
        server.is_loaded(handle.id())
    });

    assert!(
        !server.is_fully_loaded(handle.id()),
        "the parent must not count as fully loaded while its dependency is still loading",
    );
    assert!(
        server
            .get_dependency_load_state(handle.id())
            .is_some_and(|state| state.is_loading()),
        "the direct dependency state should be `Loading`, got {:?}",
        server.get_dependency_load_state(handle.id()),
    );
    assert!(
        server
            .get_recursive_dependency_load_state(handle.id())
            .is_some_and(|state| state.is_loading()),
        "the recursive dependency state should be `Loading`, got {:?}",
        server.get_recursive_dependency_load_state(handle.id()),
    );

    // Only now may the parent become fully loaded.
    gate.store(true, Ordering::Release);
    drive_until(
        &mut app,
        "the parent to be loaded once the gate opened",
        || server.is_fully_loaded(handle.id()),
    );
}

/// A panicking loader fails the asset it was loading, and that failure has to
/// travel to whoever waits on the asset that depends on it — while the parent
/// itself stays successfully loaded. The failing dependency also remains
/// reachable, so a caller can find out which asset went wrong.
#[test]
fn wait_for_asset_reports_dependency_failed_when_a_dependency_panics() {
    let (mut app, server, _gate, _side) =
        test_app(&[("parent.dep", "boom.panic"), ("boom.panic", "")]);

    let handle = server.load::<LoadedFolder>("parent.dep");
    // The wait runs on the IO pool, so the app has to be driven for it to settle.
    let task = spawn_wait(&server, &handle);

    drive_until(&mut app, "the parent's wait to settle", || {
        task.is_finished()
    });

    let result = zlim_task::block_on(task);
    assert!(
        matches!(result, Err(WaitForAssetError::DependencyFailed(_))),
        "a panicking dependency should report `DependencyFailed`, got {result:?}",
    );
    assert!(
        matches!(server.load_state(handle.id()), LoadState::Loaded),
        "the parent's own load succeeded, so it should be `Loaded`, got {:?}",
        server.load_state(handle.id()),
    );

    let dep = server
        .get_handle::<LoadedFolder>("boom.panic")
        .expect("the parent keeps the failing dependency's handle alive");
    assert!(
        server.load_state(dep.id()).is_failed(),
        "the panicking dependency should be `Failed`, got {:?}",
        server.load_state(dep.id()),
    );
}

/// The erroring counterpart of the panicking case above: a loader that returns an
/// error fails its own asset just as a panic does, and that failure reaches the
/// waiter as `DependencyFailed` while the parent stays loaded. Both kinds of
/// failure arrive as the same variant, so a caller has one case to handle.
#[test]
fn wait_for_asset_reports_dependency_failed_when_a_dependency_errors() {
    let (mut app, server, _gate, _side) = test_app(&[("parent.dep", "boom.err"), ("boom.err", "")]);

    let handle = server.load::<LoadedFolder>("parent.dep");
    // The wait runs on the IO pool, so the app has to be driven for it to settle.
    let task = spawn_wait(&server, &handle);

    drive_until(&mut app, "the parent's wait to settle", || {
        task.is_finished()
    });

    let result = zlim_task::block_on(task);
    assert!(
        matches!(result, Err(WaitForAssetError::DependencyFailed(_))),
        "an erroring dependency should report `DependencyFailed`, got {result:?}",
    );
    assert!(
        matches!(server.load_state(handle.id()), LoadState::Loaded),
        "the parent's own load succeeded, so it should be `Loaded`, got {:?}",
        server.load_state(handle.id()),
    );

    let dep = server
        .get_handle::<LoadedFolder>("boom.err")
        .expect("the parent keeps the failing dependency's handle alive");
    assert!(
        server.load_state(dep.id()).is_failed(),
        "the erroring dependency should be `Failed`, got {:?}",
        server.load_state(dep.id()),
    );
}

// -----------------------------------------------------------------------------
// Watcher-driven hot reloading
// -----------------------------------------------------------------------------

/// Registers the default source over a memory tree, with a watcher the test drives by hand.
struct WatcherSourcePlugin {
    reader: MemoryAssetReader,
    slot: Arc<Mutex<Option<Sender<AssetSourceEvent>>>>,
}

impl Plugin for WatcherSourcePlugin {
    fn build(&mut self, app: &mut App) {
        let reader = self.reader.clone();
        let slot = self.slot.clone();

        app.register_asset_source(
            AssetSourceId::Default,
            AssetSourceBuilder::new(move || Box::new(reader.clone())).with_watcher(
                move |sender: Sender<AssetSourceEvent>| {
                    *slot.lock().expect("the test never poisons this mutex") = Some(sender.clone());

                    Some(Box::new(TestWatcher { sender }) as Box<dyn AssetWatcher>)
                },
            ),
        );
    }

    fn apply(&mut self, _app: &mut App) {}
}

/// Stands in for a file-system watcher: the events the test pushes are what it would emit.
struct TestWatcher {
    #[expect(dead_code, reason = "held so the channel stays alive for the test")]
    sender: Sender<AssetSourceEvent>,
}

impl AssetWatcher for TestWatcher {}

/// The watcher-driven half of hot reloading: a change to a *loader dependency* reloads the assets
/// that read it, and a new file inside a folder reloads that folder's own handle.
#[test]
fn a_watched_change_reloads_dependents_and_parent_folders() {
    let memory = MemoryAssetReader::default();
    for (path, contents) in [
        ("base.side", ""),
        ("side.data", "side-file-contents"),
        ("nested/leaf.leaf", ""),
    ] {
        memory.root.insert_asset_text(Path::new(path), contents);
    }
    let dir = memory.root.clone();

    let slot: Arc<Mutex<Option<Sender<AssetSourceEvent>>>> = Arc::new(Mutex::new(None));
    let gate = Arc::new(AtomicBool::new(false));
    let side = Arc::new(SideState::default());

    let mut app = App::new();
    app.add_plugins((
        WatcherSourcePlugin {
            reader: memory,
            slot: slot.clone(),
        },
        AssetPlugin {
            // The test pushes the watcher events itself, so nothing else has to watch the tree.
            watch_for_changes_override: Some(true),
            ..AssetPlugin::default()
        },
        RegisterPlugin {
            gate: gate.clone(),
            side: side.clone(),
        },
    ));
    app.build();

    let server = app.main_world().resource::<AssetServer>().clone();
    // The source builder captured the sender, so the test can push exactly the
    // events a real watcher would have emitted.
    let events = slot
        .lock()
        .expect("the test never poisons this mutex")
        .clone()
        .expect("the source should have been built with its watcher");

    let asset = server.load::<LoadedFolder>("base.side");
    let folder = server.load_folder("nested");

    // The first run of the side reader loader is the read that registers the
    // reverse edge `side.data` -> `base.side` the reload below travels along.
    drive_until(&mut app, "the asset and its folder to load", || {
        server.is_loaded(asset.id())
            && server.is_fully_loaded(folder.id())
            && side.runs.load(Ordering::Acquire) == 1
    });

    let folder_handles = |app: &App| {
        app.main_world()
            .resource::<Assets<LoadedFolder>>()
            .get(&folder)
            .map(|folder| folder.handles.len())
    };

    assert_eq!(
        folder_handles(&app),
        Some(1),
        "only the one loadable file below the folder is held",
    );

    // The side file changed: the assets that *read* it through their loader are reloaded, which is
    // what the reverse loader-dependency edges are for.
    events
        .send(AssetSourceEvent::ModifiedAsset(PathBuf::from("side.data")))
        .expect("the watcher channel is alive");

    drive_until(&mut app, "the dependent asset to be reloaded", || {
        side.runs.load(Ordering::Acquire) >= 2
    });

    // A new file appeared in the folder: the folder's own handle is reloaded and lists it.
    dir.insert_asset_text(Path::new("nested/new.leaf"), "");
    events
        .send(AssetSourceEvent::AddedAsset(PathBuf::from(
            "nested/new.leaf",
        )))
        .expect("the watcher channel is alive");

    drive_until_world(&mut app, "the folder to pick the new asset up", |app| {
        folder_handles(app) == Some(2)
    });

    assert!(
        server.is_fully_loaded(folder.id()),
        "the folder's own value is loaded again after being reloaded",
    );
}

// -----------------------------------------------------------------------------
// File-level (loader) dependencies
// -----------------------------------------------------------------------------

/// The loader-side half of dependencies: `LoadContext::read_asset_bytes` hands the
/// loader the bytes of a companion file, and a reload re-runs the loader, so the
/// side file is read a second time.
#[test]
fn read_asset_bytes_reads_the_side_file_and_the_asset_can_be_reloaded() {
    let (mut app, server, _gate, side) = test_app(&[
        ("base.side", "the base asset content is ignored"),
        ("side.data", "side-file-contents"),
    ]);

    let handle = server.load::<LoadedFolder>("base.side");
    drive_until(&mut app, "the side reader loader to run once", || {
        side.runs.load(Ordering::Acquire) == 1 && server.is_loaded(handle.id())
    });

    let bytes = side
        .bytes
        .lock()
        .expect("the test never poisons this mutex")
        .clone();
    assert_eq!(
        bytes, b"side-file-contents",
        "`read_asset_bytes` should return the content of the side file",
    );

    // A reload re-runs the loader, which reads the side file again.
    server.reload("base.side");
    drive_until(
        &mut app,
        "the reload to run the loader a second time",
        || side.runs.load(Ordering::Acquire) >= 2,
    );

    assert!(
        server.is_loaded(handle.id()),
        "the asset should still be loaded after being reloaded",
    );
}

// -----------------------------------------------------------------------------
// Labeled sub-assets
// -----------------------------------------------------------------------------

/// A loader can register a sub-asset under a label, and a path carrying that label
/// has to find it: the sub-asset gets its own entry in the store while its path
/// still points at the file that contains it, label included.
#[test]
fn a_labeled_sub_asset_is_reachable_through_its_label() {
    let (mut app, server, _gate, _side) = test_app(&[("asset.label", "")]);

    // The base asset is loaded on its own as well: the load of a labeled path
    // only keeps the base's handle alive for the duration of that load, so a
    // caller that wants the base asset to stay around has to hold a handle for
    // it (here, and in real code).
    let base = server.load::<LoadedFolder>("asset.label");
    let handle = server.load::<LoadedFolder>("asset.label#part");
    drive_until(&mut app, "the labeled sub-asset to load", || {
        server.is_loaded(handle.id()) && server.is_loaded(base.id())
    });

    assert!(
        app.main_world()
            .resource::<Assets<LoadedFolder>>()
            .get(&handle)
            .is_some(),
        "the sub-asset should have its own entry in `Assets<LoadedFolder>`",
    );

    let path = server
        .get_path(handle.id())
        .expect("the labeled sub-asset always has a path");
    assert_eq!(
        path.path(),
        Path::new("asset.label"),
        "the sub-asset lives under the path of the file that contains it",
    );
    assert_eq!(
        path.label(),
        Some("part"),
        "the path of a sub-asset keeps its label",
    );

    assert_eq!(
        server
            .get_handle::<LoadedFolder>("asset.label")
            .expect("the base asset should be known to the server")
            .id(),
        base.id(),
        "the sub-asset should belong to the very file the base handle refers to",
    );
}

/// Asking for a label the loader never registered fails the load and stores
/// nothing, rather than quietly falling back to the file's own asset.
#[test]
fn requesting_a_label_the_loader_never_registered_fails() {
    let (mut app, server, _gate, _side) = test_app(&[("plain.nolabel", "")]);

    let handle = server.load::<LoadedFolder>("plain.nolabel#part");
    drive_until(&mut app, "the unknown label to be reported", || {
        server.load_state(handle.id()).is_failed()
    });

    assert!(
        !server.is_loaded(handle.id()),
        "a sub-asset that was never registered must not be stored",
    );
    assert!(
        !app.main_world()
            .resource::<Assets<LoadedFolder>>()
            .contains(&handle),
        "nothing should be stored for a label the loader never registered",
    );
}
