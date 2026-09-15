#![expect(clippy::module_inception, reason = "For better structure.")]

use core::any::TypeId;
use core::future::Future;
use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use zlim_path::TypePath;

use crate::asset::Asset;
use crate::error::{AssetSaveError, MismatchedSettingsType};
use crate::io::Writer;
use crate::meta::{AssetConfig, AssetMeta, ErasedAssetMeta, Settings};
use crate::path::AssetPath;
use crate::saver::{ErasedSavedAsset, SavedAsset};
use crate::utils::BoxedFuture;

// -----------------------------------------------------------------------------
// AssetSaver

/// Writes a runtime [`Asset`] to bytes that can later be reloaded.
///
/// A saver has two jobs, kept apart on purpose:
///
/// - [`save`](Self::save) writes the *asset bytes* and nothing else: no `.meta`, no loader;
/// - [`build_settings`] reports the [`LoaderSettings`] those bytes have to be read back
///   with — the saver is the only one that knows what it just wrote.
///
/// Keeping them separate means the settings can be produced without writing anything (the
/// importer builds the processed `.meta` from them), and bytes can be written without naming
/// a loader (a plain save writes the asset alone).
///
/// [`build_settings`]: Self::build_settings
/// [`LoaderSettings`]: Self::LoaderSettings
pub trait AssetSaver: TypePath + Send + Sync + 'static {
    /// The top level [`Asset`] saved by this [`AssetSaver`].
    type Asset: Asset;

    /// The settings type used by this [`AssetSaver`].
    type Settings: Settings + Default + Serialize + for<'a> Deserialize<'a>;

    /// The settings the saved bytes are read back with: the `Settings` of whichever
    /// [`AssetLoader`] reads them.
    ///
    /// The saver names no loader of its own, so this type has to be the settings type of whichever
    /// loader the caller means; [`build_settings`] is what produces a value of it. A processor
    /// consumes that value: [`LoadTransformAndSave`] requires its output loader to have exactly this
    /// `Settings`, and writes what it gets back into the processed side's `.meta` next to that loader.
    /// A plain save writes the asset bytes alone — it never names a loader — so there the settings only
    /// matter when a `.meta` is written along with them.
    ///
    /// [`build_settings`]: Self::build_settings
    /// [`AssetLoader`]: crate::loader::AssetLoader
    /// [`LoadTransformAndSave`]: crate::processor::LoadTransformAndSave
    type LoaderSettings: Settings + Default + Serialize + for<'a> Deserialize<'a>;

    /// File extensions handled by this saver (without leading `.`).
    ///
    /// Returns an empty slice by default, which means the saver must be selected
    /// explicitly (e.g. via a `.meta` file) rather than by extension matching.
    const EXTENSIONS: &[&'static str] = &[];

    /// Writes `asset` to `writer` as the bytes a loader reads back.
    ///
    /// Bytes only: this returns nothing and names no loader. Callers that also need the settings the
    /// bytes are read back with call [`build_settings`](Self::build_settings) — a processor does
    /// both, and writes those settings into the processed side's `.meta`.
    fn save(
        &self,
        writer: &mut dyn Writer,
        path: &AssetPath<'static>,
        asset: SavedAsset<'_, Self::Asset>,
        settings: &Self::Settings,
    ) -> impl Future<Output = Result<(), AssetSaveError>> + Send;

    /// Returns the settings the bytes [`save`](Self::save) writes are read back with.
    ///
    /// This writes nothing, so a caller can build the `.meta` that names the loader before the bytes
    /// exist: `settings` are the saver's own, and the returned value is the
    /// [`LoaderSettings`](Self::LoaderSettings) of whichever loader the caller pairs it with.
    fn build_settings(
        &self,
        path: &AssetPath<'static>,
        asset: SavedAsset<'_, Self::Asset>,
        settings: &Self::Settings,
    ) -> impl Future<Output = Result<Self::LoaderSettings, AssetSaveError>> + Send;
}

// -----------------------------------------------------------------------------
// ErasedAssetSaver

