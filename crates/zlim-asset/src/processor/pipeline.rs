//! The ready-made processor: load, transform and save, plus the settings each of the three steps
//! takes.

use core::marker::PhantomData;

use serde::{Deserialize, Serialize};
use zlim_path::TypePath;

use super::{AssetProcessor, ProcessContext};
use crate::error::AssetProcessError;
use crate::io::Writer;
use crate::loaded::LoadedAsset;
use crate::loader::AssetLoader;
use crate::saver::{AssetSaver, SavedAsset};
use crate::transformer::{AssetTransformer, IdentityTransformer, TransformedAsset};

// -----------------------------------------------------------------------------
// LoadTransformAndSave

/// A high-level [`AssetProcessor`] that:
/// 1. loads the source asset with loader `L`,
/// 2. transforms it with transformer `T`,
/// 3. saves the result with saver `S`, whose [`build_settings`] supplies the settings the
///    output loader reads those bytes back with.
///
/// `Out` is the [`AssetLoader`] that reads the saved bytes back. It is a parameter of its own
/// rather than something read off the saver: an [`AssetSaver`] writes its output in terms of a
/// [`LoaderSettings`] *type*, and only the pipeline knows which loader those settings belong to
/// (a plain save names no loader at all).
///
/// Use [`IdentityTransformer`] as `T` when the step is a plain load-then-save (format
/// conversion).
///
/// [`AssetProcessor`]: crate::processor::AssetProcessor
/// [`AssetLoader`]: crate::loader::AssetLoader
/// [`AssetSaver`]: crate::saver::AssetSaver
/// [`IdentityTransformer`]: crate::transformer::IdentityTransformer
/// [`build_settings`]: AssetSaver::build_settings
/// [`LoaderSettings`]: AssetSaver::LoaderSettings
#[derive(TypePath)]
#[doc(alias = "AssetPipeline")]
#[doc(alias = "StandardAssetProcessor")]
pub struct LoadTransformAndSave<L, T, S, Out>
where
    L: AssetLoader,
    T: AssetTransformer<AssetInput = L::Asset>,
    S: AssetSaver<Asset = T::AssetOutput>,
    Out: AssetLoader<Asset = S::Asset, Settings = S::LoaderSettings>,
{
    transformer: T,
    saver: S,
    marker: PhantomData<fn() -> (L, Out)>,
}

impl<L, S, Out> From<S> for LoadTransformAndSave<L, IdentityTransformer<L::Asset>, S, Out>
where
    L: AssetLoader,
    S: AssetSaver<Asset = L::Asset>,
    Out: AssetLoader<Asset = S::Asset, Settings = S::LoaderSettings>,
{
    #[inline]
    fn from(saver: S) -> Self {
        Self::new(IdentityTransformer::new(), saver)
    }
}

impl<L, T, S, Out> LoadTransformAndSave<L, T, S, Out>
where
    L: AssetLoader,
    T: AssetTransformer<AssetInput = L::Asset>,
    S: AssetSaver<Asset = T::AssetOutput>,
    Out: AssetLoader<Asset = S::Asset, Settings = S::LoaderSettings>,
{
    /// Creates a new [`LoadTransformAndSave`] with the given transformer and saver.
    #[inline]
    pub const fn new(transformer: T, saver: S) -> Self {
        Self {
            transformer,
            saver,
            marker: PhantomData,
        }
    }
}

// -----------------------------------------------------------------------------
// LoadTransformAndSaveSettings

/// Settings for [`LoadTransformAndSave`].
///
/// One field per step, each forwarded to the loader / transformer / saver it belongs to.
/// These are what a `.meta` file carries for this processor (`AssetConfig::Process { settings }`).
#[derive(Serialize, Deserialize, Default)]
pub struct LoadTransformAndSaveSettings<LoaderSettings, TransformerSettings, SaverSettings> {
    /// Settings forwarded to the [`AssetLoader`].
    pub loader_settings: LoaderSettings,
    /// Settings forwarded to the [`AssetTransformer`].
    pub transformer_settings: TransformerSettings,
    /// Settings forwarded to the [`AssetSaver`].
    pub saver_settings: SaverSettings,
}

// -----------------------------------------------------------------------------
// AssetProcessor impl for LoadTransformAndSave

impl<L, T, S, Out> AssetProcessor for LoadTransformAndSave<L, T, S, Out>
where
    L: AssetLoader,
    T: AssetTransformer<AssetInput = L::Asset>,
    S: AssetSaver<Asset = T::AssetOutput>,
    Out: AssetLoader<Asset = S::Asset, Settings = S::LoaderSettings>,
{
    type Loader = Out;
    type Settings = LoadTransformAndSaveSettings<L::Settings, T::Settings, S::Settings>;

    async fn process(
        &self,
        writer: &mut dyn Writer,
        context: &mut ProcessContext<'_>,
        settings: &Self::Settings,
    ) -> Result<<Self::Loader as AssetLoader>::Settings, AssetProcessError> {
        let loaded = context
            .load_source_asset::<L>(&settings.loader_settings)
            .await?;

        // The source was loaded with `L`, so its value is an `L::Asset`; anything else means the
        // loader broke its own contract.
        let loaded: LoadedAsset<L::Asset> = loaded.with_type();

        let asset = TransformedAsset::<L::Asset>::from_loaded(loaded);
        let transformed = self
            .transformer
            .transform(asset, &settings.transformer_settings)
            .await?;

        // The saver's two halves are called separately: `build_settings` reports the settings the
        // processed bytes are read back with, `save` writes the bytes themselves. The settings are
        // therefore known before anything is written — the meta of the processed output is built
        // from them by the driver, which also records the bytes.
        let path = context.path();
        let asset = SavedAsset::<T::AssetOutput>::from_transformed(&transformed);

        let loader_settings = self
            .saver
            .build_settings(path, asset.clone(), &settings.saver_settings)
            .await?;

        self.saver
            .save(writer, path, asset, &settings.saver_settings)
            .await?;

        // What the saver reports is exactly what the output loader needs: the pipeline requires
        // `Out::Settings == S::LoaderSettings`, so this is checked at compile time.
        Ok(loader_settings)
    }
}
