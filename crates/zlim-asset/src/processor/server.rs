//! A standalone importer: [`AssetProcessServer`].
//!
//! The importer is a thing of its own, not a job of the running app (the same way it is in the
//! reference implementations): it owns an [`AssetServer`] configured for *processed* sources, walks
//! the source side, runs each asset's [`AssetProcessor`], and
//! reports progress through [`ProcessorState`] / [`ProcessStatus`].
//!
//! A pass runs **one task per asset, all of them in flight at once**. That is what lets a processor
//! read a *process dependency* through the processed-side gate: the read waits for the dependency's
//! status, and while it waits the task that produces that status keeps running. A tree that has
//! never been processed therefore goes through in a single pass, in dependency order, with no
//! ordering and no retries of its own.
//!
//! What it deliberately does not have: a *persistent* index. [`ProcessorAssetInfos`] is built from
//! the processed side at the start of every pass, which is what makes the "is it up to date" answer
//! exact without keeping state between runs.
//!
//! [`AssetProcessor`]: crate::processor::AssetProcessor
//! [`ProcessorAssetInfos`]: crate::processor::infos::ProcessorAssetInfos
//!
//! Only the public surface lives here. The phases it drives are next to this file: `scan.rs` (the scan
//! a run starts with), `pass.rs` (one pass: the queue and its supervisor), `step.rs` (one asset),
//! `watch.rs` (the source events), and their state in `state.rs`, `transaction.rs`, `processed.rs` and
//! `driver.rs`.

use core::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use zlim_core::derive::Resource;
use zlim_core::error::Error;
use zlim_path::TypePath;
use zlim_task::IoTaskPool;
use zlim_utils::ext::CachePadded;
use zlim_utils::mpmc;

use crate::error::*;
use crate::ident::AssetSourceId;
use crate::io::AssetReaderError;
use crate::path::AssetPath;
use crate::processor::infos::{Task, queued_task};
use crate::processor::pass::SupervisorMode;
use crate::processor::state::ProcessingState;
use crate::processor::transaction::{LogFactoryState, LogLock};
use crate::processor::{AssetProcessor, AssetProcessors, ErasedAssetProcessor};
use crate::server::AssetMetaCheckMode;
use crate::server::{AssetServer, AssetServerMode, UnapprovedPathMode};
use crate::source::{AssetSource, AssetSourceBuilders, AssetSources};
use crate::transaction::{TransactionLogger, ValidateLogError};

// -----------------------------------------------------------------------------
// ProcessorState

/// What the processor is doing right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessorState {
    /// The processor has been built but has not started.
    Initializing = 0,
    /// A run is in progress.
    Processing = 1,
    /// Every run so far has finished: every asset a run was given has a result, so nothing is queued
    /// and nothing is in flight. What each asset got — an output, a failure, no output — is its
    /// [`ProcessStatus`], not this state.
    Finished = 2,
}

// -----------------------------------------------------------------------------
// ProcessStatus

/// How one asset came out of a run.
///
/// The status carries no error: a run reports a failure by *logging* it, and the status only says
/// that it happened. The status is what waiters are handed, and it is kept for every asset the run
/// knows about, so a copy of the error in it would live as long as the asset does — and would be
/// cloned again for every broadcast. Whoever waits on an asset needs to know whether there is
/// something to read, not why there is not.
#[derive(Clone, Copy, Debug)]
pub enum ProcessStatus {
    /// The asset has a processed output: it was just written, or it was already up to date.
    Processed,
    /// The asset has no processed output. Either processing it failed — the run logs the failure —
    /// or nothing was processed on purpose: an `.meta` that says `Ignore`, or a path no processor
    /// can be picked for.
    Failed,
    /// The asset does not exist on the source side: it was never part of a run, or its source has
    /// been removed since.
    NonExistent,
}

impl ProcessStatus {
    /// Returns the status as a string, for log display.
    #[inline]
    pub fn status(&self) -> &'static str {
        match self {
            Self::Processed => "processed",
            Self::Failed => "failed",
            Self::NonExistent => "non-existent",
        }
    }
}

// -----------------------------------------------------------------------------
// InitializeError

