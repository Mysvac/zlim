#![expect(clippy::module_inception, reason = "For better structure.")]
use std::sync::Arc;

use zlim_app::{App, First, Last, MainSchedulePlugin, Plugin, PluginExt};
use zlim_app::{PreUpdate, Startup};

use crate::event::ErasedAssetLoadFailedEvent;
use crate::io::embedded::EmbeddedAssetRegistry;
use crate::loaded::{LoadedFolder, LoadedUntypedAsset};
use crate::processor::{AssetProcessServer, StartAssetProcessServer};
use crate::server::{AssetMetaCheckMode, AssetServer, AssetServerMode};
use crate::server::{ClearFinishedAssetTask, HandleAssetSaveCommands};
use crate::server::{HandleAssetSeverEvents, UnapprovedPathMode};
use crate::source::AssetSourceBuilders;
use crate::transaction::TransactionLogger;

use super::AppAssetExt;
use super::{DEFAULT_PROCESSED_FILE_PATH, DEFAULT_UNPROCESSED_FILE_PATH};

// -----------------------------------------------------------------------------
// AssetPlugin

/// Installs the asset pipeline into an [`App`].
///
/// # The two servers, and the three states
///
/// The asset system has up to **two** [`AssetServer`]s, and which ones exist depends on the mode:
///
/// - **`Unprocessed`** (the default): only the app's server, reading the sources themselves.
/// - **`Processed` with the importer** (the default for that mode): *both*. The importer is a
///   standalone importer that owns a server of its own, in [`AssetServerMode::Processed`], and
///   writes the processed side during `Startup`; the app's server reads what it wrote. They share
///   the sources and the loader registry — a loader registered with [`AppAssetExt`] is the one the
///   processors load with — but each keeps its own handles: an asset's id in the app's server has
///   nothing to do with the id it had while being imported.
/// - **`Processed` without the importer** ([`use_asset_processor_override`] is `Some(false)`): only
///   the app's server, reading a processed side some other tool wrote. Nothing here produces it,
///   and [`register_asset_processor`] has nowhere to register (it says so and ignores the call).
///
/// In the middle state the app's server watches both sides (a source change is what makes the
/// importer run again, and the processed side is what the app reads); without the importer it
/// watches the side it reads.
///
/// # What this plugin does
///
/// - builds the app's [`AssetServer`] over its sources: the default file-system source (reading
///   [`DEFAULT_UNPROCESSED_FILE_PATH`] and, in processed mode, writing/reading
///   [`DEFAULT_PROCESSED_FILE_PATH`]), the `embedded` source backed by [`EmbeddedAssetRegistry`],
///   and every source registered before this plugin ran;
/// - installs the jobs that drain what that server collects: [`HandleAssetSeverEvents`] in
///   `PreUpdate` (applies load results, reports failures, frees the metadata of released handles),
///   [`ClearFinishedAssetTask`] in `First` (forgets load tasks that have ended) and
///   [`HandleAssetSaveCommands`] in `Last` (runs the saves a frame queued);
/// - registers [`ErasedAssetLoadFailedEvent`], and the built-in asset types ([`LoadedFolder`],
///   [`LoadedUntypedAsset`], `()`) through [`init_asset`] — which is also what installs a type's
///   *own* jobs ([`HandleAssetEventsJob`], [`HandleAssetDropEventsJob`],
///   [`ClampAssetChangesTick`]);
/// - in [`AssetServerMode::Processed`] with the importer enabled, builds the [`AssetProcessServer`]
///   and schedules its `StartAssetProcessServer` job in `Startup`, so the import runs once every
///   plugin has registered its loaders and processors.
///
/// # Do not build the asset system by hand
///
/// **An [`AssetServer`] is only usable as part of the pipeline this plugin installs.** Everything
/// the server collects is drained by a job: the per-type [`AssetEvent`] queue and the queue of
/// dropped handles by [`HandleAssetEventsJob`] / [`HandleAssetDropEventsJob`] (installed by
/// [`init_asset`]), the server's load results, save commands and finished tasks by the three jobs
/// above. Inserting an [`AssetServer`] (or a subset of those jobs) yourself is **not** supported
/// and fails silently rather than loudly: nothing consumes those queues, so they grow with every
/// load, every released handle and every event — memory that keeps rising with no upper bound —
/// while loaded assets are never applied and waits never resolve.
///
/// The same reasoning is why the pieces have to be reached through this plugin's API rather than
/// assembled: sources through [`register_asset_source`] (which only works *before* this plugin
/// runs), asset types, loaders, savers and processors through [`AppAssetExt`], and any plugin that
/// needs those in its `apply` ordered after this one.
///
/// # Ordering
///
/// Plugins that are applied *after* this one can use [`AppAssetExt`] from their `apply`; plugins
/// without an ordering constraint may be applied in any order, so such a plugin has to declare
/// it with [`AssetPlugin::apply_before`] in its `build`.
///
/// # Fields
///
/// The fields select *which* of the states above is built and how its sources behave; each one is
/// documented where it is declared.
///
/// # Example
///
/// ```ignore
/// use zlim_asset::plugin::AssetPlugin;
///
/// let mut app = App::new();
/// app.add_plugins(AssetPlugin {
///     file_path: "my_assets".into(),
///     ..AssetPlugin::default()
/// });
/// app.build();
/// app.init_asset::<MyAsset>().register_asset_loader(MyLoader);
/// ```
///
/// [`AssetPlugin::apply_before`]: PluginExt::apply_before
/// [`AppAssetExt`]: super::AppAssetExt
/// [`init_asset`]: super::AppAssetExt::init_asset
/// [`register_asset_source`]: super::AppAssetExt::register_asset_source
/// [`register_asset_processor`]: super::AppAssetExt::register_asset_processor
/// [`use_asset_processor_override`]: AssetPlugin::use_asset_processor_override
/// [`AssetServer`]: crate::server::AssetServer
/// [`AssetServerMode::Processed`]: crate::server::AssetServerMode::Processed
/// [`AssetProcessServer`]: crate::processor::AssetProcessServer
/// [`HandleAssetSeverEvents`]: crate::server::HandleAssetSeverEvents
/// [`HandleAssetSaveCommands`]: crate::server::HandleAssetSaveCommands
/// [`ClearFinishedAssetTask`]: crate::server::ClearFinishedAssetTask
/// [`HandleAssetEventsJob`]: crate::assets::HandleAssetEventsJob
/// [`HandleAssetDropEventsJob`]: crate::assets::HandleAssetDropEventsJob
/// [`ClampAssetChangesTick`]: crate::change::ClampAssetChangesTick
/// [`AssetEvent`]: crate::event::AssetEvent
/// [`ErasedAssetLoadFailedEvent`]: crate::event::ErasedAssetLoadFailedEvent
/// [`EmbeddedAssetRegistry`]: crate::io::embedded::EmbeddedAssetRegistry
/// [`LoadedFolder`]: crate::loaded::LoadedFolder
/// [`LoadedUntypedAsset`]: crate::loaded::LoadedUntypedAsset
/// [`App`]: zlim_app::App
pub struct AssetPlugin {
    /// Where the default asset source keeps its *unprocessed* assets, relative to the asset root.
    ///
    /// These are the files a developer edits. They are read by the app's server in
    /// [`AssetServerMode::Unprocessed`], and by the importer's server in every mode that has one;
    /// a processed server without an importer never reads them.
    ///
    /// Defaults to [`DEFAULT_UNPROCESSED_FILE_PATH`].
    ///
    /// [`AssetServerMode::Unprocessed`]: crate::server::AssetServerMode::Unprocessed
    pub file_path: String,

