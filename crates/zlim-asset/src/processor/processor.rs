#![expect(clippy::module_inception, reason = "For better structure.")]

use core::any::TypeId;
use core::future::Future;
use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use zlim_path::TypePath;

use crate::error::{AssetProcessError, MismatchedSettingsType};
use crate::io::Writer;
use crate::loader::AssetLoader;
use crate::meta::{AssetConfig, AssetMeta, ErasedAssetMeta, MetaParseError, Settings};
use crate::processor::ProcessContext;
use crate::utils::BoxedFuture;

// -----------------------------------------------------------------------------
// AssetProcessor

/// Low-level asset processing trait: reads source bytes via [`ProcessContext`],
/// transforms them, and writes processed bytes to `writer`.
///
/// The output is whatever the [`AssetLoader`] on the other side reads back, so a processor names
/// that loader: it is what the processed asset's `.meta` records, together with the settings
/// [`process`](Self::process) returns.
///
/// [`AssetLoader`]: crate::loader::AssetLoader
pub trait AssetProcessor: TypePath + Send + Sync + Sized + 'static {
    /// The [`AssetLoader`] that reads the processed output back.
    type Loader: AssetLoader;

    /// The settings type used by this [`AssetProcessor`].
    type Settings: Settings + Default + Serialize + for<'a> Deserialize<'a>;

    /// Whether to use short type names when serializing.
    ///
    /// Defaults to `false`, which uses the full type path.
    ///
    /// This is about the name this processor writes: the one in the `.meta` files its
    /// [`default_meta`] produces. Both forms are read back through the same lenient look-up, so this
    /// only decides what a written `.meta` looks like — a short name is easier to read, a path always
    /// selects exactly one processor.
    ///
    /// [`default_meta`]: crate::processor::ErasedAssetProcessor::default_meta
    const SHORT_NAME: bool = false;

    /// The name this processor writes into the `.meta` files it creates.
    ///
    /// The short type name when [`SHORT_NAME`](Self::SHORT_NAME) is set, the fully-qualified type path
    /// otherwise — exactly what [`ErasedAssetProcessor::default_name`] returns and what its
    /// [`default_meta`](crate::processor::ErasedAssetProcessor::default_meta) records.
    ///
    /// It is an associated function rather than a method so that a caller that has only the type does
    /// not need an instance of the processor.
    ///
    /// [`ErasedAssetProcessor::default_name`]: crate::processor::ErasedAssetProcessor::default_name
    #[inline]
    fn default_name() -> &'static str {
        if Self::SHORT_NAME {
            <Self as TypePath>::type_name()
        } else {
            <Self as TypePath>::type_path()
        }
    }

    /// Processes the source asset and writes the result to `writer`.
    ///
    /// Returns the settings the [`AssetLoader`] needs when reading the processed output back;
    /// they are stored in the output's [`AssetMeta`].
    ///
    /// The arguments are ordered like [`AssetLoader::load`]: the output first, then the context the
    /// processor works through, then the settings it was configured with.
    ///
    /// [`AssetLoader::load`]: crate::loader::AssetLoader::load
    fn process(
        &self,
        writer: &mut dyn Writer,
        context: &mut ProcessContext<'_>,
        settings: &Self::Settings,
    ) -> impl Future<Output = Result<<Self::Loader as AssetLoader>::Settings, AssetProcessError>> + Send;
}

// -----------------------------------------------------------------------------
// ErasedAssetProcessor