/// Why the importer could not be initialized.
///
/// Initialization is the scan a run starts with: it reads every source and the processed side that
/// belongs to it. Nothing can be processed without it, so a failure here is reported to whoever
/// started the run instead of being logged and skipped.
#[derive(Error, Debug)]
pub enum InitializeError {
    /// The transaction log of the previous run could not be validated.
    ///
    /// Nothing in the importer builds this variant: a log that fails validation only makes a run
    /// distrust the processed side and process everything again, which is logged as a warning
    /// instead (see [`AssetProcessServer::run`]).
    #[error("Failed to validate asset log: {_0}")]
    ValidateLogError(ValidateLogError),
    /// The source side of a source could not be read.
    #[error(transparent)]
    FailedToReadSourcePaths(AssetReaderError),
    /// The processed side of a source could not be read.
    #[error(transparent)]
    FailedToReadDestinationPaths(AssetReaderError),
}

// -----------------------------------------------------------------------------
// SetTransactionLoggerFailed

/// An error when attempting to set the transaction logger.
#[derive(Error, Debug)]
pub enum SetTransactionLoggerFailed {
    /// The transaction log is already in use, so setting the logger does nothing.
    #[error("Transaction log is already in use so setting the logger does nothing")]
    AlreadyInUse,
}

// -----------------------------------------------------------------------------
// AssetProcessServer, and the data every handle of it shares

/// A standalone asset importer.
///
/// It owns its own [`AssetServer`], in [`AssetServerMode::Processed`]: the app's server *reads*
/// processed assets, this one *writes* them, and the two share the same [`AssetSources`].
///
/// [`AssetPlugin`] is what puts one of these into an app (in [`AssetServerMode::Processed`]) and
/// schedules `StartAssetProcessServer` to run it; see its docs for the whole wiring.
///
/// [`AssetSources`]: crate::source::AssetSources
/// [`AssetPlugin`]: crate::plugin::AssetPlugin
#[derive(TypePath, Resource)]
pub struct AssetProcessServer {
    pub(super) server: AssetServer,

    /// Everything a clone of this importer shares with it.
    pub(super) data: Arc<AssetProcessorData>,
}

/// The state every clone of an [`AssetProcessServer`] shares.
///
/// The importer itself is a handle onto this: the sources it reads, the index and the coarse state a
/// run goes through, the processors it can run, and the transaction log. It stays crate-internal:
/// what an app needs from it — waiting for a run, replacing the log factory — is on
/// [`AssetProcessServer`] itself.
pub(crate) struct AssetProcessorData {
    /// The sources the importer reads from and writes to.
    pub(crate) sources: Arc<AssetSources>,

    /// The state a run goes through, and the index of every asset it knows about.
    pub(crate) state: Arc<ProcessingState>,

    /// The transaction log of the current run: a `start`/`finish` pair around every asset that is
    /// processed, so an interrupted run can be told apart from a finished one.
    pub(crate) log: LogLock,

    /// The processors the importer can run. They live here, not on the [`AssetServer`]: loading
    /// never needs one, so an app's server has no business knowing about them.
    pub(crate) processors: CachePadded<RwLock<AssetProcessors>>,

    /// Whether the processed side may be trusted, i.e. whether an asset that looks up to date may be
    /// skipped. An interrupted previous run (or a log that cannot be read) clears it, so this run
    /// processes everything again.
    pub(crate) skip_up_to_date: AtomicBool,

    /// The factory the transaction log is opened from, and whether a run has claimed it.
    pub(crate) logger: Mutex<LogFactoryState>,
}

impl Clone for AssetProcessServer {
    fn clone(&self) -> Self {
        Self {
            server: self.server.clone(),
            data: self.data.clone(),
        }
    }
}