    /// Where the same source keeps its *processed* assets, relative to the asset root.
    ///
    /// The importer writes them there, and a server in [`AssetServerMode::Processed`] reads them
    /// there. Apart from the importer's transaction log, whose base directory this is, nothing
    /// else touches this path.
    ///
    /// Defaults to [`DEFAULT_PROCESSED_FILE_PATH`].
    pub processed_file_path: String,

    /// Which side the app's server reads: the sources themselves, or the imported output.
    ///
    /// This is what picks the state: the two servers above exist because of it, together with
    /// [`use_asset_processor_override`].
    ///
    /// Defaults to [`AssetServerMode::Unprocessed`].
    ///
    /// [`use_asset_processor_override`]: AssetPlugin::use_asset_processor_override
    pub server_mode: AssetServerMode,

    /// Whether a load reads an asset's `.meta` sidecar, and how thoroughly.
    ///
    /// It is ignored whenever the importer is built — that server shares the importer's sources and
    /// has to read the `.meta` the importer wrote next to the processed bytes, so it is forced to
    /// [`Always`] — and likewise ignored by the importer's own server. It is what the *other* case
    /// uses: an app reading assets that no importer of this app writes.
    ///
    /// Defaults to [`Always`].
    ///
    /// [`Always`]: crate::server::AssetMetaCheckMode::Always
    pub meta_check_mode: AssetMetaCheckMode,

