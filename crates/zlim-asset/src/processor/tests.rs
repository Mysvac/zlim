//! Unit tests for the processor registry, the `LoadTransformAndSave` pipeline and the importer's
//! scan, service and processed-side gate.
//!
//! They live in the crate (not in `tests/`) because they build a `ProcessContext` through its
//! `pub(crate)` constructor and drive the processor registry on the importer.

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;

use futures_lite::AsyncWriteExt;
use zlim_path::derive::TypePath;
use zlim_utils::mpmc::Sender;

// `block_on` here is `zlim_task`'s rather than `futures_lite`'s: the importer hands its work to the
// IO task pool, and in single-threaded mode a spawned task only runs while that pool's executor is
// driven — which is what this one does (it is a drop-in for the multi-threaded one).
use zlim_task::{IoTaskPool, block_on};

use super::*;
use crate::asset::{Asset, VisitAssetDependencies};
use crate::error::AssetMetaWriteError;
use crate::error::{AssetLoadError, AssetSaveError, AssetTransformError};
use crate::event::AssetSourceEvent;
use crate::ident::{AssetSourceId, ErasedAssetId};
use crate::io::memory::{Dir, MemoryAssetReader, MemoryAssetWriter};
use crate::io::watcher::AssetWatcher;
use crate::io::{ErasedAssetReader, ErasedAssetWriter, Reader, Writer};
use crate::loader::{AssetLoader, LoadContext};
use crate::meta::{AssetConfig, AssetMeta, ProcessedInfo};
use crate::path::AssetPath;
use crate::processor::{AssetProcessServer, ProcessStatus, ProcessorState};
use crate::saver::{AssetSaver, SavedAsset};
use crate::server::{AssetMetaCheckMode, AssetServer, AssetServerMode, UnapprovedPathMode};
use crate::source::{AssetSourceBuilder, AssetSourceBuilders};
use crate::transaction::{LogEntry, TransactionError, TransactionLog, TransactionLogger};
use crate::transformer::{AssetTransformer, TransformedAsset};
use crate::utils::BoxedFuture;

#[derive(TypePath)]
struct SrcAsset(String);

impl VisitAssetDependencies for SrcAsset {
    fn visit_dependencies(&self, _visit: &mut dyn FnMut(ErasedAssetId)) {}
}

impl Asset for SrcAsset {}

#[derive(TypePath)]
struct DstAsset(String);

impl VisitAssetDependencies for DstAsset {
    fn visit_dependencies(&self, _visit: &mut dyn FnMut(ErasedAssetId)) {}
}

impl Asset for DstAsset {}

#[derive(TypePath)]
struct SrcLoader;

impl AssetLoader for SrcLoader {
    type Asset = SrcAsset;
    type Settings = ();

    const EXTENSIONS: &[&'static str] = &["src"];

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

        Ok(SrcAsset(String::from_utf8_lossy(&bytes).into_owned()))
    }
}

#[derive(TypePath)]
struct DstLoader;

impl AssetLoader for DstLoader {
    type Asset = DstAsset;
    type Settings = ();

    const EXTENSIONS: &[&'static str] = &["dst"];

    async fn load(
        &self,
        _reader: &mut dyn Reader,
        _context: &mut LoadContext<'_>,
        _settings: &Self::Settings,
    ) -> Result<Self::Asset, AssetLoadError> {
        unreachable!("the processed side is only written in this test")
    }
}

/// Appends `!` to the loaded source text, so a test can tell a transformed output from a copied one.
#[derive(TypePath)]
struct AppendTransformer;

impl AssetTransformer for AppendTransformer {
    type AssetInput = SrcAsset;
    type AssetOutput = DstAsset;
    type Settings = ();

    async fn transform(
        &self,
        asset: TransformedAsset<Self::AssetInput>,
        _settings: &Self::Settings,
    ) -> Result<TransformedAsset<Self::AssetOutput>, AssetTransformError> {
        Ok(TransformedAsset::new(DstAsset(format!(
            "{}!",
            asset.get().0
        ))))
    }
}

#[derive(TypePath)]
struct DstSaver;

impl AssetSaver for DstSaver {
    type Asset = DstAsset;
    type Settings = ();
    type LoaderSettings = ();