/// A type-erased [`AssetSaver`], which is how the registry stores and shares savers.
///
/// It is implemented automatically for every [`AssetSaver`], so an implementation only has to
/// implement [`AssetSaver`] itself. Every method here is the counterpart of one there.
pub trait ErasedAssetSaver: Send + Sync + 'static {
    /// The [`TypeId`] of the underlying [`AssetSaver`].
    fn type_id(&self) -> TypeId;

    /// The fully-qualified type path of the underlying [`AssetSaver`].
    fn type_path(&self) -> &'static str;

    /// The short type name of the underlying [`AssetSaver`].
    ///
    /// Unlike [`type_path`](Self::type_path), a short name can be shared by several savers, so it only
    /// selects one while no other saver has it.
    fn type_name(&self) -> &'static str;

    /// The [`TypeId`] of the [`Asset`] the underlying [`AssetSaver`] saves.
    fn asset_type_id(&self) -> TypeId;

    /// The fully-qualified type path of the [`Asset`] the underlying [`AssetSaver`] saves.
    fn asset_type_path(&self) -> &'static str;

    /// The file extensions the underlying [`AssetSaver`] handles, without the leading `.`.
    fn extensions(&self) -> &[&str];

    /// Type-erased [`AssetSaver::save`]: writes the bytes alone, and returns nothing.
    ///
    /// `settings` must be the saver's own `Settings` when given; `None` uses `Default`. A value of
    /// another type — which only a registry bug can produce — **panics**, or returns
    /// [`MismatchedSettingsType`] when neither `debug_assertions` nor the `debug` feature is on.
    fn save<'a, 'b>(
        &'a self,
        writer: &'b mut dyn Writer,
        path: &'a AssetPath<'static>,
        asset: ErasedSavedAsset<'a>,
        settings: Option<&'a dyn Settings>,
    ) -> BoxedFuture<'b, Result<(), AssetSaveError>>
    where
        'a: 'b;

    /// Builds the `.meta` for the bytes this saver writes.
    ///
    /// Combines [`AssetSaver::build_settings`] with the loader name `loader`, producing a `Load`
    /// config: that loader reads the bytes back with exactly those settings. `settings` must be the
    /// saver's own `Settings` when given; `None` uses `Default` — and a value of another type
    /// **panics** here unconditionally, where [`save`](Self::save) reports it in a release build.
    ///
    /// The current implementation requires the loader name input, but it can be an empty string.
    /// When the string is empty, it is equivalent to nothing (skip serializing) — that is what a
    /// config which records no loader carries, so a caller that *has* a name must not pass the empty
    /// one: the `.meta` would then name no loader at all.
    fn build_meta<'a, 'b>(
        &'a self,
        path: &'a AssetPath<'static>,
        asset: ErasedSavedAsset<'a>,
        settings: Option<&'a dyn Settings>,
        loader: Cow<'static, str>,
    ) -> BoxedFuture<'b, Result<Box<dyn ErasedAssetMeta>, AssetSaveError>>
    where
        'a: 'b;
}

impl<S: AssetSaver> ErasedAssetSaver for S {
    fn type_id(&self) -> TypeId {
        TypeId::of::<S>()
    }

    fn type_path(&self) -> &'static str {
        <S as TypePath>::type_path()
    }

    fn type_name(&self) -> &'static str {
        <S as TypePath>::type_name()
    }

    fn asset_type_id(&self) -> TypeId {
        TypeId::of::<S::Asset>()
    }

    fn asset_type_path(&self) -> &'static str {
        <S::Asset as TypePath>::type_path()
    }

    fn extensions(&self) -> &[&str] {
        S::EXTENSIONS
    }

    fn save<'a, 'b>(
        &'a self,
        writer: &'b mut dyn Writer,
        path: &'a AssetPath<'static>,
        asset: ErasedSavedAsset<'a>,
        settings: Option<&'a dyn Settings>,
    ) -> BoxedFuture<'b, Result<(), AssetSaveError>>
    where
        'a: 'b,
    {
        #[cold]
        #[inline(never)]
        fn invalid_settings<T: ?Sized>() -> AssetSaveError {
            if cfg!(any(debug_assertions, feature = "debug")) {
                panic!("ErasedAssetSaver settings should match the saver settings type")
            } else {
                MismatchedSettingsType::from_source::<T>().into()
            }
        }

        Box::pin(async move {
            let default: S::Settings;

            let settings = if let Some(s) = settings {
                s.downcast_ref::<S::Settings>()
                    .ok_or_else(invalid_settings::<S>)?
            } else {
                default = S::Settings::default();
                &default
            };

            let asset = asset.with_type::<S::Asset>();

            <S as AssetSaver>::save(self, writer, path, asset, settings).await?;

            Ok(())
        })
    }

    fn build_meta<'a, 'b>(
        &'a self,
        path: &'a AssetPath<'static>,
        asset: ErasedSavedAsset<'a>,
        settings: Option<&'a dyn Settings>,
        loader: Cow<'static, str>,
    ) -> BoxedFuture<'b, Result<Box<dyn ErasedAssetMeta>, AssetSaveError>>
    where
        'a: 'b,
    {
        Box::pin(async move {
            let default: S::Settings;

            let settings = if let Some(s) = settings {
                s.downcast_ref::<S::Settings>()
                    .expect("AssetSaver settings should match the saver type")
            } else {
                default = S::Settings::default();
                &default
            };

            let asset = asset.with_type::<S::Asset>();

            let s = <S as AssetSaver>::build_settings(self, path, asset, settings).await?;

            let config = AssetConfig::<S::LoaderSettings, ()>::Load {
                loader,
                settings: s,
            };
            Ok(Box::new(AssetMeta::new(config)) as Box<dyn ErasedAssetMeta>)
        })
    }
}

// -----------------------------------------------------------------------------
// Placeholder

/// Placeholder implementation; should not be used as an [`AssetSaver`].
impl AssetSaver for () {
    type Asset = ();
    type Settings = ();
    type LoaderSettings = ();

    async fn save(
        &self,
        _writer: &mut dyn Writer,
        _path: &AssetPath<'static>,
        _asset: SavedAsset<'_, Self::Asset>,
        _settings: &Self::Settings,
    ) -> Result<(), AssetSaveError> {
        unreachable!("`()` is just a placeholder, not a valid `AssetSaver`")
    }

    async fn build_settings(
        &self,
        _path: &AssetPath<'static>,
        _asset: SavedAsset<'_, Self::Asset>,
        _settings: &Self::Settings,
    ) -> Result<Self::LoaderSettings, AssetSaveError> {
        unreachable!("`()` is just a placeholder, not a valid `AssetSaver`")
    }
}

// -----------------------------------------------------------------------------
