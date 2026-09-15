//! The context a processor runs with: the source reader, the asset path, and the [`ProcessedInfo`]
//! being built.

use zlim_path::TypePath;

use crate::error::{AssetLoadError, MissingAssetLoader, MissingBuilder};
use crate::io::Reader;
use crate::loaded::ErasedLoadedAsset;
use crate::loader::AssetLoader;
use crate::meta::{ProcessDependencyInfo, ProcessedInfo};
use crate::path::AssetPath;
use crate::server::AssetServer;

// -----------------------------------------------------------------------------
// ProcessContext

/// Context passed to [`AssetProcessor::process`], providing access to the source
/// asset data.
///
/// This must only expose processor data that is represented in the asset's hash.
///
/// [`AssetProcessor::process`]: crate::processor::AssetProcessor::process
pub struct ProcessContext<'a> {
    /// Accumulates process dependencies discovered during processing.
    ///
    /// DO NOT CHANGE ANY VALUES HERE OTHER THAN APPENDING TO `process_dependencies`.
    ///
    /// Do not expose this publicly: it would be too easy to invalidate state.
    new_processed_info: &'a mut ProcessedInfo,

    /// This exists to expose access to asset values.
    ///
    /// ANY ASSET VALUE THAT IS ACCESSED SHOULD BE ADDED TO `new_processed_info.process_dependencies`.
    ///
    /// Do not expose this publicly: it would be too easy to invalidate state.
    server: &'a AssetServer,

    path: &'a AssetPath<'static>,
    reader: Box<dyn Reader + 'a>,
}

impl<'a> ProcessContext<'a> {
    /// Builds the context a processor runs with.
    ///
    /// Called by [`AssetServer::process_asset_with`], which owns the reader,
    /// the path and the [`ProcessedInfo`] being built.
    ///
    /// [`AssetServer::process_asset_with`]: crate::server::AssetServer::process_asset_with
    pub(crate) fn new(
        server: &'a AssetServer,
        path: &'a AssetPath<'static>,
        reader: Box<dyn Reader + 'a>,
        new_processed_info: &'a mut ProcessedInfo,
    ) -> Self {
        Self {
            server,
            new_processed_info,
            path,
            reader,
        }
    }

    /// Returns the path of the asset being processed.
    #[inline]
    pub fn path(&self) -> &AssetPath<'static> {
        self.path
    }

    /// Returns a mutable reference to the raw asset reader.
    #[inline]
    pub fn asset_reader(&mut self) -> &mut dyn Reader {
        &mut *self.reader
    }

    /// Loads the source asset using loader `L` with the given settings.
    ///
    /// Any load dependencies are recorded as process dependencies.
    ///
    /// # Errors
    ///
    /// Fails with the error a plain load of `L`'s asset would report — no loader for the asset, a
    /// loader that failed or panicked, bytes that cannot be read — which is the error type the
    /// reference implementation returns here. A processor propagates it as its own error with `?`,
    /// which [`From<AssetLoadError>`] makes work.
    ///
    /// [`From<AssetLoadError>`]: crate::error::AssetProcessError
    pub async fn load_source_asset<L: AssetLoader>(
        &mut self,
        settings: &L::Settings,
    ) -> Result<ErasedLoadedAsset, AssetLoadError> {
        let type_path = <L as TypePath>::type_path();

        let error = || {
            core::hint::cold_path();
            AssetLoadError::from(MissingAssetLoader::from(
                MissingBuilder::new().with_type_path(type_path),
            ))
        };

        // The registry lock is released before the await, and the loader may still be waiting to be
        // registered: processing an asset of a pre-registered loader waits for it here.
        let entry = { self.server.0.read_loaders().get_by_path(type_path) };

        let loader = match entry {
            Some(Ok(loader)) => loader,
            Some(Err(pending)) => {
                ::core::hint::cold_path();
                pending.get().await.ok_or_else(error)?
            }
            None => {
                ::core::hint::cold_path();
                return Err(error());
            }
        };

        // A source asset is loaded for its *hashes*, not for its sub-assets: the nested loads the
        // loader performs are recorded as process dependencies, and the importer re-runs this asset
        // when one of them changes. Eagerly loading them here — which is what `load_dependencies`
        // asks for — would do the work twice and fail the whole process when a dependency cannot be
        // built, so the reference implementation passes `false` and this does too.
        let loaded_asset = self
            .server
            .load_with_loader(
                self.path,
                settings,
                &*loader,
                &mut *self.reader,
                false,
                true,
            )
            .await?;

        for (path, &full_hash) in &loaded_asset.loader_dependencies {
            let info = ProcessDependencyInfo {
                path: path.clone(),
                full_hash,
            };
            self.new_processed_info.process_dependencies.push(info);
        }

        Ok(loaded_asset)
    }
}