    const EXTENSIONS: &[&'static str] = &["dst"];

    async fn save(
        &self,
        writer: &mut dyn Writer,
        _path: &AssetPath<'static>,
        asset: SavedAsset<'_, Self::Asset>,
        _settings: &Self::Settings,
    ) -> Result<(), AssetSaveError> {
        writer
            .write_all_bytes(asset.get().0.as_bytes())
            .await
            .map_err(|error| AssetSaveError::from(error.to_string()))
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

type TestProcessor = LoadTransformAndSave<SrcLoader, AppendTransformer, DstSaver, DstLoader>;

/// An importer whose source side and processed side are two different in-memory trees, which
/// is what a scan needs to be able to tell apart.
fn process_server_with(source: &Dir, processed: &Dir) -> AssetProcessServer {
    let reader_dir = source.clone();
    let writer_dir = source.clone();
    let processed_reader_dir = processed.clone();
    let processed_writer_dir = processed.clone();

    let mut builders = AssetSourceBuilders::default();
    builders.insert(
        AssetSourceId::Default,
        AssetSourceBuilder::new(move || {
            Box::new(MemoryAssetReader {
                root: reader_dir.clone(),
            }) as Box<dyn ErasedAssetReader>
        })
        .with_writer(move || {
            Some(Box::new(MemoryAssetWriter {
                root: writer_dir.clone(),
            }) as Box<dyn ErasedAssetWriter>)
        })
        .with_processed_reader(move || {
            Box::new(MemoryAssetReader {
                root: processed_reader_dir.clone(),
            }) as Box<dyn ErasedAssetReader>
        })
        .with_processed_writer(move || {
            Some(Box::new(MemoryAssetWriter {
                root: processed_writer_dir.clone(),
            }) as Box<dyn ErasedAssetWriter>)
        }),
    );

    AssetProcessServer::build(&mut builders, false, None)
}

/// A source loader that counts how often it ran, for the "was it skipped" assertions.
#[derive(TypePath)]
struct CountingSrcLoader {
    runs: Arc<AtomicUsize>,
}

impl AssetLoader for CountingSrcLoader {
    type Asset = SrcAsset;
    type Settings = ();

    const EXTENSIONS: &[&'static str] = &["cnt"];

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _context: &mut LoadContext<'_>,
        _settings: &Self::Settings,
    ) -> Result<Self::Asset, AssetLoadError> {
        self.runs.fetch_add(1, Ordering::SeqCst);

        let mut bytes = Vec::new();
        reader
            .read_all_bytes(&mut bytes)
            .await
            .map_err(|error| AssetLoadError::from(error.to_string()))?;

        Ok(SrcAsset(String::from_utf8_lossy(&bytes).into_owned()))
    }
}

/// The registry's three look-ups and the full `find` chain: a processor is reachable by its type
/// path and by its type name as soon as it is pushed, while the extension only resolves once it
/// has been registered as that extension's default.
///
/// `find` is the lenient entry point — its name step takes a fully-qualified path too — but a
/// name nothing owns is a miss rather than a fall-through to the default, which the last
/// assertion pins.
#[test]
fn a_processor_is_found_by_its_default_extension_and_by_its_type_path() {
    let mut processors = AssetProcessors::default();

    processors.push(TestProcessor::new(AppendTransformer, DstSaver));

    let type_path = <TestProcessor as zlim_path::TypePath>::type_path();

    // Registered but not a default: the type path and the type name find it, while the
    // extension alone does not.
    assert_eq!(
        processors
            .get_by_path(type_path)
            .expect("the processor should be found by its type path")
            .type_path(),
        type_path
    );
    assert_eq!(
        processors
            .get_by_name(<TestProcessor as zlim_path::TypePath>::type_name())
            .expect("the processor should be found by its type name")
            .type_path(),
        type_path
    );

    // The name look-up is the lenient form: a fully-qualified type path resolves through it too.
    assert_eq!(
        processors
            .get_by_name(type_path)
            .expect("a type path resolves through the name look-up")
            .type_path(),
        type_path
    );

    assert!(processors.get_by_extension("src").is_none());

    processors.register_extension("src", type_path);

    let by_extension = processors
        .get_by_extension("src")
        .expect("the default processor for `.src` should be found");
    let by_path = processors
        .get_by_path(type_path)
        .expect("the processor should be found by its type path");

    assert_eq!(by_extension.type_path(), by_path.type_path());

    // The full look-up also reaches the default through the source path's extension.
    let path = AssetPath::parse("thing.src");
    assert_eq!(
        processors
            .find(None, None, Some(&path))
            .expect("the default processor for `.src` should be found")
            .type_path(),
        type_path
    );

    // The name step of `find` is lenient as well, and — unlike the extension step — it is
    // authoritative: a name nothing has does not fall through to the extension's default.
    assert_eq!(
        processors
            .find(None, Some(type_path), Some(&path))
            .expect("the name step takes a type path as well")
            .type_path(),
        type_path
    );
    assert!(
        processors
            .find(None, Some("NoSuchProcessor"), Some(&path))
            .is_err(),
        "a name nothing has is a miss, not a fall-through to the default for `.src`",
    );
}

/// A single processor run driven by hand rather than by the importer: the source bytes are read
/// through the source's reader, the `ProcessContext` is built around them, and the run writes its
/// output — `hello!`, the source text with the transformer's suffix — to the processed side.
///
/// The `.meta` the run returns is checked as well: it is a load config naming the destination
/// loader, which is what makes the written file loadable again.
#[test]
fn the_pipeline_loads_transforms_and_saves() {
    let dir = Dir::new(PathBuf::new());
    let process_server = process_server_with(&dir, &dir);
    let server = process_server.server();

    server.register_loader(SrcLoader);
    server.register_loader(DstLoader);
    server.register_saver(DstSaver);
    process_server.register_processor(TestProcessor::new(AppendTransformer, DstSaver));

    block_on(server.save_bytes("thing.src", b"hello")).expect("the source should be written");

    let path = AssetPath::parse("thing.src").into_owned();

    let processor = process_server
        .get_processor_by_path(<TestProcessor as zlim_path::TypePath>::type_path())
        .expect("the processor is registered");

    // The driver would hand the processor a reader of its own; the context owns it.
    let source = server.get_source(AssetSourceId::Default).expect("source");
    let reader = block_on(source.reader().read(path.path())).expect("source bytes");

    let mut processed_info = ProcessedInfo {
        hash: crate::meta::AssetHash::ZERO,
        full_hash: crate::meta::AssetHash::ZERO,
        process_dependencies: Vec::new(),
    };

    let writer = MemoryAssetWriter { root: dir.clone() };
    let mut writer = block_on(writer.write(Path::new("thing.dst"))).expect("processed writer");

    let settings = <TestProcessor as AssetProcessor>::Settings::default();
    let context = ProcessContext::new(server, &path, reader, &mut processed_info);

    let meta = block_on(processor.process(&mut *writer, context, &settings))
        .expect("the pipeline should process the asset");

    block_on(writer.flush()).expect("flush");

    assert_eq!(
        dir.get_asset(Path::new("thing.dst"))
            .expect("processed bytes")
            .value(),
        b"hello!"
    );

    let meta = AssetMeta::<(), ()>::deserialize(&meta.serialize()).expect("meta");
    match meta.asset_config {
        AssetConfig::Load { loader, .. } => {
            assert_eq!(loader, <DstLoader as zlim_path::TypePath>::type_path());
        }
        _ => panic!("the processed output is a load config naming its loader"),
    }
}

/// The importer driven from the outside: one `process_asset` call takes the bytes on the source
/// side, runs the pipeline, and leaves the transformed output — plus a meta naming the loader that
/// reads it — on the processed side, which is a separate tree here.
///
/// The recorded hashes are checked too: a run with no process dependencies folds nothing, so the
/// full hash is simply the content hash.
#[test]
fn process_asset_writes_the_processed_side() {
    let source_dir = Dir::new(PathBuf::new());
    let processed_dir = Dir::new(PathBuf::new());

    let process_server = process_server_with(&source_dir, &processed_dir);

    process_server.server().register_loader(SrcLoader);
    process_server.server().register_saver(DstSaver);
    process_server.register_processor(TestProcessor::new(AppendTransformer, DstSaver));
    process_server.register_extension::<TestProcessor>("src");

    block_on(process_server.server().save_bytes("thing.src", b"hello"))
        .expect("the source should be written");

    block_on(process_server.process_asset("thing.src")).expect("processing should succeed");

    assert_eq!(
        processed_dir
            .get_asset(Path::new("thing.src"))
            .expect("processed bytes")
            .value(),
        b"hello!"
    );

    let meta_bytes = processed_dir
        .get_meta(Path::new("thing.src"))
        .expect("processed meta")
        .value()
        .to_vec();

    let meta = AssetMeta::<(), ()>::deserialize(&meta_bytes).expect("meta");

    match meta.asset_config {
        AssetConfig::Load { loader, .. } => {
            assert_eq!(loader, <DstLoader as zlim_path::TypePath>::type_path());
        }
        _ => panic!("the processed output is a load config naming its loader"),
    }

    let info = meta.processed_info.expect("the run recorded its hashes");
    assert_ne!(info.hash, crate::meta::AssetHash::ZERO);
    assert_eq!(
        info.full_hash, info.hash,
        "no process dependencies means `full_hash` is `hash`"
    );
}

/// One scan decides per asset: it processes while nothing has been processed yet, skips the loader
/// entirely while the recorded hash still matches, and processes again after the source changes.
#[test]
fn a_scan_processes_what_is_missing_or_out_of_date() {
    type CountingProcessor =
        LoadTransformAndSave<CountingSrcLoader, AppendTransformer, DstSaver, DstLoader>;

    let source = Dir::new(PathBuf::new());
    let processed = Dir::new(PathBuf::new());

    let runs = Arc::new(AtomicUsize::new(0));

    let process_server = process_server_with(&source, &processed);
    process_server
        .server()
        .register_loader(CountingSrcLoader { runs: runs.clone() });
    process_server.server().register_saver(DstSaver);
    process_server.register_processor(CountingProcessor::new(AppendTransformer, DstSaver));
    process_server.register_extension::<CountingProcessor>("cnt");

    block_on(process_server.server().save_bytes("thing.cnt", b"one")).expect("source");

    // First scan: nothing is processed yet, so the asset is processed.
    block_on(process_server.process_source(AssetSourceId::Default)).expect("scan");
    assert_eq!(runs.load(Ordering::SeqCst), 1);
    assert_eq!(
        processed
            .get_asset(Path::new("thing.cnt"))
            .expect("processed bytes")
            .value(),
        b"one!"
    );

    // Second scan, nothing changed: the loader must not run again.
    block_on(process_server.process_source(AssetSourceId::Default)).expect("scan");
    assert_eq!(
        runs.load(Ordering::SeqCst),
        1,
        "up-to-date asset was skipped"
    );

    // The source changed: the scan notices (the recorded hash no longer matches) and
    // processes it again.
    block_on(process_server.server().save_bytes("thing.cnt", b"two")).expect("source");
    block_on(process_server.process_source(AssetSourceId::Default)).expect("scan");
    assert_eq!(
        runs.load(Ordering::SeqCst),
        2,
        "changed asset was processed again"
    );
    assert_eq!(
        processed
            .get_asset(Path::new("thing.cnt"))
            .expect("processed bytes")
            .value(),
        b"two!"
    );
}
/// Reads `dependency` through the [`LoadContext`], which is what makes it a *process*
/// dependency: the dependency must already have a processed output, and that output's
/// `full_hash` is folded into the reader's own.
async fn read_dependency(
    context: &mut LoadContext<'_>,
    dependency: &'static str,
) -> Result<SrcAsset, AssetLoadError> {
    let bytes = context
        .read_asset_bytes(dependency)
        .await
        .map_err(|error| AssetLoadError::from(error.to_string()))?;

    Ok(SrcAsset(String::from_utf8_lossy(&bytes).into_owned()))
}

/// Reads `thing.cnt`, so its `full_hash` is folded from the leaf's.
#[derive(TypePath)]
struct MidLoader {
    runs: Arc<AtomicUsize>,
}

impl AssetLoader for MidLoader {
    type Asset = SrcAsset;
    type Settings = ();

    const EXTENSIONS: &[&'static str] = &["mid"];

    async fn load(
        &self,
        _reader: &mut dyn Reader,
        context: &mut LoadContext<'_>,
        _settings: &Self::Settings,
    ) -> Result<Self::Asset, AssetLoadError> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        read_dependency(context, "thing.cnt").await
    }
}

/// Reads `mid.mid`, one level further down the chain.
#[derive(TypePath)]
struct DepLoader {
    runs: Arc<AtomicUsize>,
}

impl AssetLoader for DepLoader {
    type Asset = SrcAsset;
    type Settings = ();

    const EXTENSIONS: &[&'static str] = &["dep"];

    async fn load(
        &self,
        _reader: &mut dyn Reader,
        context: &mut LoadContext<'_>,
        _settings: &Self::Settings,
    ) -> Result<Self::Asset, AssetLoadError> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        read_dependency(context, "mid.mid").await
    }
}

/// How often each loader of the chain has run.
fn runs(plain: &AtomicUsize, mid: &AtomicUsize, dep: &AtomicUsize) -> (usize, usize, usize) {
    (
        plain.load(Ordering::SeqCst),
        mid.load(Ordering::SeqCst),
        dep.load(Ordering::SeqCst),
    )
}

/// The paths recorded as process dependencies in the processed `.meta` of `path`.
fn dependencies_of(processed: &Dir, path: &str) -> Vec<String> {
    let meta_bytes = processed
        .get_meta(Path::new(path))
        .expect("the processed meta")
        .value()
        .to_vec();

    let meta = AssetMeta::<(), ()>::deserialize(&meta_bytes).expect("meta");

    let mut dependencies: Vec<String> = meta
        .processed_info
        .expect("the run recorded its hashes")
        .process_dependencies
        .iter()
        .map(|dependency| dependency.path.to_string())
        .collect();

    dependencies.sort();
    dependencies
}

/// The bytes of the processed output of `path`.
fn processed_bytes(processed: &Dir, path: &str) -> Vec<u8> {
    processed
        .get_asset(Path::new(path))
        .expect("the processed bytes")
        .value()
        .to_vec()
}

/// A three-level chain where every level reads the processed output of the one below it and folds
/// that output's full hash into its own. One pass over a fresh tree has to sort the levels out by
/// itself: the tasks wait on the gate instead of being ordered or retried by hand.
///
/// After that the same pass must be a no-op while nothing changed, and it must reach the top when
/// only the leaf changes — which is the point of recording the dependencies at all. The loaders
/// count their runs, so "was it skipped" is observable rather than implied.
#[test]
fn a_change_carries_through_a_chain_of_process_dependencies() {
    type CountingProcessor =
        LoadTransformAndSave<CountingSrcLoader, AppendTransformer, DstSaver, DstLoader>;
    type MidProcessor = LoadTransformAndSave<MidLoader, AppendTransformer, DstSaver, DstLoader>;
    type DepProcessor = LoadTransformAndSave<DepLoader, AppendTransformer, DstSaver, DstLoader>;

    let source = Dir::new(PathBuf::new());
    let processed = Dir::new(PathBuf::new());

    let plain_runs = Arc::new(AtomicUsize::new(0));
    let mid_runs = Arc::new(AtomicUsize::new(0));
    let dep_runs = Arc::new(AtomicUsize::new(0));

    let process_server = process_server_with(&source, &processed);

    process_server.server().register_loader(CountingSrcLoader {
        runs: plain_runs.clone(),
    });
    process_server.server().register_loader(MidLoader {
        runs: mid_runs.clone(),
    });
    process_server.server().register_loader(DepLoader {
        runs: dep_runs.clone(),
    });
    process_server.server().register_saver(DstSaver);

    process_server.register_processor(CountingProcessor::new(AppendTransformer, DstSaver));
    process_server.register_processor(MidProcessor::new(AppendTransformer, DstSaver));
    process_server.register_processor(DepProcessor::new(AppendTransformer, DstSaver));

    process_server.register_extension::<CountingProcessor>("cnt");
    process_server.register_extension::<MidProcessor>("mid");
    process_server.register_extension::<DepProcessor>("dep");

    block_on(process_server.server().save_bytes("thing.cnt", b"one")).expect("source");
    block_on(process_server.server().save_bytes("mid.mid", b"two")).expect("source");
    block_on(process_server.server().save_bytes("dep.dep", b"three")).expect("source");

    // One pass over a tree that has never been processed. Each level loads the *processed*
    // output of the one below it, so the value grows an extra `!` per level; doing it in a
    // single pass is the point of the driver — the tasks wait for each other through the gate
    // instead of being ordered or retried by hand.
    block_on(process_server.process_source(AssetSourceId::Default)).expect("scan");

    assert_eq!(runs(&plain_runs, &mid_runs, &dep_runs), (1, 1, 1));
    assert_eq!(dependencies_of(&processed, "mid.mid"), ["thing.cnt"]);
    assert_eq!(dependencies_of(&processed, "dep.dep"), ["mid.mid"]);
    assert_eq!(processed_bytes(&processed, "thing.cnt"), b"one!");
    assert_eq!(processed_bytes(&processed, "mid.mid"), b"one!!");
    assert_eq!(processed_bytes(&processed, "dep.dep"), b"one!!!");

    // Nothing changed, so nothing is processed: the whole chain is skipped. The top asset is
    // the interesting one — its dependency's `full_hash` is folded from the leaf's, so it can
    // never equal a plain source hash, and without the index it was processed on every pass.
    block_on(process_server.process_source(AssetSourceId::Default)).expect("scan");
    assert_eq!(
        runs(&plain_runs, &mid_runs, &dep_runs),
        (1, 1, 1),
        "an unchanged chain is skipped"
    );

    // Change the leaf: the change has to reach the top through the two recorded dependencies.
    block_on(process_server.server().save_bytes("thing.cnt", b"changed")).expect("source");
    block_on(process_server.process_source(AssetSourceId::Default)).expect("scan");

    let after_change = runs(&plain_runs, &mid_runs, &dep_runs);
    assert!(
        after_change.0 >= 2 && after_change.1 >= 2 && after_change.2 >= 2,
        "every level of the chain was processed again, got {after_change:?}"
    );
    assert_eq!(processed_bytes(&processed, "thing.cnt"), b"changed!");
    assert_eq!(processed_bytes(&processed, "mid.mid"), b"changed!!");
    assert_eq!(processed_bytes(&processed, "dep.dep"), b"changed!!!");

    // Whatever order the pass picked, it ended consistent: the next pass has nothing to do.
    block_on(process_server.process_source(AssetSourceId::Default)).expect("scan");
    assert_eq!(
        runs(&plain_runs, &mid_runs, &dep_runs),
        after_change,
        "the pass converged"
    );
}

/// A loader that can be held inside `load`, so a test can have a write in flight.
#[derive(TypePath)]
struct GatedLoader {
    entered: Arc<AtomicBool>,
    /// While this is set, `load` does not return.
    held: Arc<AtomicBool>,
}

impl AssetLoader for GatedLoader {
    type Asset = SrcAsset;
    type Settings = ();

    const EXTENSIONS: &[&'static str] = &["gate"];

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _context: &mut LoadContext<'_>,
        _settings: &Self::Settings,
    ) -> Result<Self::Asset, AssetLoadError> {
        self.entered.store(true, Ordering::SeqCst);

        while self.held.load(Ordering::SeqCst) {
            futures_lite::future::yield_now().await;
        }

        let mut bytes = Vec::new();
        reader
            .read_all_bytes(&mut bytes)
            .await
            .map_err(|error| AssetLoadError::from(error.to_string()))?;

        Ok(SrcAsset(String::from_utf8_lossy(&bytes).into_owned()))
    }
}

/// A read of the processed side waits for the importer's transaction lock, so it never sees the
/// old `.meta` next to new bytes — and once it gets through, it sees the whole new revision.
#[test]
fn a_processed_read_waits_for_the_importer_write() {
    type GatedProcessor = LoadTransformAndSave<GatedLoader, AppendTransformer, DstSaver, DstLoader>;

    // Two threads are the point of this test: the read has to be parked on the asset's transaction
    // lock while the importer's task holds it. With the single-threaded task pool there is no second
    // thread to park, so nothing here could be observed.
    if zlim_task::cfg::single_thread!() {
        return;
    }

    let source = Dir::new(PathBuf::new());
    let processed = Dir::new(PathBuf::new());

    let entered = Arc::new(AtomicBool::new(false));
    let held = Arc::new(AtomicBool::new(false));

    let process_server = process_server_with(&source, &processed);

    process_server.server().register_loader(GatedLoader {
        entered: entered.clone(),
        held: held.clone(),
    });
    process_server.server().register_saver(DstSaver);
    process_server.register_processor(GatedProcessor::new(AppendTransformer, DstSaver));
    process_server.register_extension::<GatedProcessor>("gate");

    block_on(process_server.server().save_bytes("a.gate", b"one")).expect("source");

    // The first run establishes the processed side *and* this asset's status, so a later read is
    // not waiting for the run — only for the write it is about to race with.
    block_on(process_server.run()).expect("the run should initialize");
    assert!(matches!(
        block_on(process_server.wait_until_processed("a.gate")),
        ProcessStatus::Processed
    ));
    assert_eq!(processed_bytes(&processed, "a.gate"), b"one!");

    // The app's server reads the processed side through the gate (it has no importer of its own).
    let app = AssetServer::new(
        process_server.clone_sources(),
        AssetServerMode::Processed,
        AssetMetaCheckMode::Always,
        UnapprovedPathMode::Deny,
        false,
    );

    // A second revision, with the processor held inside `load` for as long as the test wants:
    // from here on the importer holds this asset's transaction lock for writing.
    block_on(process_server.server().save_bytes("a.gate", b"two")).expect("source");
    entered.store(false, Ordering::SeqCst);
    held.store(true, Ordering::SeqCst);

    let writing = {
        let process_server = process_server.clone();
        IoTaskPool::get().spawn(async move { process_server.process_asset("a.gate").await })
    };

    while !entered.load(Ordering::SeqCst) {
        std::thread::yield_now();
    }

    // The read gets past the gate (the asset *has* been processed) and then waits for the lock.
    let mut read = Box::pin(app.load_bytes("a.gate"));
    assert!(
        block_on(futures_lite::future::poll_once(&mut read)).is_none(),
        "the read waits while the processed files are being rewritten"
    );

    // Let the write finish: the read then sees the new revision, whole.
    held.store(false, Ordering::SeqCst);
    block_on(writing).expect("the second run should succeed");

    assert_eq!(
        block_on(&mut read).expect("the read should finish"),
        b"two!"
    );
    assert_eq!(processed_bytes(&processed, "a.gate"), b"two!");
}

/// A watcher that does nothing but hand the test the channel the source sends changes on.
///
/// The importer listens to that channel, so pushing an event is exactly what a file-system
/// watcher would do.
struct TestWatcher {
    #[expect(dead_code, reason = "held so the channel stays open")]
    sender: Sender<AssetSourceEvent>,
}

impl AssetWatcher for TestWatcher {}

/// [`process_server_with`], plus the channel that drives the source's watcher.
fn process_server_with_watcher(
    source: &Dir,
    processed: &Dir,
) -> (AssetProcessServer, Sender<AssetSourceEvent>) {
    let reader_dir = source.clone();
    let writer_dir = source.clone();
    let processed_reader_dir = processed.clone();
    let processed_writer_dir = processed.clone();

    let slot: Arc<Mutex<Option<Sender<AssetSourceEvent>>>> = Arc::new(Mutex::new(None));
    let slot_for_watcher = slot.clone();

    let mut builders = AssetSourceBuilders::default();
    builders.insert(
        AssetSourceId::Default,
        AssetSourceBuilder::new(move || {
            Box::new(MemoryAssetReader {
                root: reader_dir.clone(),
            }) as Box<dyn ErasedAssetReader>
        })
        .with_writer(move || {
            Some(Box::new(MemoryAssetWriter {
                root: writer_dir.clone(),
            }) as Box<dyn ErasedAssetWriter>)
        })
        .with_processed_reader(move || {
            Box::new(MemoryAssetReader {
                root: processed_reader_dir.clone(),
            }) as Box<dyn ErasedAssetReader>
        })
        .with_processed_writer(move || {
            Some(Box::new(MemoryAssetWriter {
                root: processed_writer_dir.clone(),
            }) as Box<dyn ErasedAssetWriter>)
        })
        .with_watcher(move |sender: Sender<AssetSourceEvent>| {
            *slot_for_watcher.lock().unwrap() = Some(sender.clone());

            Some(Box::new(TestWatcher { sender }) as Box<dyn AssetWatcher>)
        }),
    );

    let process_server = AssetProcessServer::build(&mut builders, false, None);

    let sender = slot
        .lock()
        .unwrap()
        .clone()
        .expect("the source should have been built with its watcher");

    (process_server, sender)
}

/// Waits (bounded, by yielding rather than by sleeping) until `condition` holds.
///
/// The task pools are ticked between attempts: the importer's work runs on the IO task pool, and in
/// single-threaded mode a spawned task only makes progress while somebody drives that pool's
/// executor — the job an app's runner does.
#[track_caller]
fn spin_until(what: &str, condition: impl Fn() -> bool) {
    for _ in 0..10_000 {
        if condition() {
            return;
        }

        zlim_task::run_local();
        std::thread::yield_now();
    }

    panic!("waited for {what}, but it never happened");
}

/// Waits until the service has nothing in flight any more.
///
/// A change is picked up asynchronously, so a test that wants to move on to the *next* change
/// has to wait for the first one to settle: only then can a later event not race with the task
/// it started.
#[track_caller]
fn spin_until_idle(server: &AssetProcessServer) {
    spin_until("the importer to be idle", || {
        server.state() == ProcessorState::Finished
    });
}

/// An asset with no processor is *copied* to the processed side: a processed source holds
/// everything the app can load, not only what a processor produced.
#[test]
fn an_asset_without_a_processor_is_copied_over() {
    let source = Dir::new(PathBuf::new());
    let processed = Dir::new(PathBuf::new());

    let process_server = process_server_with(&source, &processed);
    process_server.server().register_loader(SrcLoader);

    block_on(process_server.server().save_bytes("thing.src", b"plain")).expect("source");
    block_on(process_server.run()).expect("the run should initialize");

    assert_eq!(processed_bytes(&processed, "thing.src"), b"plain");

    let meta_bytes = processed
        .get_meta(Path::new("thing.src"))
        .expect("the copy should have a meta")
        .value()
        .to_vec();
    let meta = AssetMeta::<(), ()>::deserialize(&meta_bytes).expect("meta");

    match meta.asset_config {
        AssetConfig::Load { loader, .. } => {
            assert_eq!(loader, <SrcLoader as zlim_path::TypePath>::type_path());
        }
        _ => panic!("a copied asset is a load config naming its loader"),
    }

    assert!(
        meta.processed_info.is_some(),
        "a copy records what it was made from, like any other output"
    );

    // Nothing changed: the second pass leaves it alone.
    block_on(process_server.run()).expect("the run should initialize");
    assert_eq!(processed_bytes(&processed, "thing.src"), b"plain");
}

/// The processed side is cleaned up on every scan: an output whose source is gone is removed,
/// and so is a folder that held nothing else.
#[test]
fn a_processed_output_without_a_source_is_removed() {
    type CountingProcessor =
        LoadTransformAndSave<CountingSrcLoader, AppendTransformer, DstSaver, DstLoader>;

    let source = Dir::new(PathBuf::new());
    let processed = Dir::new(PathBuf::new());

    let runs = Arc::new(AtomicUsize::new(0));

    let process_server = process_server_with(&source, &processed);
    process_server
        .server()
        .register_loader(CountingSrcLoader { runs });
    process_server.server().register_saver(DstSaver);
    process_server.register_processor(CountingProcessor::new(AppendTransformer, DstSaver));
    process_server.register_extension::<CountingProcessor>("cnt");

    block_on(
        process_server
            .server()
            .save_bytes("folder/thing.cnt", b"one"),
    )
    .expect("source");
    block_on(process_server.run()).expect("the run should initialize");

    assert!(processed.get_asset(Path::new("folder/thing.cnt")).is_some());
    assert!(processed.get_meta(Path::new("folder/thing.cnt")).is_some());

    // The source is deleted: the output has nothing left to be an output of.
    source.remove_asset(Path::new("folder/thing.cnt"));

    block_on(process_server.run()).expect("the run should initialize");

    assert!(
        processed.get_asset(Path::new("folder/thing.cnt")).is_none(),
        "the processed output of a deleted source is removed"
    );
    assert!(processed.get_meta(Path::new("folder/thing.cnt")).is_none());
    assert!(
        processed.get_dir(Path::new("folder")).is_none(),
        "the folder that held nothing else is removed too"
    );
}

/// Half a pair — bytes with no `.meta`, which is what a run interrupted between the two writes
/// leaves behind — is not an output: the next scan throws it away and writes the asset again.
#[test]
fn a_processed_output_without_a_meta_is_replaced() {
    type CountingProcessor =
        LoadTransformAndSave<CountingSrcLoader, AppendTransformer, DstSaver, DstLoader>;

    let source = Dir::new(PathBuf::new());
    let processed = Dir::new(PathBuf::new());

    let runs = Arc::new(AtomicUsize::new(0));

    let process_server = process_server_with(&source, &processed);
    process_server
        .server()
        .register_loader(CountingSrcLoader { runs });
    process_server.server().register_saver(DstSaver);
    process_server.register_processor(CountingProcessor::new(AppendTransformer, DstSaver));
    process_server.register_extension::<CountingProcessor>("cnt");

    block_on(process_server.server().save_bytes("thing.cnt", b"one")).expect("source");

    // The bytes are there, without the `.meta` that says how to load them.
    processed.insert_asset(Path::new("thing.cnt"), "half-written");

    block_on(process_server.run()).expect("the run should initialize");

    assert_eq!(
        processed_bytes(&processed, "thing.cnt"),
        b"one!",
        "a pair the importer cannot load is written again, not left behind"
    );
    assert!(processed.get_meta(Path::new("thing.cnt")).is_some());
}

/// An asset whose source is gone is *forgotten* by the scan, not merely emptied: a read of it is
/// told it does not exist, instead of waiting for a pass that will never come.
#[test]
fn a_removed_source_is_forgotten_by_the_next_scan() {
    type CountingProcessor =
        LoadTransformAndSave<CountingSrcLoader, AppendTransformer, DstSaver, DstLoader>;

    let source = Dir::new(PathBuf::new());
    let processed = Dir::new(PathBuf::new());

    let runs = Arc::new(AtomicUsize::new(0));

    let process_server = process_server_with(&source, &processed);
    process_server
        .server()
        .register_loader(CountingSrcLoader { runs });
    process_server.server().register_saver(DstSaver);
    process_server.register_processor(CountingProcessor::new(AppendTransformer, DstSaver));
    process_server.register_extension::<CountingProcessor>("cnt");

    block_on(process_server.server().save_bytes("thing.cnt", b"one")).expect("source");
    block_on(process_server.run()).expect("the run should initialize");

    assert!(matches!(
        block_on(process_server.wait_until_processed("thing.cnt")),
        ProcessStatus::Processed
    ));

    source.remove_asset(Path::new("thing.cnt"));

    block_on(process_server.run()).expect("the run should initialize");

    assert!(matches!(
        block_on(process_server.wait_until_processed("thing.cnt")),
        ProcessStatus::NonExistent
    ));
}

/// Source changes drive the importer: what the watcher reports is what gets looked at again.
#[test]
fn a_source_change_is_followed_by_the_service() {
    type CountingProcessor =
        LoadTransformAndSave<CountingSrcLoader, AppendTransformer, DstSaver, DstLoader>;

    let source = Dir::new(PathBuf::new());
    let processed = Dir::new(PathBuf::new());

    let runs = Arc::new(AtomicUsize::new(0));

    let (process_server, events) = process_server_with_watcher(&source, &processed);
    process_server
        .server()
        .register_loader(CountingSrcLoader { runs: runs.clone() });
    process_server.server().register_saver(DstSaver);
    process_server.register_processor(CountingProcessor::new(AppendTransformer, DstSaver));
    process_server.register_extension::<CountingProcessor>("cnt");

    block_on(process_server.server().save_bytes("thing.cnt", b"one")).expect("source");

    // The service processes what is there, and then follows the source.
    process_server.start();
    block_on(process_server.wait_until_finished());

    assert_eq!(processed_bytes(&processed, "thing.cnt"), b"one!");

    // A change the watcher reports is queued and processed again.
    block_on(process_server.server().save_bytes("thing.cnt", b"two")).expect("source");
    events
        .send(AssetSourceEvent::ModifiedAsset(PathBuf::from("thing.cnt")))
        .expect("the source should still be listening");

    spin_until("the change to be processed", || {
        processed
            .get_asset(Path::new("thing.cnt"))
            .is_some_and(|bytes| bytes.value() == b"two!")
    });

    assert_eq!(
        runs.load(Ordering::SeqCst),
        2,
        "the change was processed once"
    );
    spin_until_idle(&process_server);

    // A removal throws the output away, and whoever waits on it is told it is gone.
    events
        .send(AssetSourceEvent::RemovedAsset(PathBuf::from("thing.cnt")))
        .expect("the source should still be listening");

    spin_until("the removal to be handled", || {
        processed.get_asset(Path::new("thing.cnt")).is_none()
            && matches!(
                block_on(process_server.wait_until_processed("thing.cnt")),
                ProcessStatus::NonExistent
            )
    });
}

/// A rename moves the processed output instead of processing it again from scratch.
#[test]
fn a_renamed_source_moves_its_processed_output() {
    type CountingProcessor =
        LoadTransformAndSave<CountingSrcLoader, AppendTransformer, DstSaver, DstLoader>;

    let source = Dir::new(PathBuf::new());
    let processed = Dir::new(PathBuf::new());

    let runs = Arc::new(AtomicUsize::new(0));

    let (process_server, events) = process_server_with_watcher(&source, &processed);
    process_server
        .server()
        .register_loader(CountingSrcLoader { runs: runs.clone() });
    process_server.server().register_saver(DstSaver);
    process_server.register_processor(CountingProcessor::new(AppendTransformer, DstSaver));
    process_server.register_extension::<CountingProcessor>("cnt");

    block_on(process_server.server().save_bytes("a.cnt", b"one")).expect("source");

    process_server.start();
    block_on(process_server.wait_until_finished());

    assert!(processed.get_asset(Path::new("a.cnt")).is_some());
    assert!(
        processed.get_meta(Path::new("a.cnt")).is_some(),
        "the first pass should have written a meta"
    );

    // The source file moves: the processed bytes and their `.meta` move with it.
    block_on(process_server.server().save_bytes("b.cnt", b"one")).expect("source");
    source.remove_asset(Path::new("a.cnt"));

    events
        .send(AssetSourceEvent::RenamedAsset {
            old: PathBuf::from("a.cnt"),
            new: PathBuf::from("b.cnt"),
        })
        .expect("the source should still be listening");

    // A move is two writes (the bytes, then the `.meta`), so both have to be there.
    spin_until("the rename to be handled", || {
        processed.get_asset(Path::new("b.cnt")).is_some()
            && processed.get_meta(Path::new("b.cnt")).is_some()
    });

    assert!(processed.get_asset(Path::new("a.cnt")).is_none());
}

/// A default `.meta` records who reads an asset, and never overwrites an existing one.
#[test]
fn a_default_meta_names_the_processor_or_the_loader() {
    type TestSettings = LoadTransformAndSaveSettings<(), (), ()>;

    let source = Dir::new(PathBuf::new());
    let processed = Dir::new(PathBuf::new());

    let process_server = process_server_with(&source, &processed);
    process_server.server().register_loader(SrcLoader);
    process_server.server().register_saver(DstSaver);
    process_server.register_processor(TestProcessor::new(AppendTransformer, DstSaver));
    process_server.register_extension::<TestProcessor>("cnt");

    // An asset the default processor handles: the meta names that processor.
    block_on(process_server.write_default_meta("thing.cnt", false))
        .expect("the default meta should be written");

    let meta_bytes = source
        .get_meta(Path::new("thing.cnt"))
        .expect("the meta should be next to the source")
        .value()
        .to_vec();
    let meta = AssetMeta::<(), TestSettings>::deserialize(&meta_bytes).expect("meta");

    match meta.asset_config {
        AssetConfig::Process { processor, .. } => {
            // The processor is named by its fully-qualified type path: that form always selects
            // exactly one kind of processor, so a `.meta` needs no knowledge of which short names
            // happen to be unambiguous.
            assert_eq!(
                processor,
                <TestProcessor as zlim_path::TypePath>::type_path()
            );
        }
        _ => panic!("a processor default meta is a process config naming that processor"),
    }

    // The `.meta` that was just written names the processor, so the asset it was written for is
    // processed like any other.
    block_on(process_server.server().save_bytes("thing.cnt", b"cnt"))
        .expect("the source should be written");
    block_on(process_server.process_asset("thing.cnt")).expect("processing should succeed");

    assert_eq!(
        processed
            .get_asset(Path::new("thing.cnt"))
            .expect("the asset named by the default meta should be processed")
            .value(),
        b"cnt!"
    );

    // It is never overwritten.
    assert!(matches!(
        block_on(process_server.write_default_meta("thing.cnt", false)),
        Err(AssetMetaWriteError::MetaAlreadyExists)
    ));

    // An asset no processor handles, but a loader does: the loader is named instead.
    block_on(process_server.write_default_meta("thing.src", false))
        .expect("the default meta should be written");

    let meta_bytes = source
        .get_meta(Path::new("thing.src"))
        .expect("the meta should be next to the source")
        .value()
        .to_vec();
    let meta = AssetMeta::<(), ()>::deserialize(&meta_bytes).expect("meta");

    match meta.asset_config {
        AssetConfig::Load { loader, .. } => {
            assert_eq!(loader, <SrcLoader as zlim_path::TypePath>::type_path());
        }
        _ => panic!("a loader default meta is a load config naming that loader"),
    }

    // Nothing at all can read that one.
    assert!(matches!(
        block_on(process_server.write_default_meta("thing.unknown", false)),
        Err(AssetMetaWriteError::MissingAssetLoader(_))
    ));
}

/// An in-memory transaction log, so the test can see what a run recorded and hand the next run
/// a "previous run" to recover from.
#[derive(Clone, Default)]
struct RecordingLog {
    entries: Arc<Mutex<Vec<LogEntry>>>,
    previous: Arc<Mutex<Vec<LogEntry>>>,
}

impl TransactionLogger for RecordingLog {
    fn read(&self) -> BoxedFuture<'_, Result<Vec<LogEntry>, TransactionError>> {
        Box::pin(async move { Ok(self.previous.lock().unwrap().clone()) })
    }

    fn new_log(&self) -> BoxedFuture<'_, Result<Box<dyn TransactionLog>, TransactionError>> {
        Box::pin(async move {
            self.entries.lock().unwrap().clear();
            Ok(Box::new(RecordingLogWriter {
                entries: self.entries.clone(),
            }) as Box<dyn TransactionLog>)
        })
    }
}

struct RecordingLogWriter {
    entries: Arc<Mutex<Vec<LogEntry>>>,
}

impl TransactionLog for RecordingLogWriter {
    fn unrecoverable(&mut self) -> BoxedFuture<'_, Result<(), TransactionError>> {
        Box::pin(async move {
            self.entries
                .lock()
                .unwrap()
                .push(LogEntry::UnrecoverableError);
            Ok(())
        })
    }

