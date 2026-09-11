use core::time::Duration;
use std::sync::Arc;

use atomicow::CowArc;
use zlim_core::derive::Resource;
use zlim_path::derive::TypePath;
use zlim_utils::hash::HashMap;
use zlim_utils::mpsc::{self, Receiver, Sender};

use super::AssetSourceEvent;
use super::{MissingAssetSource, MissingAssetWriter};
use super::{MissingProcessedAssetReader, MissingProcessedAssetWriter};
use crate::ident::AssetSourceId;
use crate::io::watcher::AssetWatcher;
use crate::io::{ErasedAssetReader, ErasedAssetWriter};

// -----------------------------------------------------------------------------
// AssetSourceBuilder & AssetSource

type ReaderBuilder = Box<dyn FnMut() -> Box<dyn ErasedAssetReader> + Send + Sync>;
type WriterBuilder = Box<dyn FnMut() -> Option<Box<dyn ErasedAssetWriter>> + Send + Sync>;
type WatcherBuilder =
    Box<dyn FnMut(Sender<AssetSourceEvent>) -> Option<Box<dyn AssetWatcher>> + Send + Sync>;

/// Metadata about an "asset source".
///
/// Such as how to construct the [`AssetReader`] and [`AssetWriter`]
/// for the source, and whether or not the source is processed.
///
/// [`AssetReader`]: crate::io::AssetReader
/// [`AssetWriter`]: crate::io::AssetWriter
pub struct AssetSourceBuilder {
    /// The [`ErasedAssetReader`] to use on the unprocessed asset.
    pub reader: ReaderBuilder,

    /// The [`ErasedAssetWriter`] to use on the unprocessed asset.
    pub writer: Option<WriterBuilder>,

    /// The [`AssetWatcher`] to use for unprocessed assets, if any.
    pub watcher: Option<WatcherBuilder>,

    /// The [`ErasedAssetReader`] to use on the processed asset, if any.
    pub processed_reader: Option<ReaderBuilder>,

    /// The [`ErasedAssetWriter`] to use on the processed asset, if any.
    pub processed_writer: Option<WriterBuilder>,

    /// The [`AssetWatcher`] to use for processed assets, if any.
    pub processed_watcher: Option<WatcherBuilder>,

    /// The warning message to display when watching an unprocessed asset fails.
    pub watch_warning: Option<&'static str>,

    /// The warning message to display when watching a processed asset fails.
    pub processed_watch_warning: Option<&'static str>,
}

/// A collection of [`AssetReader`], [`AssetWriter`], and [`AssetWatcher`] instances.
///
/// For a specific asset source, identified by an [`AssetSourceId`].
///
/// [`AssetReader`]: crate::io::AssetReader
/// [`AssetWriter`]: crate::io::AssetWriter
pub struct AssetSource {
    id: AssetSourceId<'static>,
    reader: Box<dyn ErasedAssetReader>,
    writer: Option<Box<dyn ErasedAssetWriter>>,
    watcher: Option<Box<dyn AssetWatcher>>,
    processed_reader: Option<Arc<dyn ErasedAssetReader>>,
    processed_writer: Option<Box<dyn ErasedAssetWriter>>,
    processed_watcher: Option<Box<dyn AssetWatcher>>,
    event_receiver: Option<Receiver<AssetSourceEvent>>,
    processed_event_receiver: Option<Receiver<AssetSourceEvent>>,
    // TODO(asset_processor): add `ungated_processed_reader` together with
    // `gate_on_processor` when `ProcessorGatedReader` lands (M4).
}

// -----------------------------------------------------------------------------
// AssetSourceBuilder Implementation

impl AssetSourceBuilder {
    /// Creates a new builder, starting with the provided reader.
    #[inline]
    pub fn new(
        reader: impl FnMut() -> Box<dyn ErasedAssetReader> + Send + Sync + 'static,
    ) -> AssetSourceBuilder {
        Self {
            reader: Box::new(reader),
            writer: None,
            watcher: None,
            processed_reader: None,
            processed_writer: None,
            processed_watcher: None,
            watch_warning: None,
            processed_watch_warning: None,
        }
    }