/// A type-erased variant of [`AssetProcessor`].
///
/// Wrapper is created automatically for every [`AssetProcessor`], so an implementation only has to
/// implement [`AssetProcessor`]. Every method here is the counterpart of one there.
pub trait ErasedAssetProcessor: Send + Sync + 'static {
    /// The [`TypeId`] of the underlying [`AssetProcessor`].
    fn type_id(&self) -> TypeId;

    /// The fully-qualified type path of the underlying [`AssetProcessor`].
    fn type_path(&self) -> &'static str;

    /// The short type name of the underlying [`AssetProcessor`].
    ///
    /// Unlike [`type_path`](Self::type_path) a short name can be shared by several processors, so it
    /// only selects one while no other does.
    fn type_name(&self) -> &'static str;

    /// Type-erased variant of [`AssetProcessor::process`].
    ///
    /// The returned meta is the meta of the **processed output** (a `Load` config naming the
    /// processor's loader and the settings it has to be loaded with). The loader is named the way
    /// that loader names itself (see [`AssetLoader::SHORT_NAME`]).
    ///
    /// Takes `context` by value so that the caller can take the fields the future mutably
    /// borrowed back out of it once the future resolves.
    ///
    /// `settings` must be the settings of *this* processor: it is downcast to
    /// [`AssetProcessor::Settings`], and a value of another type — which only a registry bug can
    /// produce — **panics**, or returns [`MismatchedSettingsType`] when neither
    /// `debug_assertions` nor the `debug` feature is on.
    fn process<'a>(
        &'a self,
        writer: &'a mut dyn Writer,
        context: ProcessContext<'a>,
        settings: &'a dyn Settings,
    ) -> BoxedFuture<'a, Result<Box<dyn ErasedAssetMeta>, AssetProcessError>>;

    /// Returns the default processor name for the [`AssetProcessor`].
    ///
    /// This is [`AssetProcessor::default_name`]: [`type_name`](Self::type_name) when
    /// [`AssetProcessor::SHORT_NAME`] is true, [`type_path`](Self::type_path) otherwise. It is what
    /// the meta of [`default_meta`](Self::default_meta) names the processor by.
    fn default_name(&self) -> &'static str;

    /// Returns the default type-erased [`AssetMeta`] for this processor.
    ///
    /// It is a `Process` config with the processor's `Settings::default()` and the processor named by
    /// [`default_name`](Self::default_name): what an asset without a `.meta` file is processed with
    /// once the processor is made the default for its extension.
    fn default_meta(&self) -> Box<dyn ErasedAssetMeta>;

    /// Deserializes `meta` as a type-erased [`AssetMeta`] for this processor.
    ///
    /// The bytes are read as this processor's [`AssetProcessor::Settings`], so a `.meta` written for
    /// another processor is a parse error rather than a silently empty config.
    fn deserialize_meta(&self, meta: &[u8]) -> Result<Box<dyn ErasedAssetMeta>, MetaParseError>;
}

impl<T: AssetProcessor> ErasedAssetProcessor for T {
    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn type_path(&self) -> &'static str {
        <T as TypePath>::type_path()
    }

    fn type_name(&self) -> &'static str {
        <T as TypePath>::type_name()
    }

    fn process<'a>(
        &'a self,
        writer: &'a mut dyn Writer,
        mut context: ProcessContext<'a>,
        settings: &'a dyn Settings,
    ) -> BoxedFuture<'a, Result<Box<dyn ErasedAssetMeta>, AssetProcessError>> {
        #[cold]
        #[inline(never)]
        fn invalid_settings_type<T: ?Sized>() -> AssetProcessError {
            if cfg!(any(debug_assertions, feature = "debug")) {
                panic!("ErasedAssetProcessor settings should match the processor settings type")
            } else {
                MismatchedSettingsType::from_source::<T>().into()
            }
        }

        Box::pin(async move {
            // Settings of another type could only come from a registry bug: debug builds panic on
            // it, release builds report it.
            let settings = settings
                .downcast_ref::<T::Settings>()
                .ok_or_else(invalid_settings_type::<T>)?;

            let settings = AssetProcessor::process(self, writer, &mut context, settings).await?;

            // The output's `.meta` names the loader the way that loader names itself, so a config
            // reads the same whether the loader or the processor wrote it.
            let loader = <T::Loader as AssetLoader>::default_name();

            let config = AssetConfig::Load {
                loader: loader.into(),
                settings,
            };

            let meta: AssetMeta<<T::Loader as AssetLoader>::Settings, ()> = AssetMeta::new(config);

            Ok(Box::new(meta) as Box<dyn ErasedAssetMeta>)
        })
    }

    fn default_name(&self) -> &'static str {
        <T as AssetProcessor>::default_name()
    }

    fn default_meta(&self) -> Box<dyn ErasedAssetMeta> {
        // Must be a `Process` config.
        let config = AssetConfig::Process {
            processor: Cow::Borrowed(<T as AssetProcessor>::default_name()),
            settings: T::Settings::default(),
        };

        Box::new(AssetMeta::<(), T::Settings>::new(config))
    }

    fn deserialize_meta(&self, meta: &[u8]) -> Result<Box<dyn ErasedAssetMeta>, MetaParseError> {
        Ok(Box::new(AssetMeta::<(), T::Settings>::deserialize(meta)?))
    }
}

// -----------------------------------------------------------------------------
// Placeholder

/// Placeholder implementation, should not be used as [`AssetProcessor`].
impl AssetProcessor for () {
    type Loader = ();
    type Settings = ();

    async fn process(
        &self,
        _: &mut dyn Writer,
        _: &mut ProcessContext<'_>,
        _: &Self::Settings,
    ) -> Result<<Self::Loader as AssetLoader>::Settings, AssetProcessError> {
        unreachable!("`()` is just a placeholder, not a valid `AssetProcessor`")
    }
}