impl AssetProcessServer {
    /// Creates the importer around the sources `builders` produce.
    ///
    /// `watch_processed` decides whether the processed side is watched too (which is what makes a
    /// running app notice that the importer rewrote something). `transaction_logger` is the factory
    /// the transaction log is opened from, or [`None`] for a run without a log.
    ///
    /// The sources the importer was built over are not handed back with it:
    /// [`clone_sources`](Self::clone_sources) returns them, which is what an app's own
    /// [`AssetServer`] is built over.
    #[must_use]
    pub fn build(
        builders: &mut AssetSourceBuilders,
        watch_processed: bool,
        transaction_logger: Option<Box<dyn TransactionLogger>>,
    ) -> Self {
        let state = Arc::new(ProcessingState::new());

        let mut built = builders.build_sources(true, watch_processed);
        // Reading the processed side has to wait for the run that writes it.
        built.gate_on_processor(&state);

        let sources = Arc::new(built);

        let server = AssetServer::new(
            sources.clone(),
            AssetServerMode::Processed,
            AssetMetaCheckMode::Always,
            UnapprovedPathMode::default(),
            watch_processed,
        );

        let data = Arc::new(AssetProcessorData {
            sources,
            state,
            processors: CachePadded::new(RwLock::new(AssetProcessors::default())),
            skip_up_to_date: AtomicBool::new(true),
            log: Arc::new(async_lock::Mutex::new(None)),
            logger: Mutex::new(LogFactoryState::Pending(transaction_logger)),
        });

        Self { server, data }
    }

    /// The [`AssetServer`] the importer runs on.
    ///
    /// This is *not* the server the app loads with: it is configured for processed sources and has
    /// its own registry-independent view of them.
    #[inline]
    pub fn server(&self) -> &AssetServer {
        &self.server
    }

    /// The sources the importer reads from and writes to.
    #[inline]
    pub fn sources(&self) -> &AssetSources {
        &self.data.sources
    }

    /// The sources the importer was built over, as an owned handle.
    ///
    /// An app's own [`AssetServer`] reads and writes the same sources, so building it over a clone of
    /// these is what makes both sides see one tree.
    #[inline]
    pub fn clone_sources(&self) -> Arc<AssetSources> {
        self.data.sources.clone()
    }

    /// Returns the source with `id`.
    #[inline]
    #[doc(alias = "source")]
    pub fn get_source(
        &self,
        id: impl Into<AssetSourceId>,
    ) -> Result<&AssetSource, MissingAssetSource> {
        self.data.sources.get(id)
    }

    /// What the processor is doing right now.
    #[inline]
    pub fn state(&self) -> ProcessorState {
        self.data.state.state()
    }
}

// -----------------------------------------------------------------------------
// AssetProcessServer: the processor registry

impl AssetProcessServer {
    /// Registers `processor` with the importer.
    pub fn register_processor<P: AssetProcessor>(&self, processor: P) {
        self.write_processors().push(processor);
    }

    /// Makes `P` the default processor for `extension`.
    ///
    /// A processor names no extension list of its own (unlike a loader or a saver), so this is how
    /// a source file gets a processor without a `.meta` naming one. The extension is given without
    /// a leading dot and is matched case-insensitively.
    ///
    /// `P` has to be registered first (see [`register_processor`]); an extension registered for a
    /// processor that is not registered is reported and ignored.
    ///
    /// [`register_processor`]: Self::register_processor
    pub fn register_extension<P: AssetProcessor>(&self, extension: &str) {
        self.write_processors()
            .register_extension(extension, <P as TypePath>::type_path());
    }

    /// Returns the processor registered under the processor type path `type_path`.
    pub fn get_processor_by_path(&self, type_path: &str) -> Option<Arc<dyn ErasedAssetProcessor>> {
        self.read_processors().get_by_path(type_path)
    }

    /// Returns the processor registered under the processor type name `type_name`.
    ///
    /// The name is the lenient form: a fully-qualified type path resolves as well, so a string read
    /// out of a `.meta` file can be passed as it is. A short name that several processors share
    /// selects none of them, which is reported as an ambiguity rather than as a miss.
    ///
    /// # Errors
    ///
    /// - `Err(None)`: no processor has that name.
    /// - `Err(Some(_))`: several processors share it; the [`AmbiguousName`] lists their type paths.
    pub fn get_processor_by_name(
        &self,
        type_name: &str,
    ) -> Result<Arc<dyn ErasedAssetProcessor>, Option<AmbiguousName>> {
        self.read_processors().get_by_name(type_name)
    }

    /// Returns the processor that handles source files with `extension` by default.
    pub fn get_default_processor(&self, extension: &str) -> Option<Arc<dyn ErasedAssetProcessor>> {
        self.read_processors().get_by_extension(extension)
    }
}

// -----------------------------------------------------------------------------
// AssetProcessServer: running

