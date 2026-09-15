//! One asset, from its `.meta` to the files it produces.
//!
//! This is the single-asset step of a pass: resolve what the asset wants — a processor with the settings
//! from its `.meta`, a plain copy, or nothing at all — decide whether the output it already has is still
//! up to date, and write the new revision under the asset's transaction lock.
//!
//! Everything here is about *one* asset. What a chain of assets makes of the results is the pass's
//! business: see `pass.rs` and `crate::processor::infos`.

use core::sync::atomic::Ordering;

use std::debug_assert_matches;
use std::sync::Arc;

use futures_lite::AsyncWriteExt;

use crate::error::*;
use crate::meta::*;
use crate::path::AssetPath;
use crate::processor::ErasedAssetProcessor;
use crate::processor::driver::read_meta_or_empty;
use crate::processor::infos::ProcessResult;
use crate::processor::server::AssetProcessServer;
use crate::server::AssetMetaCheckMode;

// -----------------------------------------------------------------------------
// What to do with one asset

/// What the importer does with one asset.
enum Action {
    /// Run a processor over it: the source is read, transformed and written to the processed side.
    Process {
        processor: Arc<dyn ErasedAssetProcessor>,
        /// The `.meta` the processor's settings come from (the source's, or a default one).
        settings_meta: Box<dyn ErasedAssetMeta>,
    },
    /// Copy the bytes through: the asset has no processor, but a loader can read it, so the
    /// processed side gets a copy of it (and a `.meta` that says how to load it).
    Copy { meta: Box<dyn ErasedAssetMeta> },
    /// Leave it alone: its `.meta` says so, or nothing can read it.
    Ignore,
}

// -----------------------------------------------------------------------------
// AssetProcessServer: one asset

impl AssetProcessServer {
    /// The single-asset step, without recording anything: resolve what to do, decide whether it is
    /// needed, and do it.
    ///
    /// # Panics
    ///
    /// In `debug_assertions` (or with the `debug` feature), a `.meta` whose `Process` settings
    /// cannot be read back as the processor's `Settings` panics instead of being reported as an
    /// [`AssetProcessError`]: the `.meta` was deserialized by that very processor, so the miss is a
    /// bug in the processor rather than a problem with the asset.
    pub(super) async fn process_asset_internal(
        &self,
        path: &AssetPath<'static>,
    ) -> Result<ProcessResult, AssetProcessError> {
        let source = self.data.sources.get(path.source_id())?;

        // The `.meta` decides which processor runs and with which settings; without one the
        // processor is chosen by the source path's extension. A missing `.meta` is normal.
        let meta_bytes = read_meta_or_empty(source.reader(), path).await?;

        let action = self.resolve_action(path, &meta_bytes).await?;

        // Both ways of writing an output — processing and copying — are skipped the same way, so
        // the source is hashed once, here, for both. An ignored asset never gets this far: nothing
        // is written for it, so there is nothing to compare against the source.
        let source_hash = if matches!(action, Action::Ignore) {
            AssetHash::ZERO
        } else {
            let source_hash = self.server.source_hash_of(path).await?;

            if self.data.skip_up_to_date.load(Ordering::SeqCst)
                && self.is_up_to_date(path, source_hash).await
            {
                return Ok(ProcessResult::SkippedNotChanged);
            }

            source_hash
        };

        match action {
            Action::Ignore => Ok(ProcessResult::Ignored),
            Action::Process {
                processor,
                settings_meta,
            } => {
                #[cold]
                #[inline(never)]
                fn missing_process_settings(
                    processor: &dyn ErasedAssetProcessor,
                    path: &AssetPath<'static>,
                ) -> AssetProcessError {
                    let processor_name = processor.type_path();

                    let msg = format!(
                        "AssetProcessor `{processor_name}` deserialized the `.meta` of '{path}' as a \
                        `Process` config, but its `Settings` cannot be read back from that meta; this \
                        is most likely a bug in the processor, not a problem with the asset"
                    );

                    if cfg!(any(debug_assertions, feature = "debug")) {
                        panic!("{msg}")
                    } else {
                        AssetProcessError::from(AssetError::Custom(msg))
                    }
                }

                // The `.meta` was deserialized by this very processor, and it names a `Process`
                // config: settings are the one thing it has to carry, so there is nothing to resolve
                // here — see `missing_process_settings` for what a miss means.
                let settings = settings_meta
                    .process_settings()
                    .ok_or_else(|| missing_process_settings(&*processor, path))?;

                // The processed files are rewritten under the asset's transaction lock, so a reader
                // that already got past the gate waits here instead of reading a half-written
                // revision.
                let _write_lock = self.data.state.transaction_lock_for_write(path).await;

                self.log_start(path).await;

                match self
                    .server
                    .process_asset_with(&*processor, settings, path, &meta_bytes)
                    .await
                {
                    Ok(processed_info) => {
                        self.log_finish(path).await;
                        Ok(ProcessResult::Processed(processed_info))
                    }
                    Err(error) => {
                        // A `start` with no `finish` is what tells the next run this one did not
                        // finish.
                        Err(error)
                    }
                }
            }
            Action::Copy { mut meta } => {
                let _write_lock = self.data.state.transaction_lock_for_write(path).await;

                self.log_start(path).await;

                let processed_writer = source.processed_writer()?;

                let bytes = source.reader().read_bytes(path.path()).await?;

                let mut writer = processed_writer.write(path.path()).await?;

                writer
                    .write_all_bytes(&bytes)
                    .await
                    .map_err(AssetWriterError::from)?;

                writer.flush().await.map_err(AssetWriterError::from)?;

                // The copied asset has no dependencies of its own: it is its own bytes.
                let processed_info = ProcessedInfo {
                    hash: source_hash,
                    full_hash: source_hash,
                    process_dependencies: Vec::new(),
                };

                // `process_dependencies` is empty, `clone` is cheap
                *meta.processed_info_mut() = Some(processed_info.clone());

                processed_writer
                    .write_meta_bytes(path.path(), &meta.serialize())
                    .await?;

                self.log_finish(path).await;

                Ok(ProcessResult::Processed(processed_info))
            }
        }
    }

