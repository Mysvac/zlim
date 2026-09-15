//! Tests for [`AssetPlugin`]'s wiring: the built-in asset types, the sources it
//! builds, the importer it starts in `Processed` mode, and the ordering rule that
//! makes `init_asset` usable at all.

use core::panic::AssertUnwindSafe;
// `catch_unwind` has no `core` counterpart: unwinding is a `std` facility.
use std::panic::catch_unwind;
use std::path::{Path, PathBuf};

use zlim_app::{App, Plugin, PluginExt};
use zlim_asset::asset::Asset;
use zlim_asset::assets::Assets;
use zlim_asset::error::{AssetLoadError, AssetSaveError};
use zlim_asset::ident::AssetSourceId;
use zlim_asset::io::memory::{Dir, MemoryAssetReader, MemoryAssetWriter};
use zlim_asset::io::{ErasedAssetReader, ErasedAssetWriter, Reader, Writer};
use zlim_asset::loaded::{LoadedFolder, LoadedUntypedAsset};
use zlim_asset::loader::{AssetLoader, LoadContext};
use zlim_asset::plugin::{AppAssetExt, AssetPlugin};
use zlim_asset::processor::{AssetProcessServer, LoadTransformAndSave};
use zlim_asset::saver::{AssetSaver, SavedAsset};
use zlim_asset::server::{AssetServer, AssetServerMode};
use zlim_asset::source::AssetSourceBuilder;
use zlim_asset::transformer::IdentityTransformer;
use zlim_core::world::FromWorld;
use zlim_path::TypePath;

/// Builds an app with `AssetPlugin` and no extra source, then runs one frame.
#[test]
fn asset_plugin_registers_the_builtin_asset_types() {
    let mut app = App::new();
    app.add_plugins(AssetPlugin {
        // The tests never touch the file system, and watching would keep a
        // watcher thread alive for the whole test binary.
        watch_for_changes_override: Some(false),
        ..AssetPlugin::default()
    });
    app.build();

    let world = app.main_world();
    assert!(
        world.contains_resource::<AssetServer>(),
        "`AssetPlugin` should insert the `AssetServer` resource",
    );
    assert!(
        world.contains_resource::<Assets<()>>(),
        "`AssetPlugin` should register `Assets<()>`",
    );
    assert!(
        world.contains_resource::<Assets<LoadedFolder>>(),
        "`AssetPlugin` should register `Assets<LoadedFolder>`",
    );
    assert!(
        world.contains_resource::<Assets<LoadedUntypedAsset>>(),
        "`AssetPlugin` should register `Assets<LoadedUntypedAsset>`",
    );

    // The default source the plugin builds is the file system source; the test
    // only checks that a frame with those jobs runs, it never loads from it.
    app.update();
    app.update();
}

/// `init_asset` before `AssetPlugin` was applied must panic, not silently drop
/// the registration.
#[test]
fn init_asset_before_asset_plugin_panics() {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mut app = App::new();
        app.add_plugins(AssetPlugin {
            watch_for_changes_override: Some(false),
            ..AssetPlugin::default()
        });

        // `AssetPlugin` is lazy: it has not been applied yet, so there is no
        // `AssetServer` and `init_asset` has to report that.
        app.init_asset::<LoadedFolder>();
    }));

    assert!(
        result.is_err(),
        "`AppAssetExt::init_asset` should panic when `AssetPlugin` has not been applied, \
         because no `AssetServer` exists yet",
    );
}

/// The documented ordering helper is what makes `init_asset` work from another
/// plugin's `apply`, so the built app must end up with the asset registered.
#[test]
fn a_plugin_ordered_after_asset_plugin_can_register_asset_types() {
    /// Registers `Assets<LoadedFolder>` from its `apply`.
    struct LaterPlugin;

    impl Plugin for LaterPlugin {
        fn build(&mut self, app: &mut App) {
            AssetPlugin::apply_before::<Self>(app);
        }

        fn apply(&mut self, app: &mut App) {
            app.init_asset::<LoadedFolder>();
        }
    }

    let mut app = App::new();
    app.add_plugins((
        AssetPlugin {
            watch_for_changes_override: Some(false),
            ..AssetPlugin::default()
        },
        LaterPlugin,
    ));
    app.build();

    assert!(
        app.main_world().contains_resource::<Assets<LoadedFolder>>(),
        "a plugin ordered after `AssetPlugin` should be able to call `init_asset`",
    );

    app.update();
}

// -----------------------------------------------------------------------------
// Processed mode
// -----------------------------------------------------------------------------

/// The asset the importer processes and the app loads back.
#[derive(TypePath, Asset)]
struct TextAsset(String);

#[derive(TypePath)]
struct TextLoader;