    /// Builds a new [`AssetSource`] with the given `id`.
    ///
    /// - If `watch` is true, the unprocessed source will watch for changes.
    /// - If `watch_processed` is true, the processed source will watch for changes.
    ///
    /// Note that the default watcher needs the `notify` feature.
    pub fn build(
        &mut self,
        id: AssetSourceId<'static>,
        watch: bool,
        watch_processed: bool,
    ) -> AssetSource {
        let reader = self.reader.as_mut()();
        let writer = self.writer.as_mut().and_then(|w| w());
        let processed_reader = self.processed_reader.as_mut().map(|r| Arc::from(r()));
        let processed_writer = self.processed_writer.as_mut().and_then(|w| w());

        let mut source = AssetSource {
            id: id.clone(),
            reader,
            writer,
            processed_reader,
            processed_writer,
            watcher: None,
            event_receiver: None,
            processed_watcher: None,
            processed_event_receiver: None,
        };

        if watch {
            let (sender, receiver) = mpsc::channel();
            if let Some(w) = self.watcher.as_mut().and_then(|w| w(sender)) {
                source.watcher = Some(w);
                source.event_receiver = Some(receiver);
            } else if let Some(warning) = self.watch_warning {
                zlim_log::warn!("{id} does not have an AssetWatcher configured. {warning}");
            }
        }

        if watch_processed {
            let (sender, receiver) = mpsc::channel();
            if let Some(w) = self.processed_watcher.as_mut().and_then(|w| w(sender)) {
                source.processed_watcher = Some(w);
                source.processed_event_receiver = Some(receiver);
            } else if let Some(warning) = self.processed_watch_warning {
                zlim_log::warn!(
                    "{id} does not have a processed AssetWatcher configured. {warning}"
                );
            }
        }

        source
    }

    /// Will use the given function to construct unprocessed [`ErasedAssetReader`].
    #[inline]
    pub fn with_reader(
        mut self,
        reader: impl FnMut() -> Box<dyn ErasedAssetReader> + Send + Sync + 'static,
    ) -> Self {
        self.reader = Box::new(reader);
        self
    }

    /// Will use the given function to construct unprocessed [`ErasedAssetWriter`].
    #[inline]
    pub fn with_writer(
        mut self,
        writer: impl FnMut() -> Option<Box<dyn ErasedAssetWriter>> + Send + Sync + 'static,
    ) -> Self {
        self.writer = Some(Box::new(writer));
        self
    }

    /// Will use the given function to construct unprocessed [`AssetWatcher`].
    #[inline]
    pub fn with_watcher(
        mut self,
        watcher: impl FnMut(Sender<AssetSourceEvent>) -> Option<Box<dyn AssetWatcher>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.watcher = Some(Box::new(watcher));
        self
    }

    /// Will use the given function to construct processed [`ErasedAssetReader`].
    #[inline]
    pub fn with_processed_reader(
        mut self,
        reader: impl FnMut() -> Box<dyn ErasedAssetReader> + Send + Sync + 'static,
    ) -> Self {
        self.processed_reader = Some(Box::new(reader));
        self
    }

    /// Will use the given function to construct processed [`ErasedAssetWriter`].
    #[inline]
    pub fn with_processed_writer(
        mut self,
        writer: impl FnMut() -> Option<Box<dyn ErasedAssetWriter>> + Send + Sync + 'static,
    ) -> Self {
        self.processed_writer = Some(Box::new(writer));
        self
    }

    /// Will use the given function to construct processed [`AssetWatcher`].
    #[inline]
    pub fn with_processed_watcher(
        mut self,
        watcher: impl FnMut(Sender<AssetSourceEvent>) -> Option<Box<dyn AssetWatcher>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.processed_watcher = Some(Box::new(watcher));
        self
    }

    /// Enables a warning for the unprocessed source watcher.
    ///
    /// which will print when watching is enabled and the unprocessed source doesn't have a watcher.
    #[inline]
    pub fn with_watch_warning(mut self, warning: &'static str) -> Self {
        self.watch_warning = Some(warning);
        self
    }

    /// Enables a warning for the processed source watcher.
    ///
    /// which will print when watching is enabled and the processed source doesn't have a watcher.
    #[inline]
    pub fn with_processed_watch_warning(mut self, warning: &'static str) -> Self {
        self.processed_watch_warning = Some(warning);
        self
    }

    /// Returns a builder containing the "platform default source" for the given `path` and
    /// `processed_path`.
    ///
    /// For most platforms, this will use [`FileAssetReader`] / [`FileAssetWriter`],
    /// but some platforms (such as Android and Wasm) have their own default readers / writers /
    /// watchers.
    ///
    /// [`FileAssetReader`]: crate::io::file::FileAssetReader
    /// [`FileAssetWriter`]: crate::io::file::FileAssetWriter
    pub fn platform_default(path: &str, processed_path: Option<&str>) -> Self {
        const D: Duration = Duration::from_millis(300); // debounce wait time

        let default = Self::new(AssetSource::default_reader(path.to_owned(), false))
            .with_writer(AssetSource::default_writer(path.to_owned(), false))
            .with_watcher(AssetSource::default_watcher(path.to_owned(), false, D))
            .with_watch_warning(AssetSource::default_watch_warning());

        let Some(p_path) = processed_path else {
            return default;
        };

        default
            .with_processed_reader(AssetSource::default_reader(p_path.to_owned(), true))
            .with_processed_writer(AssetSource::default_writer(p_path.to_owned(), true))
            .with_processed_watcher(AssetSource::default_watcher(p_path.to_owned(), true, D))
            .with_processed_watch_warning(AssetSource::default_watch_warning())
    }
}

