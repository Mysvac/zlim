#![expect(clippy::module_inception, reason = "For better structure.")]

use core::any::TypeId;
use core::future::Future;
use core::marker::PhantomData;

use serde::{Deserialize, Serialize};
use zlim_path::TypePath;
use zlim_utils::hash::{HashMap, HashSet};

use super::transformed::TransformedAsset;
use crate::asset::Asset;
use crate::error::{AssetTransformError, MismatchedSettingsType};
use crate::loaded::ErasedLoadedAsset;
use crate::meta::Settings;
use crate::utils::BoxedFuture;

// -----------------------------------------------------------------------------
// AssetTransformer

/// Transforms an [`Asset`] of [`AssetInput`] into [`AssetOutput`].
///
/// A transformer is never named by a `.meta` file: the pipeline that runs
/// it chooses it in code, so unlike a loader, a saver or a processor it has
/// no name to write and nothing to disambiguate.
///
/// [`AssetInput`]: AssetTransformer::AssetInput
/// [`AssetOutput`]: AssetTransformer::AssetOutput
pub trait AssetTransformer: TypePath + Send + Sync + 'static {
    /// The [`Asset`] type which this [`AssetTransformer`] inputs.
    type AssetInput: Asset;

    /// The [`Asset`] type which this [`AssetTransformer`] outputs.
    ///
    /// It is the type the pipeline's loader reads back, which is what
    /// pairs a transformer with the saver that writes its output.
    type AssetOutput: Asset;

    /// The settings type used by this [`AssetTransformer`].
    ///
    /// The pipeline passes them in. Nothing names a transformer in a `.meta`, so they have no entry
    /// of their own there: they are recorded only as part of the settings of the processor that
    /// runs the transformer (see [`LoadTransformAndSaveSettings`]).
    ///
    /// [`LoadTransformAndSaveSettings`]: crate::processor::LoadTransformAndSaveSettings
    type Settings: Settings + Default + Serialize + for<'a> Deserialize<'a>;

    /// Transforms `asset` (and its labeled sub-assets) into [`Self::AssetOutput`].
    fn transform<'a>(
        &'a self,
        asset: TransformedAsset<Self::AssetInput>,
        settings: &'a Self::Settings,
    ) -> impl Future<Output = Result<TransformedAsset<Self::AssetOutput>, AssetTransformError>> + Send;
}

// -----------------------------------------------------------------------------
// ErasedAssetTransformer

/// A type-erased [`AssetTransformer`].
///
/// It is implemented automatically for every [`AssetTransformer`],
/// so an implementation only has to implement [`AssetTransformer`]
/// itself. Every method here is the counterpart of one there.
pub trait ErasedAssetTransformer: Send + Sync + 'static {
    /// The [`TypeId`] of the underlying [`AssetTransformer`].
    fn type_id(&self) -> TypeId;

    /// The fully-qualified type path of the underlying [`AssetTransformer`].
    fn type_path(&self) -> &'static str;

    /// Type-erased variant of [`AssetTransformer::transform`].
    ///
    /// The returned asset starts with empty dependency sets:
    /// a [`TransformedAsset`] does not carry the load dependencies
    /// of its source, so they cannot be preserved here.
    ///
    /// `settings` must be the settings of *this* transformer:
    /// it is downcast to [`AssetTransformer::Settings`], and a value of
    /// another type — which only a pipeline bug can produce — **panics**,
    /// or returns [`MismatchedSettingsType`] when neither `debug_assertions`
    /// nor the `debug` feature is on.
    fn transform<'a>(
        &'a self,
        asset: ErasedLoadedAsset,
        settings: &'a dyn Settings,
    ) -> BoxedFuture<'a, Result<ErasedLoadedAsset, AssetTransformError>>;
}

impl<T: AssetTransformer> ErasedAssetTransformer for T {
    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn type_path(&self) -> &'static str {
        <T as TypePath>::type_path()
    }

    fn transform<'a>(
        &'a self,
        asset: ErasedLoadedAsset,
        settings: &'a dyn Settings,
    ) -> BoxedFuture<'a, Result<ErasedLoadedAsset, AssetTransformError>> {
        #[cold]
        #[inline(never)]
        fn invalid_settings_type<T: ?Sized>() -> AssetTransformError {
            if cfg!(any(debug_assertions, feature = "debug")) {
                panic!("ErasedAssetTransformer settings should match the transformer settings type")
            } else {
                MismatchedSettingsType::from_source::<T>().into()
            }
        }

        Box::pin(async move {
            let settings = settings
                .downcast_ref::<T::Settings>()
                .ok_or_else(invalid_settings_type::<T>)?;

            let asset = asset.with_type::<T::AssetInput>();
            let asset = TransformedAsset::<T::AssetInput>::from_loaded(asset);

            let transformed = <T as AssetTransformer>::transform(self, asset, settings).await?;

            Ok(ErasedLoadedAsset {
                value: Box::new(transformed.value),
                dependencies: HashSet::new(),
                labeled_assets: transformed.labeled_assets,
                label_to_label_index: transformed.label_to_label_index,
                asset_to_label_index: transformed.asset_to_label_index,
                loader_dependencies: HashMap::new(),
            })
        })
    }
}

// -----------------------------------------------------------------------------
// IdentityTransformer

/// An [`AssetTransformer`] that returns the input asset unchanged.
///
/// Useful for format-conversion pipelines where no runtime transformation is needed.
#[derive(TypePath)]
#[type_path = "zlim_asset::transformer::IdentityTransformer"]
#[doc(alias = "IdentityAssetTransformer")]
pub struct IdentityTransformer<A: Asset> {
    _marker: PhantomData<fn(A) -> A>,
}

impl<A: Asset> IdentityTransformer<A> {
    /// Creates a new `IdentityTransformer`.
    pub const fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<A: Asset> Default for IdentityTransformer<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: Asset> AssetTransformer for IdentityTransformer<A> {
    type AssetInput = A;
    type AssetOutput = A;
    type Settings = ();

    async fn transform(
        &self,
        asset: TransformedAsset<Self::AssetInput>,
        _settings: &Self::Settings,
    ) -> Result<TransformedAsset<Self::AssetOutput>, AssetTransformError> {
        Ok(asset)
    }
}

// -----------------------------------------------------------------------------
// Placeholder

/// Placeholder implementation; should not be used as an [`AssetTransformer`].
impl AssetTransformer for () {
    type AssetInput = ();
    type AssetOutput = ();
    type Settings = ();

    async fn transform(
        &self,
        _: TransformedAsset<()>,
        _: &Self::Settings,
    ) -> Result<TransformedAsset<()>, AssetTransformError> {
        unreachable!("`()` is just a placeholder, not a valid `AssetTransformer`")
    }
}