impl AssetLoader for TextLoader {
    type Asset = TextAsset;
    type Settings = ();

    const EXTENSIONS: &[&'static str] = &["txt"];

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _context: &mut LoadContext<'_>,
        _settings: &Self::Settings,
    ) -> Result<Self::Asset, AssetLoadError> {
        let mut bytes = Vec::new();
        reader
            .read_all_bytes(&mut bytes)
            .await
            .map_err(|error| AssetLoadError::from(error.to_string()))?;

        Ok(TextAsset(String::from_utf8_lossy(&bytes).into_owned()))
    }
}

#[derive(TypePath)]
struct TextSaver;

impl AssetSaver for TextSaver {
    type Asset = TextAsset;
    type Settings = ();
    type LoaderSettings = ();

    const EXTENSIONS: &[&'static str] = &["txt"];

    async fn save(
        &self,
        writer: &mut dyn Writer,
        _path: &zlim_asset::path::AssetPath<'static>,
        asset: SavedAsset<'_, Self::Asset>,
        _settings: &Self::Settings,
    ) -> Result<(), AssetSaveError> {
        writer
            .write_all_bytes(asset.get().0.as_bytes())
            .await
            .map_err(|error| AssetSaveError::from(error.to_string()))?;

        Ok(())
    }

    async fn build_settings(
        &self,
        _path: &zlim_asset::path::AssetPath<'static>,
        _asset: SavedAsset<'_, Self::Asset>,
        _settings: &Self::Settings,
    ) -> Result<Self::LoaderSettings, AssetSaveError> {
        Ok(())
    }
}

type TextProcessor =
    LoadTransformAndSave<TextLoader, IdentityTransformer<TextAsset>, TextSaver, TextLoader>;

/// Registers the default source as two in-memory trees: what the app reads and writes, and what the
/// importer writes for it to read.
struct MemorySourcePlugin {
    source: Dir,
    processed: Dir,
}

impl Plugin for MemorySourcePlugin {
    fn build(&mut self, app: &mut App) {
        let reader = self.source.clone();
        let writer = self.source.clone();
        let processed_reader = self.processed.clone();
        let processed_writer = self.processed.clone();

        app.register_asset_source(
            AssetSourceId::Default,
            AssetSourceBuilder::new(move || {
                Box::new(MemoryAssetReader {
                    root: reader.clone(),
                }) as Box<dyn ErasedAssetReader>
            })
            .with_writer(move || {
                Some(Box::new(MemoryAssetWriter {
                    root: writer.clone(),
                }) as Box<dyn ErasedAssetWriter>)
            })
            .with_processed_reader(move || {
                Box::new(MemoryAssetReader {
                    root: processed_reader.clone(),
                }) as Box<dyn ErasedAssetReader>
            })
            .with_processed_writer(move || {
                Some(Box::new(MemoryAssetWriter {
                    root: processed_writer.clone(),
                }) as Box<dyn ErasedAssetWriter>)
            }),
        );
    }

    fn apply(&mut self, _app: &mut App) {}
}

/// Registers the asset type, its loader and its processor, from a plugin ordered after
/// `AssetPlugin` — which is the only place the server and the importer both exist.
struct RegisterTextPlugin;

impl Plugin for RegisterTextPlugin {
    fn build(&mut self, app: &mut App) {
        AssetPlugin::apply_before::<Self>(app);
    }

    fn apply(&mut self, app: &mut App) {
        app.init_asset::<TextAsset>()
            .register_asset_loader(TextLoader)
            .register_asset_processor(TextProcessor::from(TextSaver));

        // The source has no `.meta`, so the importer picks the processor by the file extension.
        // With the importer turned off this is reported and ignored, and the test below only reads.
        app.register_extension::<TextProcessor>("txt");
    }
}

/// A loader built from the world, which is what `init_asset_loader` is for.
#[derive(TypePath)]
struct ConfiguredLoader {
    marker: String,
}

impl FromWorld for ConfiguredLoader {
    fn from_world(_world: &zlim_core::world::World) -> Self {
        Self {
            marker: "from the world".to_owned(),
        }
    }
}

impl AssetLoader for ConfiguredLoader {
    type Asset = TextAsset;
    type Settings = ();

    const EXTENSIONS: &[&'static str] = &["configured"];

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _context: &mut LoadContext<'_>,
        _settings: &Self::Settings,
    ) -> Result<Self::Asset, AssetLoadError> {
        let mut bytes = Vec::new();
        reader
            .read_all_bytes(&mut bytes)
            .await
            .map_err(|error| AssetLoadError::from(error.to_string()))?;

        Ok(TextAsset(format!(
            "{}: {}",
            self.marker,
            String::from_utf8_lossy(&bytes)
        )))
    }
}

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
        // A yield, not a sleep: the importer and the loads run on the IO task pool.
        std::thread::yield_now();
    }

    panic!("waited {MAX_FRAMES} frames for {what}, but it never happened");
}