    /// How the app's server treats a path that escapes its source root (`../` and the like).
    ///
    /// Defaults to [`Deny`], which refuses such a path unless the caller asks for it explicitly.
    /// The importer's own server always uses the default.
    ///
    /// [`Deny`]: crate::server::UnapprovedPathMode::Deny
    pub unapproved_path_mode: UnapprovedPathMode,

    /// Forces change watching on or off, for the sides the current state watches.
    ///
    /// `None` (the default) follows the platform: watching is compiled in with the `watch`
    /// feature. Watching is what makes a changed source reload, and what makes the importer see a
    /// change at all; a source that has no watcher stays unwatched even when this says `true` (the
    /// default source warns about it).
    pub watch_for_changes_override: Option<bool>,

    /// Whether [`AssetServerMode::Processed`] also builds and starts the importer.
    ///
    /// `None` (the default) means "yes", which is the middle state described above: the processed
    /// side has to be written by someone, and the importer is that someone. `Some(false)` is the
    /// state for a processed side produced elsewhere (an external tool): the app only reads it, and
    /// [`register_asset_processor`] has nowhere to register.
    ///
    /// Only [`AssetServerMode::Processed`] can have an importer, so this is ignored otherwise.
    ///
    /// [`AssetServerMode::Processed`]: crate::server::AssetServerMode::Processed
    /// [`register_asset_processor`]: super::AppAssetExt::register_asset_processor
    pub use_asset_processor_override: Option<bool>,

    /// Where the importer's transaction log comes from, when it is built.
    ///
    /// The log is what tells an *interrupted* import run apart from a finished one: a run records
    /// the asset it starts and the asset it finishes, and an entry that has no end means the
    /// previous run was cut short, so the processed side cannot be trusted and everything is
    /// processed again.
    ///
    /// A factory function rather than a value, because it is called once per app and the importer
    /// takes ownership of what it returns; its `&str` argument is the **base directory** the log
    /// should live under (the plugin passes its [`processed_file_path`]); a logger that keeps no
    /// file is free to ignore it. `None` (the default) means no log at all: an interrupted run then
    /// looks exactly like a finished one, and the importer skips whatever the processed side claims
    /// is up to date.
    ///
    /// The built-in file-backed log is `FileTransactionLogger` which writes the file `log` inside
    /// that base directory:
    ///
    /// ```rust, ignore
    /// use zlim_asset::transaction::file::FileTransactionLogger;
    ///
    /// AssetPlugin {
    ///     transaction_logger: Some(|base| Box::new(FileTransactionLogger::new(base))),
    ///     ..Default::default()
    /// }
    /// ```
    ///
    /// [`processed_file_path`]: Self::processed_file_path
    pub transaction_logger: Option<fn(path: &str) -> Box<dyn TransactionLogger>>,
}

impl Default for AssetPlugin {
    fn default() -> Self {
        Self {
            file_path: DEFAULT_UNPROCESSED_FILE_PATH.to_owned(),
            processed_file_path: DEFAULT_PROCESSED_FILE_PATH.to_owned(),
            watch_for_changes_override: None,
            server_mode: AssetServerMode::default(),
            meta_check_mode: AssetMetaCheckMode::default(),
            unapproved_path_mode: UnapprovedPathMode::default(),
            use_asset_processor_override: None,
            transaction_logger: None,
        }
    }
}