impl AssetSource {
    /// Returns this [`AssetSourceId`].
    #[inline]
    pub fn id(&self) -> AssetSourceId<'static> {
        self.id.clone()
    }

    /// Return's this source's unprocessed [`ErasedAssetReader`].
    #[inline]
    pub fn reader(&self) -> &dyn ErasedAssetReader {
        &*self.reader
    }

    /// Return's this source's unprocessed [`ErasedAssetWriter`], if it exists.
    #[inline]
    pub fn writer(&self) -> Result<&dyn ErasedAssetWriter, MissingAssetWriter> {
        self.writer
            .as_deref()
            .ok_or_else(|| MissingAssetWriter(self.id.clone_owned()))
    }

    /// Return's this source's processed [`ErasedAssetReader`], if it exists.
    #[inline]
    pub fn processed_reader(&self) -> Result<&dyn ErasedAssetReader, MissingProcessedAssetReader> {
        self.processed_reader
            .as_deref()
            .ok_or_else(|| MissingProcessedAssetReader(self.id.clone_owned()))
    }

    /// Return's this source's processed [`ErasedAssetWriter`], if it exists.
    #[inline]
    pub fn processed_writer(&self) -> Result<&dyn ErasedAssetWriter, MissingProcessedAssetWriter> {
        self.processed_writer
            .as_deref()
            .ok_or_else(|| MissingProcessedAssetWriter(self.id.clone_owned()))
    }

    /// Return's this source's unprocessed watcher, if the source is currently watching.
    #[inline]
    pub fn watcher(&self) -> Option<&dyn AssetWatcher> {
        self.watcher.as_deref()
    }

    /// Return's this source's processed watcher, if the source is currently watching.
    #[inline]
    pub fn processed_watcher(&self) -> Option<&dyn AssetWatcher> {
        self.processed_watcher.as_deref()
    }

    /// Return's this source's unprocessed event receiver,
    /// if the source is currently watching for changes.
    #[inline]
    pub fn event_receiver(&self) -> Option<&Receiver<AssetSourceEvent>> {
        self.event_receiver.as_ref()
    }

    /// Return's this source's processed event receiver,
    /// if the source is currently watching for changes.
    #[inline]
    pub fn processed_event_receiver(&self) -> Option<&Receiver<AssetSourceEvent>> {
        self.processed_event_receiver.as_ref()
    }

    /// Returns true if the assets in this source should be processed.
    #[inline]
    pub fn should_process(&self) -> bool {
        self.processed_writer.is_some()
    }

    /// Returns a builder function for this platform's default [`ErasedAssetReader`].
    ///
    /// - `path` is the relative path to the asset root.
    /// - `processed` control whether the data has been processed.
    pub fn default_reader(
        _path: String,
        _processed: bool,
    ) -> impl FnMut() -> Box<dyn ErasedAssetReader> + Send + Sync {
        move || {
            cfg_select! {
                target_family = "wasm" => {
                    let reader = crate::io::http::HttpWasmAssetReader::new(&_path);
                    Box::new(reader) as Box<dyn ErasedAssetReader>
                }
                target_os = "android" => {
                    Box::new(crate::io::platform::AndroidAssetReader) as Box<dyn ErasedAssetReader>
                }
                _ => {
                    let reader = crate::io::file::FileAssetReader::new(&_path);
                    Box::new(reader) as Box<dyn ErasedAssetReader>
                }
            }
        }
    }

    /// Returns a builder function for this platform's default [`ErasedAssetWriter`].
    ///
    /// - `path` is the relative path to the asset root.
    /// - `processed` control whether the data has been processed.
    pub fn default_writer(
        _path: String,
        _processed: bool,
    ) -> impl FnMut() -> Option<Box<dyn ErasedAssetWriter>> + Send + Sync {
        move || {
            cfg_select! {
                target_family = "wasm" => None,
                target_os = "android" => None,
                _ => {
                    let writer = crate::io::file::FileAssetWriter::new(&_path, _processed);
                    Some(Box::new(writer) as Box<dyn ErasedAssetWriter>)
                },
            }
        }
    }

    /// Returns a builder function for this platform's default [`AssetWatcher`].
    ///
    /// - `path` is the relative path to the asset root.
    /// - `processed` control whether the data has been processed.
    pub fn default_watcher(
        _path: String,
        _processed: bool,
        _debounce_wait_time: Duration,
    ) -> impl FnMut(Sender<AssetSourceEvent>) -> Option<Box<dyn AssetWatcher>> + Send + Sync {
        move |_sender: Sender<AssetSourceEvent>| {
            crate::cfg::notify! {
                if {
                    let path = crate::io::file::base_path().join(_path.clone());
                    if path.exists() {
                        crate::io::watcher::FileWatcher::build(path, _sender, _debounce_wait_time)
                    } else {
                        zlim_log::warn!("Skip creating file watcher because path {path:?} does not exist.");
                        None
                    }
                } else {
                    None
                }
            }
        }
    }

    /// Returns a default watch warning message for this platform.
    pub fn default_watch_warning() -> &'static str {
        if crate::cfg::notify!() {
            return "Consider adding an \"assets\" directory.";
        }

        cfg_select! {
            target_family = "wasm" => "Web does not currently support watching assets.",
            target_os = "android" => "Android does not currently support watching assets.",
            feature = "notify" => "The current platform does not currently support watching assets.",
            _ => "Consider enabling the `notify` feature.",
        }
    }
}