    /// Returns whether the processed output of `path` still matches its source, per the index.
    ///
    /// `source_hash` is the hash of the source as it is now, read by the caller: this only asks
    /// whether the index still records that hash, and the hashes of the process dependencies that
    /// went into the output.
    async fn is_up_to_date(&self, path: &AssetPath<'static>, source_hash: AssetHash) -> bool {
        self.data
            .state
            .read_infos()
            .await
            .is_up_to_date(path, source_hash)
    }

    /// Decides what to do with one asset: process it, copy it, or leave it alone.
    ///
    /// A loader that a `.meta` names may only be pre-registered at this point, so resolving the
    /// action waits for it: the asset is then processed by whoever claimed it.
    async fn resolve_action(
        &self,
        path: &AssetPath<'static>,
        meta_bytes: &[u8],
    ) -> Result<Action, AssetProcessError> {
        // The importer's own server reads source `.meta` files whatever the app's server is
        // configured to do — [`AssetProcessServer::build`] builds it with `Always`, since a `.meta`
        // is how a source asset names its processor and its settings. A `Custom` (or `Never`) mode
        // could leave a `.meta` unread, which would silently pick the wrong processor — or none at
        // all — so this is asserted rather than merely documented: the branch below only ever asks
        // whether there *is* a meta, never whether it *should* be read.
        //
        // [`AssetProcessServer::build`]: AssetProcessServer::build
        debug_assert_matches!(
            self.server.asset_meta_check_mode(),
            AssetMetaCheckMode::Always,
            "the importer's server has to read every source `.meta` file",
        );

        if !meta_bytes.is_empty() {
            let minimal = AssetConfigMinimal::from_bytes(meta_bytes).map_err(|error| {
                ::core::hint::cold_path();
                let path = path.to_string().into_boxed_str();
                let error = AssetMetaParseError { path, error };
                AssetProcessError::from(AssetError::from(error))
            })?;

            match minimal.asset_config {
                AssetActionMinimal::Process { processor } => {
                    // A `.meta` names its processor by *type name*: the lenient form, which also
                    // accepts a fully-qualified type path, so the string can be used without being
                    // classified first.
                    let found = {
                        let processors = self.read_processors();
                        processors
                            .find(None, Some(&processor), Some(path))
                            .map_err(|error| {
                                ::core::hint::cold_path();
                                match error {
                                    Some(ambiguous) => AssetProcessError::from(ambiguous),
                                    None => AssetProcessError::from(MissingAssetProcessor::from(
                                        MissingBuilder::new().with_type_name(processor.clone()),
                                    )),
                                }
                            })?
                    };

                    let meta = found.deserialize_meta(meta_bytes).map_err(|error| {
                        ::core::hint::cold_path();
                        let path = path.to_string().into_boxed_str();
                        AssetProcessError::from(AssetMetaParseError { path, error })
                    })?;

                    return Ok(Action::Process {
                        processor: found,
                        settings_meta: meta,
                    });
                }
                // `Load` is a deliberate choice: the asset is not processed. It is still copied, so
                // that a processed source holds everything the app can load.
                AssetActionMinimal::Load { loader } => {
                    let error = || {
                        ::core::hint::cold_path();
                        AssetProcessError::from(MissingAssetLoader::from(
                            MissingBuilder::new().with_type_name(loader.clone()),
                        ))
                    };

                    // The registry lock is released before the await, like everywhere a loader is
                    // awaited out of it.
                    let entry = {
                        self.server
                            .0
                            .read_loaders()
                            .find(None, Some(&loader), None, Some(path))
                    };

                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(None) => return Err(error()),
                        Err(Some(ambiguous)) => {
                            ::core::hint::cold_path();
                            return Err(AssetProcessError::from(ambiguous));
                        }
                    };

                    let loader = match entry {
                        Ok(loader) => loader,
                        Err(pending) => pending.get().await.ok_or_else(error)?,
                    };

                    let meta = loader.deserialize_meta(meta_bytes).map_err(|error| {
                        ::core::hint::cold_path();
                        let path = path.to_string().into_boxed_str();
                        AssetProcessError::from(AssetMetaParseError { path, error })
                    })?;

                    return Ok(Action::Copy { meta });
                }
                AssetActionMinimal::Ignore => return Ok(Action::Ignore),
                AssetActionMinimal::None => {}
            }
        }