impl AssetPlugin {
    /// Adds the default source to `builders`, with a processed side when the mode has one.
    ///
    /// The builders are then consumed by `build_sources`, either here (no importer) or by
    /// [`AssetProcessServer::build`], which is why this is a step of its own.
    fn init_default_source(&self, builders: &mut AssetSourceBuilders, processed: bool) {
        let processed = processed.then_some(self.processed_file_path.as_str());
        builders.init_default_source(&self.file_path, processed);
    }
}

impl Plugin for AssetPlugin {
    fn build(&mut self, app: &mut App) {
        MainSchedulePlugin::apply_before::<Self>(app);
    }

    fn apply(&mut self, app: &mut App) {
        MainSchedulePlugin::warn_if_unset(app, "AssetPlugin");

        // -------------------------------------------------------------
        // Override

        let watching_for_changes = self
            .watch_for_changes_override
            .unwrap_or(crate::cfg::watch!());

        let use_asset_processor = self
            .use_asset_processor_override
            .unwrap_or(self.server_mode == AssetServerMode::Processed);

        let enable_processor =
            use_asset_processor && self.server_mode == AssetServerMode::Processed;

        // -------------------------------------------------------------
        // Asset sources

        let embedded = EmbeddedAssetRegistry::default();
        let world = app.main_world_mut();

        let mut builders = world
            .remove_resource::<AssetSourceBuilders>()
            .unwrap_or_default();

        // -------------------------------------------------------------
        // Asset process server

        if enable_processor {
            self.init_default_source(&mut builders, true);
            embedded.register_source(&mut builders);

            let logger = self
                .transaction_logger
                .map(|f| f(&self.processed_file_path));

            let importer = AssetProcessServer::build(&mut builders, watching_for_changes, logger);

            // The app's server shares the importer's loaders: a loader registered through
            // `AppAssetExt` has to be the very one the importer's processors load with.
            world.insert_resource(AssetServer::new_with_loaders(
                importer.clone_sources(),
                importer.server().0.loaders.clone(),
                AssetServerMode::Processed,
                // The processed `.meta` is the importer's output and names the
                // loader that reads the bytes next to it, so it is always read.
                AssetMetaCheckMode::Always,
                self.unapproved_path_mode.clone(),
                watching_for_changes,
            ));

            world.insert_resource(importer);

            // The first run happens once every plugin has been applied (`Startup`), so the
            // processors and loaders registered in an `apply` are all in place by then.
            world
                .schedule_entry(Startup)
                .insert::<StartAssetProcessServer>(());
        };

        if !enable_processor {
            self.init_default_source(
                &mut builders,
                self.server_mode == AssetServerMode::Processed,
            );
            embedded.register_source(&mut builders);

            // Which side is watched follows from which side is read: unprocessed mode reads
            // the sources, processed mode reads what the import step wrote (so without an
            // importer, the processed side is the only one worth watching).
            let sources = match self.server_mode {
                // In `unprocessed` mode the *processed* side is not watched: `watch_processed`,
                // the second argument, must be `false`.
                // See `HandleAssetSeverEvents` hot-reload function for details, no one consumes the processed queue.
                AssetServerMode::Unprocessed => builders.build_sources(watching_for_changes, false),
                // In `processed & !enable_processor` mode the *source* side is not watched: `watch`,
                // the first argument, must be `false`.
                // See `HandleAssetSeverEvents` hot-reload function for details, no one consumes the normal queue.
                AssetServerMode::Processed => builders.build_sources(false, watching_for_changes),
            };

            world.insert_resource(AssetServer::new(
                Arc::new(sources),
                self.server_mode,
                self.meta_check_mode.clone(),
                self.unapproved_path_mode.clone(),
                watching_for_changes,
            ));
        }

        world.insert_resource(embedded);
        world.register_message::<ErasedAssetLoadFailedEvent>();
        world.insert_job::<HandleAssetSeverEvents>(PreUpdate, ());
        world.insert_job::<ClearFinishedAssetTask>(First, ());
        world.insert_job::<HandleAssetSaveCommands>(Last, ());

        app.init_asset::<LoadedFolder>()
            .init_asset::<LoadedUntypedAsset>()
            .init_asset::<()>();
    }
}

// -----------------------------------------------------------------------------
