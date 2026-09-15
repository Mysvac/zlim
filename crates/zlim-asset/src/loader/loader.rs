#![expect(clippy::module_inception, reason = "For better structure.")]

use core::any::TypeId;
use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use zlim_path::TypePath;

use crate::asset::Asset;
use crate::error::{AssetLoadError, MismatchedSettingsType};
use crate::io::Reader;
use crate::loaded::ErasedLoadedAsset;
use crate::loader::context::LoadContext;
use crate::meta::{AssetConfig, AssetMeta, ErasedAssetMeta, MetaParseError, Settings};
use crate::utils::BoxedFuture;

// -----------------------------------------------------------------------------
// AssetLoader

/// A file-format plugin that deserialises raw bytes into an [`Asset`].
///
/// A loader is the read half of a format: it is picked for a path by its [`EXTENSIONS`], or named by
/// a `.meta` file, and turns the bytes a [`Reader`] hands it into an asset — usually through
/// [`LoadContext`], which is also how it produces labeled sub-assets and records the dependencies of
/// what it read.
///
/// The writing half is a separate trait, [`AssetSaver`]; a [`LoadTransformAndSave`] pipeline pairs
/// the two when a source asset has to become a processed one.
///
/// [`EXTENSIONS`]: Self::EXTENSIONS
/// [`AssetSaver`]: crate::saver::AssetSaver
/// [`LoadTransformAndSave`]: crate::processor::LoadTransformAndSave
pub trait AssetLoader: TypePath + Send + Sync + 'static {
    /// The asset type produced by this loader.
    ///
    /// A loader produces exactly one type; several loaders may produce the same one, and then the
    /// newest registration is the one an asset-type look-up picks.
    type Asset: Asset;

    /// Per-asset configuration; stored in `.meta` files next to the asset.
    ///
    /// The `.meta` of an asset is read back with this type, so changing it invalidates the `.meta`
    /// files this loader wrote; `Default` is what a load uses when no `.meta` is read at all.
    type Settings: Settings + Default + Serialize + for<'d> Deserialize<'d>;

    /// Whether to use short type names when serializing.
    ///
    /// Defaults to `false`, which uses the full type path.
    ///
    /// This is the name this loader writes into the `.meta` files it creates (see
    /// [`default_name`](crate::loader::ErasedAssetLoader::default_name)); both forms are read back
    /// through the same lenient look-up, so it only decides what a written `.meta` looks like.
    ///
    /// Third-party crates should normally leave this `false` to avoid name collisions,
    /// while internal crates may set it to `true` for shorter, simpler config names
    const SHORT_NAME: bool = false;

    /// The name this loader writes into the `.meta` files it creates.
    ///
    /// The short type name when [`SHORT_NAME`](Self::SHORT_NAME) is set, the fully-qualified type path
    /// otherwise — exactly what [`ErasedAssetLoader::default_name`] returns and
    /// [`ErasedAssetLoader::default_meta`] records.
    ///
    /// It is an associated function rather than a method so that a caller that has only the type — a
    /// save builder recording the loader a written `.meta` should name, say — does not need an
    /// instance of the loader.
    ///
    /// [`ErasedAssetLoader::default_name`]: crate::loader::ErasedAssetLoader::default_name
    /// [`ErasedAssetLoader::default_meta`]: crate::loader::ErasedAssetLoader::default_meta
    #[inline]
    fn default_name() -> &'static str {
        if Self::SHORT_NAME {
            <Self as TypePath>::type_name()
        } else {
            <Self as TypePath>::type_path()
        }
    }

    /// File extensions handled by this loader (without leading `.`).
    ///
    /// Returns an empty slice by default, which means the loader must be selected
    /// explicitly (e.g. via a `.meta` file) rather than by extension matching.
    ///
    /// An extension may be claimed by several loaders: the most recently registered one still wins
    /// for it, and naming a loader in a `.meta` file is the only way to force a particular one. A
    /// collision only logs a warning when the second claimant produces the same asset type.
    const EXTENSIONS: &[&'static str] = &[];

    /// Asynchronously loads [`AssetLoader::Asset`] (and any other labeled
    /// assets) from the bytes provided by [`Reader`].
    ///
    /// `settings` are the ones the resolved `.meta` carries, or the loader's `Settings::default()`
    /// when there is no `.meta`. `context` is what the load is recorded through: finishing it is what
    /// turns the returned asset into the result of the load, together with the sub-assets and the
    /// dependencies the implementation registered on the way.
    ///
    /// A failed load is reported as [`AssetLoadError`]. A panic inside the implementation does not
    /// escape either: the server catches it and reports the asset as having failed to load.
    fn load(
        &self,
        reader: &mut dyn Reader,
        context: &mut LoadContext,
        settings: &Self::Settings,
    ) -> impl Future<Output = Result<Self::Asset, AssetLoadError>> + Send;
}

// -----------------------------------------------------------------------------
// ErasedAssetLoader