// -----------------------------------------------------------------------------
// Errors

const MISSING_DEFAULT_SOURCE: &str =
    "A default AssetSource is required. Add one to `AssetSourceBuilders`";

// -----------------------------------------------------------------------------
// AssetSources

/// A [`Resource`] that hold (repeatable) functions capable of producing
/// new [`AssetReader`] and [`AssetWriter`] instances for a given asset source.
///
/// [`AssetReader`]: crate::io::AssetReader
/// [`AssetWriter`]: crate::io::AssetWriter
#[derive(TypePath, Resource, Default)]
pub struct AssetSourceBuilders {
    sources: HashMap<CowArc<'static, str>, AssetSourceBuilder>,
    default: Option<AssetSourceBuilder>,
}

impl AssetSourceBuilders {
    /// Inserts a new builder with the given `id`
    pub fn insert(&mut self, id: impl Into<AssetSourceId<'static>>, source: AssetSourceBuilder) {
        match id.into() {
            AssetSourceId::Default => {
                self.default = Some(source);
            }
            AssetSourceId::Name(name) => {
                self.sources.insert(name, source);
            }
        }
    }

    /// Gets a mutable builder with the given `id`, if it exists.
    pub fn get_mut<'a, 'b>(
        &'a mut self,
        id: impl Into<AssetSourceId<'b>>,
    ) -> Option<&'a mut AssetSourceBuilder> {
        match id.into() {
            AssetSourceId::Default => self.default.as_mut(),
            AssetSourceId::Name(name) => self.sources.get_mut(&name.into_owned()),
        }
    }

    /// Initializes the default [`AssetSourceBuilder`] if it has not already been set.
    pub fn init_default_source(&mut self, path: &str, processed_path: Option<&str>) {
        self.default
            .get_or_insert_with(|| AssetSourceBuilder::platform_default(path, processed_path));
    }

    /// Builds a new [`AssetSources`] collection.
    ///
    /// - If `watch` is true, the unprocessed sources will watch for changes.
    /// - If `watch_processed` is true, the processed sources will watch for changes.
    ///
    /// Note that the default watcher needs to enable the `notify` cargo feature.
    pub fn build_sources(&mut self, watch: bool, watch_processed: bool) -> AssetSources {
        let mut sources: HashMap<&'static str, AssetSource> = HashMap::new();

        for (key, source) in &mut self.sources {
            let k: &'static str = match key {
                CowArc::Static(k) => k,
                CowArc::Borrowed(b) => zlim_utils::str::intern_str(b),
                CowArc::Owned(o) => zlim_utils::str::intern_str(o),
            };

            let id = AssetSourceId::Name(CowArc::Static(k));
            let source = source.build(id, watch, watch_processed);

            sources.insert(k, source);
        }

        let default = self
            .default
            .as_mut()
            .map(|p| p.build(AssetSourceId::Default, watch, watch_processed))
            .expect(MISSING_DEFAULT_SOURCE);

        AssetSources { sources, default }
    }
}

// -----------------------------------------------------------------------------
// AssetSources