        // No `.meta` (or one that names nothing): the default processor for the extension first...
        // No type name is passed, so no name can be ambiguous: `find` can only report a missing
        // processor here.
        if let Ok(processor) = self.read_processors().find(None, None, Some(path)) {
            // Only the settings of this meta are used — the asset is processed with the processor
            // that was just found — so which processor the meta names does not matter here.
            let meta = processor.default_meta();

            return Ok(Action::Process {
                processor,
                settings_meta: meta,
            });
        }

        // ...then the loader the path resolves to, so the asset still reaches the processed side...
        match self.server.get_asset_loader_by_asset_path(path).await {
            Ok(loader) => {
                let meta = loader.default_meta();
                Ok(Action::Copy { meta })
            }
            // ...and nothing at all is a deliberate no-op.
            Err(_) => Ok(Action::Ignore),
        }
    }

    /// Writes the `.meta` an asset with none should have, next to its source.
    ///
    /// The `.meta` names the processor that handles the asset — by its extension — with default
    /// settings; when no processor handles it, the loader that reads the asset is named instead. See
    /// [`AssetProcessServer::write_default_meta`], which is the public form of this, including what
    /// `overwrite` means.
    pub(super) async fn write_default_meta_internal(
        &self,
        path: AssetPath<'_>,
        overwrite: bool,
    ) -> Result<(), AssetMetaWriteError> {
        // The registry lock is released before the first `await`.
        // No type name is passed, so no name can be ambiguous: `find` can only report a missing
        // processor here.
        let processor = {
            let processors = self.read_processors();
            processors.find(None, None, Some(&path)).ok()
        };

        if let Some(processor) = processor {
            let meta = processor.default_meta();
            self.server
                .write_meta_erased(&path, &*meta, overwrite)
                .await
        } else {
            self.server.write_default_meta(&path, overwrite).await
        }
    }
}