impl AssetProcessServer {
    /// Processes the source asset at `path`, and records what came out of it.
    ///
    /// Which processor runs follows from the source's `.meta`
    /// (`AssetConfig::Process { processor }`) and, without one, from the source path's extension
    /// (see [`register_extension`](Self::register_extension)). An asset whose `.meta` says
    /// `Load` — or which has no processor but does have a loader — is **copied** to the processed
    /// side instead, so a processed source holds every asset the app can load; an `.meta` that says
    /// `Ignore` leaves the asset alone.
    ///
    /// This is the single-asset step of a pass. The asset is processed unless the index of the
    /// processed side says its output still matches its source *and* the processed side may be
    /// trusted — a run takes that trust from the previous run's transaction log, so only a log it
    /// cannot validate (an interrupted run) takes the trust away. A skip rewrites nothing and only
    /// records the status; a rewrite holds the asset's transaction lock for writing while it does. A
    /// processor that reads a *process dependency* goes through the processed-side gate, so what it
    /// waits for is that dependency's status — which is why a pass runs one task per asset.
    ///
    /// # Errors
    ///
    /// Fails when the path names a source this importer does not know (`MissingAssetSource`), when
    /// the source has no processed side, when the processor cannot be resolved or loaded, when no
    /// loader matches the asset, and when the processor (or the reader/writer) fails.
    pub async fn process_asset(
        &self,
        path: impl Into<AssetPath<'static>>,
    ) -> Result<(), AssetProcessError> {
        let path = path.into();

        self.register_paths(core::slice::from_ref(&path)).await;

        let result = self.process_asset_internal(&path).await;

        let ret = match &result {
            Ok(_) => Ok(()),
            Err(e) => Err(e.clone()),
        };

        // Nothing is requeued here: a single-asset call has no pass to feed. The status is still
        // recorded, so whoever waits on the asset is answered.
        self.data
            .state
            .write_infos()
            .await
            .finish_processing(path, result, None)
            .await;

        ret
    }

    /// Starts the importer's service: process what the sources hold now, then keep watching them.
    ///
    /// The run is handed to the IO task pool, so this returns as soon as it has been queued; a read
    /// of the processed side waits for it through the processed-side gate instead.
    ///
    /// The initial work is queued before the supervisor starts, so the first asset can already wait
    /// for a dependency that is further back in the queue. Source changes are only listened for once
    /// that first pass is over — a change that arrives earlier is part of the scan anyway.
    ///
    /// # Panics
    ///
    /// The run panics when a source cannot be read at all, because the scan would then process
    /// nothing. [`run`](Self::run) reports that failure as [`InitializeError`] instead of panicking.
    pub fn start(&self) {
        let this = self.clone();

        IoTaskPool::get()
            .spawn(async move {
                this.open_transaction_log().await;

                // The scan comes first, and it is what the state knows about before it announces
                // anything: a read that is already waiting has to find its asset *registered* rather
                // than be told it does not exist.
                //
                // A source that cannot be read at all is the one thing a run cannot work around: it
                // would process nothing, so it panics — the same way the reference implementation
                // unwraps the very same call.
                let paths = this.initialize().await.unwrap();

                let (tasks, receiver) = mpmc::channel::<Task>();
                for path in &paths {
                    let _ = tasks.send(queued_task(path));
                }

                let supervisor_tasks = tasks.clone();
                let supervisor = this.clone();
                IoTaskPool::get()
                    .spawn(async move {
                        supervisor
                            .execute_processing_tasks(
                                supervisor_tasks,
                                receiver,
                                SupervisorMode::LongRunning,
                            )
                            .await;
                    })
                    .detach();

                this.data.state.wait_until_finished().await;
                this.spawn_source_change_event_listeners(&tasks);
            })
            .detach();
    }

    /// Runs the importer once over every source, then returns.
    ///
    /// This is [`start`](Self::start) without the service part: the sources are scanned, everything
    /// that is out of date is processed (the tasks still run concurrently, so a processor can wait
    /// for a dependency), and the run is over when nothing is left.
    ///
    /// # Errors
    ///
    /// Fails when the sources cannot be read at all; see [`InitializeError`].
    pub async fn run(&self) -> Result<(), InitializeError> {
        let recoverable = self.open_transaction_log().await;
        let paths = self.initialize().await?;

        self.process_paths(paths, recoverable).await;

        Ok(())
    }