/// A type-erased [`AssetLoader`], which is how the registry stores and shares loaders.
///
/// It is implemented automatically for every [`AssetLoader`], so an implementation only has to
/// implement [`AssetLoader`] itself. Every method here is the counterpart of one there.
pub trait ErasedAssetLoader: Send + Sync + 'static {
    /// The [`TypeId`] of the underlying [`AssetLoader`].
    fn type_id(&self) -> TypeId;

    /// The fully-qualified type path of the underlying [`AssetLoader`].
    fn type_path(&self) -> &'static str;

    /// The short type name of the underlying [`AssetLoader`].
    ///
    /// Unlike [`type_path`](Self::type_path), a short name can be shared by several loaders, so it
    /// only selects one while no other loader has it.
    fn type_name(&self) -> &'static str;

    /// The [`TypeId`] of the [`Asset`] the underlying [`AssetLoader`] produces.
    fn asset_type_id(&self) -> TypeId;

    /// The fully-qualified type path of the [`Asset`] the underlying [`AssetLoader`] produces.
    fn asset_type_path(&self) -> &'static str;

    /// The file extensions the underlying [`AssetLoader`] handles, without the leading `.`.
    fn extensions(&self) -> &[&str];

    /// Type-erased [`AssetLoader::load`].
    ///
    /// Takes `context` by value so that the caller can take the fields the future mutably
    /// borrowed back out of it once the future resolves.
    ///
    /// `settings` must be the settings of *this* loader: it is downcast to [`AssetLoader::Settings`],
    /// and a value of another type — which only a registry bug can produce — **panics**, or returns
    /// [`MismatchedSettingsType`] when neither `debug_assertions` nor the `debug` feature is on.
    fn load<'a>(
        &'a self,
        reader: &'a mut dyn Reader,
        context: LoadContext<'a>,
        settings: &'a dyn Settings,
    ) -> BoxedFuture<'a, Result<ErasedLoadedAsset, AssetLoadError>>;

    /// Returns the default loader name for the [`AssetLoader`].
    ///
    /// If [`AssetLoader::SHORT_NAME`] is true, this is [`type_name`](Self::type_name), otherwise this
    /// is [`type_path`](Self::type_path). It is what [`default_meta`](Self::default_meta) writes into
    /// the `.meta` files this loader creates.
    fn default_name(&self) -> &'static str;

    /// Returns the default meta value for the [`AssetLoader`].
    ///
    /// A `Load` config naming this loader — [`default_name`](Self::default_name) — with the loader's
    /// `Settings::default()`. This is what a load uses when the asset has no `.meta` of its own, and
    /// what writing a new `.meta` starts from.
    ///
    /// This function must return a `Load` meta, otherwise some methods will panic.
    fn default_meta(&self) -> Box<dyn ErasedAssetMeta>;

    /// Deserializes metadata from the input `meta` bytes into the appropriate type.
    ///
    /// The bytes are read as this loader's [`AssetLoader::Settings`], so a `.meta` written for another
    /// loader is a parse error rather than a silently defaulted config.
    fn deserialize_meta(&self, meta: &[u8]) -> Result<Box<dyn ErasedAssetMeta>, MetaParseError>;
}

impl<L: AssetLoader> ErasedAssetLoader for L {
    fn type_id(&self) -> TypeId {
        TypeId::of::<L>()
    }

    fn type_path(&self) -> &'static str {
        <L as TypePath>::type_path()
    }

    fn type_name(&self) -> &'static str {
        <L as TypePath>::type_name()
    }

    fn asset_type_id(&self) -> TypeId {
        TypeId::of::<L::Asset>()
    }

    fn asset_type_path(&self) -> &'static str {
        <L::Asset as TypePath>::type_path()
    }

    fn extensions(&self) -> &[&str] {
        L::EXTENSIONS
    }

    fn load<'a>(
        &'a self,
        reader: &'a mut dyn Reader,
        mut context: LoadContext<'a>,
        settings: &'a dyn Settings,
    ) -> BoxedFuture<'a, Result<ErasedLoadedAsset, AssetLoadError>> {
        #[cold]
        #[inline(never)]
        fn invalid_settings_type<T: ?Sized>() -> AssetLoadError {
            if cfg!(any(debug_assertions, feature = "debug")) {
                panic!("ErasedAssetLoader settings should match the loader settings type")
            } else {
                MismatchedSettingsType::from_source::<T>().into()
            }
        }

        Box::pin(async move {
            let settings = settings
                .downcast_ref::<L::Settings>()
                .ok_or_else(invalid_settings_type::<L>)?;

            let asset = <L as AssetLoader>::load(self, reader, &mut context, settings).await?;

            Ok(context.finish(asset).erased())
        })
    }

    fn default_name(&self) -> &'static str {
        <L as AssetLoader>::default_name()
    }

    fn default_meta(&self) -> Box<dyn ErasedAssetMeta> {
        // Must be a `Load` AssetConfig.
        let config = AssetConfig::Load {
            loader: Cow::Borrowed(<L as AssetLoader>::default_name()),
            settings: L::Settings::default(),
        };
        Box::new(AssetMeta::<L::Settings, ()>::new(config))
    }

    fn deserialize_meta(&self, meta: &[u8]) -> Result<Box<dyn ErasedAssetMeta>, MetaParseError> {
        Ok(Box::new(AssetMeta::<L::Settings, ()>::deserialize(meta)?))
    }
}

// -----------------------------------------------------------------------------
// Placeholder

/// Placeholder implementation, should not be used as an [`AssetLoader`].
impl AssetLoader for () {
    type Asset = ();
    type Settings = ();
    const EXTENSIONS: &[&'static str] = &[];

    async fn load(
        &self,
        _: &mut dyn Reader,
        _: &mut LoadContext<'_>,
        _: &Self::Settings,
    ) -> Result<Self::Asset, AssetLoadError> {
        unreachable!("`()` is just a placeholder, not a valid `AssetLoader`")
    }
}