/// A collection of [`AssetSource`]s.
pub struct AssetSources {
    sources: HashMap<&'static str, AssetSource>,
    default: AssetSource,
}

impl AssetSources {
    /// Gets the [`AssetSource`] with the given `id`, if it exists.
    pub fn get<'a, 'b>(
        &'a self,
        id: impl Into<AssetSourceId<'b>>,
    ) -> Result<&'a AssetSource, MissingAssetSource> {
        match id.into().into_owned() {
            AssetSourceId::Default => Ok(&self.default),
            AssetSourceId::Name(name) => self
                .sources
                .get(name.as_ref())
                .ok_or(MissingAssetSource(AssetSourceId::Name(name))),
        }
    }

    /// Iterates all asset sources in the collection (including the default source).
    pub fn iter(&self) -> impl Iterator<Item = &AssetSource> {
        self.sources.values().chain(Some(&self.default))
    }

    /// Mutably iterates all asset sources in the collection (including the default source).
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut AssetSource> {
        self.sources.values_mut().chain(Some(&mut self.default))
    }

    /// Iterates all processed asset sources in the collection (including the default source).
    pub fn iter_processed(&self) -> impl Iterator<Item = &AssetSource> {
        self.iter().filter(|p| p.should_process())
    }

    /// Mutably iterates all processed asset sources in the collection (including the default source).
    pub fn iter_processed_mut(&mut self) -> impl Iterator<Item = &mut AssetSource> {
        self.iter_mut().filter(|p| p.should_process())
    }

    /// Iterates over the [`AssetSourceId`] of every source (including the default source).
    pub fn iter_id(&self) -> impl Iterator<Item = AssetSourceId<'static>> + '_ {
        self.sources
            .keys()
            .map(|k| AssetSourceId::Name(CowArc::Static(*k)))
            .chain(Some(AssetSourceId::Default))
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::memory::{MemoryAssetReader, MemoryAssetWriter};

    /// Creates a builder for a plain in-memory source.
    fn memory_builder() -> AssetSourceBuilder {
        AssetSourceBuilder::new(|| {
            Box::new(MemoryAssetReader::default()) as Box<dyn ErasedAssetReader>
        })
        .with_writer(|| Some(Box::new(MemoryAssetWriter::default()) as Box<dyn ErasedAssetWriter>))
    }

    /// Looks a source up, failing the test when it is missing.
    fn source(sources: &AssetSources, id: impl Into<AssetSourceId<'static>>) -> &AssetSource {
        match sources.get(id) {
            Ok(source) => source,
            Err(error) => panic!("{error}"),
        }
    }

    #[test]
    fn named_sources_are_looked_up_by_id() {
        let mut builders = AssetSourceBuilders::default();
        builders.insert(AssetSourceId::Default, memory_builder());
        builders.insert(AssetSourceId::from("remote"), memory_builder());

        let sources = builders.build_sources(false, false);

        assert_eq!(sources.iter_id().count(), 2);
        assert!(source(&sources, AssetSourceId::Default).writer().is_ok());
        assert!(source(&sources, "remote").writer().is_ok());

        let Err(error) = sources.get("missing") else {
            panic!("an unregistered source id must not resolve");
        };
        assert_eq!(
            error.to_string(),
            "Asset Source 'AssetSourceId::Name(missing)' does not exist"
        );
    }

    #[test]
    fn sources_without_a_processed_writer_are_not_processed() {
        let mut builders = AssetSourceBuilders::default();
        builders.insert(AssetSourceId::Default, memory_builder());

        let sources = builders.build_sources(false, false);
        let default = source(&sources, AssetSourceId::Default);

        assert!(!default.should_process());
        assert!(default.processed_reader().is_err());
        assert!(default.processed_writer().is_err());
        assert_eq!(sources.iter_processed().count(), 0);
    }

    #[test]
    fn asking_to_watch_without_a_watcher_keeps_the_source_unwatched() {
        let mut builders = AssetSourceBuilders::default();
        builders.insert(
            AssetSourceId::Default,
            memory_builder().with_watch_warning("no watcher on this target"),
        );

        let sources = builders.build_sources(true, false);
        let default = source(&sources, AssetSourceId::Default);

        assert!(default.watcher().is_none());
        assert!(default.event_receiver().is_none());
    }

    #[test]
    #[should_panic(expected = "A default AssetSource is required")]
    fn building_without_a_default_source_panics() {
        let mut builders = AssetSourceBuilders::default();
        builders.insert(AssetSourceId::from("remote"), memory_builder());
        let _ = builders.build_sources(false, false);
    }
}