    fn start<'a>(&'a mut self, asset: &'a str) -> BoxedFuture<'a, Result<(), TransactionError>> {
        Box::pin(async move {
            self.entries
                .lock()
                .unwrap()
                .push(LogEntry::ProcessingStarted(asset.to_string()));
            Ok(())
        })
    }

    fn finish<'a>(&'a mut self, asset: &'a str) -> BoxedFuture<'a, Result<(), TransactionError>> {
        Box::pin(async move {
            self.entries
                .lock()
                .unwrap()
                .push(LogEntry::ProcessingFinished(asset.to_string()));
            Ok(())
        })
    }
}

/// One full run of the importer: it processes what the source tree holds, ends in `Finished`, and
/// records a start/finish pair in the transaction log, while the outcome stays available through
/// the waiter — for an asset that was processed and for one that does not exist.
///
/// A second run skips what is up to date. A log whose last entry is a start with no finish — the
/// trace an interrupted run leaves behind — instead makes it distrust the processed side and do
/// the work again.
#[test]
fn the_process_server_runs_the_import_and_reports_it() {
    type CountingProcessor =
        LoadTransformAndSave<CountingSrcLoader, AppendTransformer, DstSaver, DstLoader>;

    let source = Dir::new(PathBuf::new());
    let processed = Dir::new(PathBuf::new());

    // The importer builds its own sources (and its own server) from the builders.
    let reader_dir = source.clone();
    let writer_dir = source.clone();
    let processed_reader_dir = processed.clone();
    let processed_writer_dir = processed.clone();

    let mut builders = AssetSourceBuilders::default();
    builders.insert(
        AssetSourceId::Default,
        AssetSourceBuilder::new(move || {
            Box::new(MemoryAssetReader {
                root: reader_dir.clone(),
            }) as Box<dyn ErasedAssetReader>
        })
        .with_writer(move || {
            Some(Box::new(MemoryAssetWriter {
                root: writer_dir.clone(),
            }) as Box<dyn ErasedAssetWriter>)
        })
        .with_processed_reader(move || {
            Box::new(MemoryAssetReader {
                root: processed_reader_dir.clone(),
            }) as Box<dyn ErasedAssetReader>
        })
        .with_processed_writer(move || {
            Some(Box::new(MemoryAssetWriter {
                root: processed_writer_dir.clone(),
            }) as Box<dyn ErasedAssetWriter>)
        }),
    );

    let log = RecordingLog::default();
    let process_server =
        AssetProcessServer::build(&mut builders, false, Some(Box::new(log.clone())));

    let runs = Arc::new(AtomicUsize::new(0));

    process_server.register_processor(CountingProcessor::new(AppendTransformer, DstSaver));
    process_server.register_processor(TestProcessor::new(AppendTransformer, DstSaver));
    process_server.register_extension::<CountingProcessor>("cnt");

    // The importer's own server still needs the loaders/savers: they stay on the `AssetServer`,
    // while only the processor registry lives on the importer.
    process_server
        .server()
        .register_loader(CountingSrcLoader { runs: runs.clone() });
    process_server.server().register_saver(DstSaver);

    block_on(process_server.server().save_bytes("thing.cnt", b"one")).expect("source");

    block_on(process_server.run()).expect("the run should initialize");

    assert_eq!(process_server.state(), ProcessorState::Finished);
    assert_eq!(runs.load(Ordering::SeqCst), 1);
    assert_eq!(
        processed
            .get_asset(Path::new("thing.cnt"))
            .expect("processed bytes")
            .value(),
        b"one!"
    );

    // The run recorded a start/finish pair for the asset it processed.
    let entries = log.entries.lock().unwrap().clone();
    assert_eq!(
        entries,
        vec![
            LogEntry::ProcessingStarted("thing.cnt".into()),
            LogEntry::ProcessingFinished("thing.cnt".into()),
        ]
    );

    // The result is observable through the waiter, for an asset that was processed...
    assert!(matches!(
        block_on(process_server.wait_until_processed("thing.cnt")),
        ProcessStatus::Processed
    ));
    // ...and for one that is not there.
    assert!(matches!(
        block_on(process_server.wait_until_processed("nothing.cnt")),
        ProcessStatus::NonExistent
    ));

    // A second run skips what is up to date.
    block_on(process_server.run()).expect("the run should initialize");
    assert_eq!(runs.load(Ordering::SeqCst), 1);

    // But an interrupted previous run makes it distrust the processed side and reprocess.
    log.previous
        .lock()
        .unwrap()
        .push(LogEntry::ProcessingStarted("thing.cnt".into()));
    block_on(process_server.run()).expect("the run should initialize");
    assert_eq!(runs.load(Ordering::SeqCst), 2);
}

/// The processed side is gated on the importer: a read of a path the importer knows about but has
/// not imported yet parks until the run answers for it, instead of failing or handing out the
/// unprocessed bytes. The run and the read are zipped, so the read really is in flight throughout.
///
/// A path the importer has never seen is missing straight away rather than hanging.
#[test]
fn a_processed_read_waits_for_the_importer() {
    type CountingProcessor =
        LoadTransformAndSave<CountingSrcLoader, AppendTransformer, DstSaver, DstLoader>;

    let source = Dir::new(PathBuf::new());
    let processed = Dir::new(PathBuf::new());

    let process_server = process_server_with(&source, &processed);

    let runs = Arc::new(AtomicUsize::new(0));

    process_server
        .server()
        .register_loader(CountingSrcLoader { runs: runs.clone() });
    process_server.server().register_saver(DstSaver);
    process_server.register_processor(CountingProcessor::new(AppendTransformer, DstSaver));
    process_server.register_extension::<CountingProcessor>("cnt");

    // The app's server reads the *processed* side of the very same sources.
    let app = AssetServer::new(
        process_server.clone_sources(),
        AssetServerMode::Processed,
        AssetMetaCheckMode::Always,
        UnapprovedPathMode::Deny,
        false,
    );

    block_on(process_server.server().save_bytes("thing.cnt", b"one")).expect("source");

    // Before the import the path is a known source asset with no result yet, so a read of the
    // processed side has to *wait*: here the import runs while that read is parked.
    let (run, result) = block_on(futures_lite::future::zip(process_server.run(), async {
        app.load_bytes("thing.cnt").await
    }));
    run.expect("the run should initialize");

    let bytes = result.expect("the read waits for the importer");
    assert_eq!(bytes, b"one!");
    assert_eq!(runs.load(Ordering::SeqCst), 1);

    // A path the importer does not know is missing — immediately, not by hanging.
    assert!(block_on(app.load_bytes("missing.cnt")).is_err());
}