    /// Processes every source asset of `source_id` whose processed output is not up to date.
    ///
    /// This is one pass over a single source, like the one [`run`](Self::run) makes over all of them:
    /// every source is still scanned first, so a dependency that lives in another one is known. It
    /// does not open the transaction log, so it always trusts the processed side — an interrupted
    /// previous run is not noticed here, where [`run`](Self::run) would process everything again.
    ///
    /// # Errors
    ///
    /// Fails when the sources cannot be read at all; see [`InitializeError`].
    pub async fn process_source(&self, source_id: AssetSourceId) -> Result<(), InitializeError> {
        let paths: Vec<AssetPath<'static>> = self
            .initialize()
            .await?
            .into_iter()
            .filter(|path| path.source_id() == source_id)
            .collect();

        self.process_paths(paths, true).await;

        Ok(())
    }

    /// Processes every source this importer knows.
    ///
    /// # Errors
    ///
    /// Fails when the sources cannot be read at all; see [`InitializeError`].
    pub async fn process_all(&self) -> Result<(), InitializeError> {
        let sources: Vec<AssetSourceId> = self.data.sources.iter_id().collect();

        for source_id in sources {
            self.process_source(source_id).await?;
        }

        Ok(())
    }

    /// Writes the `.meta` an asset with none should have.
    ///
    /// The `.meta` names the processor that handles the asset — by its extension — with default
    /// settings, so the choice is recorded next to the asset instead of living in someone's head.
    /// When no processor handles that extension, the loader that reads the asset is named instead
    /// (see [`AssetServer::write_default_meta`]), which is what makes such an asset reach the
    /// processed side.
    ///
    /// The `.meta` goes next to the *source*: an existing one is kept when `overwrite` is `false`, and
    /// replaced when it is `true`.
    ///
    /// # Errors
    ///
    /// Fails when the path names a source this importer does not know (`MissingAssetSource`), when
    /// neither a processor nor a loader handles the asset, when the source has no writer, when
    /// `overwrite` is `false` and there is already something there to overwrite
    /// (`MetaAlreadyExists`), and when the existing path cannot be checked or the new `.meta` cannot
    /// be written.
    pub async fn write_default_meta<'b>(
        &self,
        path: impl Into<AssetPath<'b>>,
        overwrite: bool,
    ) -> Result<(), AssetMetaWriteError> {
        let path = path.into();
        self.write_default_meta_internal(path, overwrite).await
    }
}

// -----------------------------------------------------------------------------
// AssetProcessServer: progress, and the transaction log

impl AssetProcessServer {
    /// Replaces the [`TransactionLog`] factory the transaction log is opened from, before processing
    /// starts.
    ///
    /// # Errors
    ///
    /// [`SetTransactionLoggerFailed::AlreadyInUse`] once a run has claimed a factory: from then
    /// on, writing that run's log is the importer's business.
    ///
    /// [`TransactionLog`]: crate::transaction::TransactionLog
    #[inline]
    pub fn set_transaction_logger(
        &self,
        factory: Box<dyn TransactionLogger>,
    ) -> Result<(), SetTransactionLoggerFailed> {
        let mut guard = self
            .data
            .logger
            .lock()
            .unwrap_or_else(PoisonError::into_inner);

        let LogFactoryState::Pending(pending) = &mut *guard else {
            ::core::hint::cold_path();
            return Err(SetTransactionLoggerFailed::AlreadyInUse);
        };

        *pending = Some(factory);

        Ok(())
    }

    /// Waits until the importer has been initialized: the sources have been scanned and the index
    /// describes the processed side.
    pub async fn wait_until_initialized(&self) {
        self.data.state.wait_until_initialized().await;
    }

    /// Waits until every run so far has finished.
    pub async fn wait_until_finished(&self) {
        self.data.state.wait_until_finished().await;
    }

    /// Waits for the result of processing `path`.
    ///
    /// Nothing can be answered before the sources have been scanned, so a call made before the first
    /// run waits for it. Returns [`ProcessStatus::NonExistent`] when the path is not a source asset
    /// of any source this processor knows.
    pub async fn wait_until_processed(&self, path: impl Into<AssetPath<'static>>) -> ProcessStatus {
        self.data.state.wait_until_processed(path.into()).await
    }
}