/// `Processed` mode has to *build and start* the importer: nothing else writes the processed side
/// the app reads, so without it the very first load would wait forever.
#[test]
fn processed_mode_builds_and_starts_the_importer() {
    let source = Dir::new(PathBuf::new());
    let processed = Dir::new(PathBuf::new());
    source.insert_asset_text(Path::new("a.txt"), "hello");

    let mut app = App::new();
    app.add_plugins((
        MemorySourcePlugin {
            source: source.clone(),
            processed: processed.clone(),
        },
        AssetPlugin {
            server_mode: AssetServerMode::Processed,
            watch_for_changes_override: Some(false),
            ..AssetPlugin::default()
        },
        RegisterTextPlugin,
    ));
    app.build();

    assert!(
        app.main_world().contains_resource::<AssetProcessServer>(),
        "`AssetPlugin` should insert the importer in `Processed` mode",
    );

    let server = app.main_world().resource::<AssetServer>().clone();
    let handle = server.load::<TextAsset>("a.txt");

    // The load reads the processed side, which the importer writes during `Startup`: the app's own
    // read is what proves the importer was started and finished.
    drive_until(&mut app, "the imported asset to load", || {
        server.is_loaded(handle.id())
    });

    assert_eq!(
        app.main_world()
            .resource::<Assets<TextAsset>>()
            .get(&handle)
            .expect("the loaded asset should be stored")
            .0,
        "hello",
    );
    assert!(
        processed.get_asset(Path::new("a.txt")).is_some(),
        "the importer should have written the processed bytes",
    );
    assert!(
        processed.get_meta(Path::new("a.txt")).is_some(),
        "the importer should have written the processed `.meta`",
    );
}

/// With the importer turned off, `Processed` mode only *reads* the processed side: an app whose
/// processed files come from somewhere else (an external tool) still works.
#[test]
fn processed_mode_without_the_importer_only_reads() {
    let source = Dir::new(PathBuf::new());
    let processed = Dir::new(PathBuf::new());

    // What an external importer would have written: the bytes plus the `.meta` naming the loader.
    processed.insert_asset_text(Path::new("a.txt"), "hello");
    let meta = zlim_asset::meta::AssetMeta::<(), ()>::new(zlim_asset::meta::AssetConfig::Load {
        loader: <TextLoader as TypePath>::type_path().into(),
        settings: (),
    });
    processed.insert_meta(Path::new("a.txt"), meta.serialize());

    let mut app = App::new();
    app.add_plugins((
        MemorySourcePlugin { source, processed },
        AssetPlugin {
            server_mode: AssetServerMode::Processed,
            watch_for_changes_override: Some(false),
            use_asset_processor_override: Some(false),
            ..AssetPlugin::default()
        },
        RegisterTextPlugin,
    ));
    app.build();

    assert!(
        !app.main_world().contains_resource::<AssetProcessServer>(),
        "the importer should not be built when it is turned off",
    );

    let server = app.main_world().resource::<AssetServer>().clone();
    let handle = server.load::<TextAsset>("a.txt");

    drive_until(&mut app, "the processed asset to load", || {
        server.is_loaded(handle.id())
    });

    assert_eq!(
        app.main_world()
            .resource::<Assets<TextAsset>>()
            .get(&handle)
            .expect("the loaded asset should be stored")
            .0,
        "hello",
    );
}
/// A loader that is only *pre-registered* must block the assets that resolve to it instead of
/// failing them, so a load started before its loader is registered still completes.
#[test]
fn a_load_waits_for_a_preregistered_loader() {
    let source = Dir::new(PathBuf::new());
    let processed = Dir::new(PathBuf::new());
    source.insert_asset_text(Path::new("a.txt"), "hello");

    /// Claims the `.txt` extension without registering the loader that reads it.
    struct PreregisterTextPlugin;

    impl Plugin for PreregisterTextPlugin {
        fn build(&mut self, app: &mut App) {
            AssetPlugin::apply_before::<Self>(app);
        }

        fn apply(&mut self, app: &mut App) {
            app.init_asset::<TextAsset>()
                .preregister_asset_loader::<TextLoader>();
        }
    }

    let mut app = App::new();
    app.add_plugins((
        MemorySourcePlugin {
            source: source.clone(),
            processed,
        },
        AssetPlugin {
            watch_for_changes_override: Some(false),
            ..AssetPlugin::default()
        },
        PreregisterTextPlugin,
    ));
    app.build();

    let server = app.main_world().resource::<AssetServer>().clone();
    let handle = server.load::<TextAsset>("a.txt");

    // The load is waiting on the pre-registered loader, so no number of frames finishes it.
    for _ in 0..8 {
        app.update();
    }
    assert!(
        !server.is_loaded(handle.id()),
        "a load of a pre-registered loader should wait for it, not fail or finish",
    );
    let state = server.load_state(handle.id());
    assert!(
        state.is_loading(),
        "the load should still be in flight while it waits for its loader, not {state:?}",
    );

    // Registering the loader releases the wait.
    app.register_asset_loader(TextLoader);

    drive_until(&mut app, "the pre-registered loader to arrive", || {
        server.is_loaded(handle.id())
    });

    assert_eq!(
        app.main_world()
            .resource::<Assets<TextAsset>>()
            .get(&handle)
            .expect("the loaded asset should be stored")
            .0,
        "hello",
    );
}

/// `init_asset_loader` builds the loader from the world, and a `World` can load and add assets
/// through `DirectAssetAccessExt` without reaching for the server resource by hand.
#[test]
fn loaders_can_be_built_from_the_world_and_the_world_can_load() {
    use zlim_asset::plugin::WorldAssetExt;

    let source = Dir::new(PathBuf::new());
    let processed = Dir::new(PathBuf::new());
    source.insert_asset_text(Path::new("a.configured"), "hello");

    /// Registers the asset type with a loader that comes from the world.
    struct ConfiguredPlugin;

    impl Plugin for ConfiguredPlugin {
        fn build(&mut self, app: &mut App) {
            AssetPlugin::apply_before::<Self>(app);
        }

        fn apply(&mut self, app: &mut App) {
            app.init_asset::<TextAsset>()
                .init_asset_loader::<ConfiguredLoader>();
        }
    }

    let mut app = App::new();
    app.add_plugins((
        MemorySourcePlugin { source, processed },
        AssetPlugin {
            watch_for_changes_override: Some(false),
            ..AssetPlugin::default()
        },
        ConfiguredPlugin,
    ));
    app.build();

    // A `World` can start a load and add a value, without looking the server up itself.
    let server = app.main_world().resource::<AssetServer>().clone();
    let loaded = app.main_world().load_asset::<TextAsset>("a.configured");
    let added = app
        .main_world_mut()
        .add_asset(TextAsset("added".to_owned()));

    drive_until(&mut app, "the load to finish", || {
        server.is_loaded(loaded.id())
    });

    let assets = app.main_world().resource::<Assets<TextAsset>>();
    assert!(
        assets.get(&added).is_some(),
        "the value added through the world should be stored"
    );
    assert_eq!(
        assets
            .get(&loaded)
            .expect("the loaded asset should be stored")
            .0,
        "from the world: hello",
        "the loader was built from the world",
    );
}

/// `load_internal_asset!` with a builder that takes extra arguments: that arm only ever expands in a
/// test like this one, so without it a broken arm would go unnoticed.
#[test]
fn an_internal_asset_builder_can_take_extra_arguments() {
    /// Registers the asset type, which is where the value is stored.
    struct InitTextPlugin;

    impl Plugin for InitTextPlugin {
        fn build(&mut self, app: &mut App) {
            AssetPlugin::apply_before::<Self>(app);
        }

        fn apply(&mut self, app: &mut App) {
            app.init_asset::<TextAsset>();
        }
    }

    let mut app = App::new();
    app.add_plugins((
        MemorySourcePlugin {
            source: Dir::new(PathBuf::new()),
            processed: Dir::new(PathBuf::new()),
        },
        AssetPlugin {
            watch_for_changes_override: Some(false),
            ..AssetPlugin::default()
        },
        InitTextPlugin,
    ));
    app.build();

    const HANDLE: zlim_asset::handle::Handle<TextAsset> =
        zlim_asset::uuid_handle!("1347c9b7-c46a-48e7-b7b8-023a354b7cac");

    // The file is embedded with `include_str!`, resolved next to *this* file.
    zlim_asset::load_internal_asset!(
        app.main_world_mut(),
        HANDLE,
        "asset_plugin.rs",
        |data: &'static str, path: &str, marker: &str| {
            assert!(
                path.ends_with("asset_plugin.rs"),
                "the path is reported as {path}"
            );
            TextAsset(format!("{marker}: {} bytes", data.len()))
        },
        "marker"
    );

    let assets = app.main_world().resource::<Assets<TextAsset>>();
    let asset = assets
        .get(&HANDLE)
        .expect("the internal asset should have been built and stored");

    assert!(
        asset.0.starts_with("marker: ") && asset.0.ends_with(" bytes"),
        "the builder's extra argument reached it: {}",
        asset.0,
    );
}
