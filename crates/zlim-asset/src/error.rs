//! The asset system's error surface.
//!
//! Every phase of the pipeline reports through a wrapper that names the phase —
//! [`AssetLoadError`], [`AssetSaveError`], [`AssetTransformError`] and [`AssetProcessError`] — and
//! each wrapper shares a single [`AssetError`] behind an [`Arc`]. [`AssetError`] is the hub: it
//! carries the error type of whichever subsystem failed (IO, loader, transformer, saver, meta
//! files, path approval, the processor, ...), and the `From` impls let those errors be raised with
//! `?`.
//!
//! The per-subsystem error types are either defined in this module or re-exported from the module
//! that owns them, so `crate::error` is the single path a call site needs.

use std::sync::Arc;

use zlim_core::derive::Error;
use zlim_core::error::ZlimError;

// -----------------------------------------------------------------------------
// Phase Wrapper Errors

/// An error from the load phase: the asset could not be read or decoded.
///
/// The wrapped [`AssetError`] is shared through [`Arc`], so reporting the same failure from several
/// places does not duplicate the message.
#[derive(Error, Debug, Clone)]
#[error("An error occurred while loading an asset: {_0}")]
pub struct AssetLoadError(pub Arc<AssetError>);

/// An error from the save phase: the asset could not be written back.
///
/// The wrapped [`AssetError`] is shared through [`Arc`], so reporting the same failure from several
/// places does not duplicate the message.
#[derive(Error, Debug, Clone)]
#[error("An error occurred while saving an asset: {_0}")]
pub struct AssetSaveError(pub Arc<AssetError>);

/// An error from the transform phase: the loaded asset could not be turned into its final form.
///
/// The wrapped [`AssetError`] is shared through [`Arc`], so reporting the same failure from several
/// places does not duplicate the message.
#[derive(Error, Debug, Clone)]
#[error("An error occurred while transforming an asset: {_0}")]
pub struct AssetTransformError(pub Arc<AssetError>);

/// An error from the process phase: the asset could not be processed into its processed form.
///
/// The wrapped [`AssetError`] is shared through [`Arc`], so reporting the same failure from several
/// places does not duplicate the message.
#[derive(Error, Debug, Clone)]
#[error("An error occurred while processing an asset: {_0}")]
pub struct AssetProcessError(pub Arc<AssetError>);

// -----------------------------------------------------------------------------
// Compile-time Assertions

const _ASSERT_: () = {
    const fn type_safe<T: Clone + Send + Sync>() {}
    type_safe::<AssetLoadError>();
    type_safe::<AssetSaveError>();
    type_safe::<AssetTransformError>();
    type_safe::<AssetProcessError>();
};

// -----------------------------------------------------------------------------
// Phase Wrapper Conversions

macro_rules! impl_from_error_t {
    ($ident:ident) => {
        impl<T> From<T> for $ident
        where
            AssetError: From<T>,
        {
            #[cold]
            fn from(value: T) -> Self {
                $ident::from(Arc::new(AssetError::from(value)))
            }
        }
    };
}

impl_from_error_t!(AssetLoadError);
impl_from_error_t!(AssetSaveError);
impl_from_error_t!(AssetTransformError);
impl_from_error_t!(AssetProcessError);

macro_rules! impl_from_into_arc {
    ($ident:ident) => {
        impl From<$ident> for Arc<AssetError> {
            #[inline(always)]
            fn from(value: $ident) -> Self {
                value.0
            }
        }

        impl From<Arc<AssetError>> for $ident {
            #[inline(always)]
            fn from(value: Arc<AssetError>) -> Self {
                $ident(value)
            }
        }
    };
}

impl_from_into_arc!(AssetLoadError);
impl_from_into_arc!(AssetSaveError);
impl_from_into_arc!(AssetTransformError);
impl_from_into_arc!(AssetProcessError);

impl From<AssetLoadError> for AssetProcessError {
    #[cold]
    #[inline(always)]
    fn from(value: AssetLoadError) -> Self {
        Self(value.0)
    }
}

impl From<AssetSaveError> for AssetProcessError {
    #[cold]
    #[inline(always)]
    fn from(value: AssetSaveError) -> Self {
        Self(value.0)
    }
}

impl From<AssetTransformError> for AssetProcessError {
    #[cold]
    #[inline(always)]
    fn from(value: AssetTransformError) -> Self {
        Self(value.0)
    }
}

// -----------------------------------------------------------------------------
// AssetError

/// The shared, type-erased error of the asset system.
///
/// A fallible step wraps whatever its subsystem reported into this enum, and the phase wrappers
/// ([`AssetLoadError`], [`AssetSaveError`], [`AssetTransformError`], [`AssetProcessError`]) name
/// the phase it happened in. Every variant forwards `Display` to the error it carries, so a
/// message never loses the origin the subsystem gave it.
///
/// The enum is `#[non_exhaustive]`: a new subsystem brings a new variant, and a downstream `match`
/// has to stay valid when it does.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum AssetError {
    /// A free-form message from a call site that has no dedicated error type.
    #[error(transparent)]
    Custom(String),

    /// An error from the wider engine, reported as [`ZlimError`].
    #[error(transparent)]
    ZlimError(ZlimError),

    /// An asset source could not be read.
    #[error(transparent)]
    AssetReaderError(AssetReaderError),
    /// An asset source could not be written.
    #[error(transparent)]
    AssetWriterError(AssetWriterError),

    /// An [`AssetLoader`](crate::loader::AssetLoader) reported a failure.
    #[error(transparent)]
    AssetLoaderError(AssetLoaderError),
    /// An [`AssetLoader`](crate::loader::AssetLoader) panicked.
    #[error(transparent)]
    AssetLoaderPanic(AssetLoaderPanic),
    /// An [`AssetTransformer`](crate::transformer::AssetTransformer) reported a failure.
    #[error(transparent)]
    AssetTransformerError(AssetTransformerError),
    /// An [`AssetTransformer`](crate::transformer::AssetTransformer) panicked.
    #[error(transparent)]
    AssetTransformerPanic(AssetTransformerPanic),
    /// An [`AssetSaver`](crate::saver::AssetSaver) reported a failure.
    #[error(transparent)]
    AssetSaverError(AssetSaverError),
    /// An [`AssetSaver`](crate::saver::AssetSaver) panicked.
    #[error(transparent)]
    AssetSaverPanic(AssetSaverPanic),

    /// An asset's `.meta` data could not be deserialized.
    #[error(transparent)]
    MetaParseError(MetaParseError),
    /// The `.meta` of a specific asset could not be deserialized.
    #[error(transparent)]
    AssetMetaParseError(AssetMetaParseError),

    /// A short type name selects several registered types.
    #[error(transparent)]
    AmbiguousName(AmbiguousName),

    /// No asset source is registered under the requested id.
    #[error(transparent)]
    MissingAssetSource(MissingAssetSource),
    /// The requested asset source has no writer.
    #[error(transparent)]
    MissingAssetWriter(MissingAssetWriter),
    /// The requested asset source has no processed reader.
    #[error(transparent)]
    MissingProcessedAssetReader(MissingProcessedAssetReader),
    /// The requested asset source has no processed writer.
    #[error(transparent)]
    MissingProcessedAssetWriter(MissingProcessedAssetWriter),
    /// No asset loader is registered for the extension or name that was looked up.
    #[error(transparent)]
    MissingAssetLoader(MissingAssetLoader),
    /// No asset transformer is registered for the name that was looked up.
    #[error(transparent)]
    MissingAssetTransformer(MissingAssetTransformer),
    /// No asset saver is registered for the name that was looked up.
    #[error(transparent)]
    MissingAssetSaver(MissingAssetSaver),
    /// No asset processor is registered for the extension or name that was looked up.
    #[error(transparent)]
    MissingAssetProcessor(MissingAssetProcessor),
    /// The asset type has no registered handle provider.
    #[error(transparent)]
    MissingHandleProvider(MissingHandleProvider),
    /// The loaded asset does not contain the requested label.
    #[error(transparent)]
    MissingLabeledAsset(MissingLabeledAsset),

    /// An empty asset path was read or written.
    #[error(transparent)]
    EmptyPathError(EmptyPathError),
    /// The asset path escapes its source root and the approval mode rejected it.
    #[error(transparent)]
    UnapprovedPath(UnapprovedPath),
    /// An asset without an extension was processed.
    #[error(transparent)]
    ExtensionRequired(ExtensionRequired),

    /// An asset configured to be ignored was loaded directly.
    #[error(transparent)]
    CannotLoadIgnoredAsset(CannotLoadIgnoredAsset),
    /// An asset configured to be processed was loaded directly.
    #[error(transparent)]
    CannotLoadProcessedAsset(CannotLoadProcessedAsset),

    /// Settings of another type were passed into a loader or a processor.
    #[error(transparent)]
    MismatchedSettingsType(MismatchedSettingsType),

    /// An asset's `.meta` file could not be read.
    #[error(transparent)]
    AssetMetaReadError(AssetMetaReadError),
    /// An asset's `.meta` file could not be written.
    #[error(transparent)]
    AssetMetaWriteError(AssetMetaWriteError),

    /// Waiting for an asset to finish loading failed.
    #[error(transparent)]
    WaitForAssetError(WaitForAssetError),

    /// An immediate nested load failed.
    #[error(transparent)]
    LoadDirectError(LoadDirectError),
    /// Reading raw bytes through a [`LoadContext`](crate::loader::LoadContext) failed.
    #[error(transparent)]
    ReadAssetBytesError(ReadAssetBytesError),

    /// The requested handle type does not match the loaded asset type.
    #[error(transparent)]
    RequestedHandleTypeMismatch(RequestedHandleTypeMismatch),
}

// -----------------------------------------------------------------------------
// AssetError From impls

macro_rules! impl_into_asset_error {
    ($ident:ident) => {
        impl From<$ident> for AssetError {
            #[cold]
            #[inline]
            fn from(value: $ident) -> Self {
                Self::$ident(value)
            }
        }
    };
}

// -----------------------------------------------------------------------------
// String & ZlimError

impl From<String> for AssetError {
    #[cold]
    #[inline]
    fn from(value: String) -> Self {
        Self::Custom(value)
    }
}

impl From<ZlimError> for AssetError {
    #[cold]
    #[inline]
    fn from(value: ZlimError) -> Self {
        Self::ZlimError(value)
    }
}

// -----------------------------------------------------------------------------
// IO: AssetReaderError & AssetWriterError

pub use crate::io::reader::AssetReaderError;
pub use crate::io::writer::AssetWriterError;

impl_into_asset_error!(AssetReaderError);
impl_into_asset_error!(AssetWriterError);

// -----------------------------------------------------------------------------
// AssetLoaderError

mod loader {
    use super::AssetError;
    use crate::path::AssetPath;
    use zlim_core::derive::Error;

    /// An error that can occur during asset loading.
    #[derive(Error, Debug, Clone)]
    #[error("Failed to load asset '{path}' with asset loader '{loader}': {error}")]
    pub struct AssetLoaderError {
        /// The path of the asset that was being loaded.
        pub path: AssetPath<'static>,
        /// The type path of the loader that failed.
        pub loader: &'static str,
        /// What the loader reported, as a message.
        pub error: String,
    }

    /// A panic that can occur during asset loading.
    #[derive(Error, Debug, Clone)]
    #[error("Failed to load asset '{path}', asset loader '{loader}' panicked")]
    pub struct AssetLoaderPanic {
        /// The path of the asset that was being loaded.
        pub path: AssetPath<'static>,
        /// The type path of the loader that panicked.
        pub loader: &'static str,
    }

    impl_into_asset_error!(AssetLoaderError);
    impl_into_asset_error!(AssetLoaderPanic);
}

pub use loader::*;

// -----------------------------------------------------------------------------
// AssetSaverError & AssetSaverPanic

mod saver {
    use super::AssetError;
    use crate::path::AssetPath;
    use zlim_core::derive::Error;

    /// An error that can occur during asset saving.
    #[derive(Error, Debug, Clone)]
    #[error("Failed to save asset '{path}' with asset saver '{saver}': {error}")]
    pub struct AssetSaverError {
        /// The path of the asset that was being saved.
        pub path: AssetPath<'static>,
        /// The type path of the saver that failed.
        pub saver: &'static str,
        /// What the saver reported, as a message.
        pub error: String,
    }

    /// A panic that can occur during asset saving.
    #[derive(Error, Debug, Clone)]
    #[error("Failed to save asset '{path}', asset saver '{saver}' panicked")]
    pub struct AssetSaverPanic {
        /// The path of the asset that was being saved.
        pub path: AssetPath<'static>,
        /// The type path of the saver that panicked.
        pub saver: &'static str,
    }

    impl_into_asset_error!(AssetSaverError);
    impl_into_asset_error!(AssetSaverPanic);
}

pub use saver::*;

// -----------------------------------------------------------------------------
// AssetTransformerError & AssetTransformerPanic

mod transformer {
    use super::AssetError;
    use zlim_core::derive::Error;

    /// An error that can occur during asset transforming.
    #[derive(Error, Debug, Clone)]
    #[error("Failed to transform asset with asset transformer '{transformer}': {error}")]
    pub struct AssetTransformerError {
        /// The type path of the transformer that failed.
        pub transformer: &'static str,
        /// What the transformer reported, as a message.
        pub error: String,
    }

    /// A panic that can occur during asset transforming.
    #[derive(Error, Debug, Clone)]
    #[error("Failed to transform asset,, transformer '{transformer}' panicked")]
    pub struct AssetTransformerPanic {
        /// The type path of the transformer that panicked.
        pub transformer: &'static str,
    }

    impl_into_asset_error!(AssetTransformerError);
    impl_into_asset_error!(AssetTransformerPanic);
}

pub use transformer::*;

// -----------------------------------------------------------------------------
// AssetMetaParseError

pub use crate::meta::{AssetMetaParseError, MetaParseError};

impl_into_asset_error!(MetaParseError);
impl_into_asset_error!(AssetMetaParseError);

// -----------------------------------------------------------------------------
// AmbiguousName

mod ambiguous {
    use super::AssetError;
    use zlim_core::derive::Error;

    /// An error that occurs when a short type name selects several registered types.
    ///
    /// Registries that index their entries by short type name as well as by fully-qualified type path
    /// (processors, loaders, savers) cannot select one when several entries share the short name. This
    /// names what was being looked up and every type path that could have been meant, so the report is
    /// the same whichever registry it came from.
    #[derive(Error, Debug, Clone, PartialEq, Eq)]
    #[error("The {service} name '{type_name}' is ambiguous on following: `{type_paths:?}`")]
    pub struct AmbiguousName {
        /// What was being looked up, e.g. `"processor"` / `"loader"` / `"saver"`.
        pub service: &'static str,
        /// The short type name that was used.
        pub type_name: &'static str,
        /// The type paths of the registered types that share that name.
        pub type_paths: Vec<&'static str>,
    }

    impl_into_asset_error!(AmbiguousName);

    impl AmbiguousName {
        /// Builds the error for a name that selected several types, before its candidates are known.
        pub fn new(service: &'static str, type_name: &'static str) -> Self {
            Self {
                service,
                type_name,
                type_paths: Vec::new(),
            }
        }

        /// Collects the type paths that `type_name` could have meant out of a registry's type paths.
        ///
        /// A name without a generic argument (`TextProcessor`) is a plain type name, so every type path
        /// containing it is a candidate (`my_crate::TextProcessor`, `my_crate::nested::TextProcessor`).
        ///
        /// A name with one (`TextProcessor<Config>`) names a specific instantiation, whose
        /// fully-qualified form spells its arguments as full paths of their own — the short form is
        /// therefore no substring of any type path, and every one of them is a candidate.
        pub fn collect_paths(&mut self, paths: impl Iterator<Item = &'static str>) {
            if self.type_name.contains('<') {
                self.type_paths = paths.collect();
            } else {
                self.type_paths = paths.filter(|path| path.contains(self.type_name)).collect();
            }
        }
    }
}

pub use ambiguous::*;

// -----------------------------------------------------------------------------
// Missing

mod missing {
    use super::AssetError;
    use crate::ident::AssetSourceId;
    use crate::path::AssetPath;
    use core::any::TypeId;
    use zlim_core::derive::Error;

    /// An error returned when an [`AssetSource`] does not exist for a given id.
    ///
    /// [`AssetSource`]: crate::source::AssetSource
    #[derive(Error, Debug, Clone, PartialEq, Eq)]
    #[error("Asset Source '{_0}' does not exist")]
    pub struct MissingAssetSource(pub AssetSourceId);

    /// An error returned when an [`AssetWriter`] does not exist for a given id.
    ///
    /// [`AssetWriter`]: crate::io::AssetWriter
    #[derive(Error, Debug, Clone, PartialEq, Eq)]
    #[error("Asset Source '{_0}' does not have an AssetWriter.")]
    pub struct MissingAssetWriter(pub AssetSourceId);

    /// An error returned when a processed [`AssetReader`] does not exist for a given id.
    ///
    /// [`AssetReader`]: crate::io::AssetReader
    #[derive(Error, Debug, Clone, PartialEq, Eq)]
    #[error("Asset Source '{_0}' does not have a processed AssetReader.")]
    pub struct MissingProcessedAssetReader(pub AssetSourceId);

    /// An error returned when a processed [`AssetWriter`] does not exist for a given id.
    ///
    /// [`AssetWriter`]: crate::io::AssetWriter
    #[derive(Error, Debug, Clone, PartialEq, Eq)]
    #[error("Asset Source '{_0}' does not have a processed AssetWriter.")]
    pub struct MissingProcessedAssetWriter(pub AssetSourceId);

    /// An error that occurs when an `AssetLoader` is not registered for a given identifier.
    #[derive(Error, Debug, Clone)]
    #[error("No asset loader found with {_0}")]
    pub struct MissingAssetLoader(pub String);

    /// An error that occurs when an `AssetTransformer` is not registered for a given identifier.
    #[derive(Error, Debug, Clone)]
    #[error("No asset transformer found with {_0}")]
    pub struct MissingAssetTransformer(pub String);

    /// An error that occurs when an `AssetSaver` is not registered for a given identifier.
    #[derive(Error, Debug, Clone)]
    #[error("No asset saver found with {_0}")]
    pub struct MissingAssetSaver(pub String);

    /// An error that occurs when an `AssetProcessor` is not registered for a given identifier.
    #[derive(Error, Debug, Clone)]
    #[error("No asset processor found with {_0}")]
    pub struct MissingAssetProcessor(pub String);

    impl_into_asset_error!(MissingAssetSource);
    impl_into_asset_error!(MissingAssetWriter);
    impl_into_asset_error!(MissingProcessedAssetReader);
    impl_into_asset_error!(MissingProcessedAssetWriter);
    impl_into_asset_error!(MissingAssetLoader);
    impl_into_asset_error!(MissingAssetTransformer);
    impl_into_asset_error!(MissingAssetSaver);
    impl_into_asset_error!(MissingAssetProcessor);

    macro_rules! impl_from_source_id {
        ($ident:ident) => {
            impl From<AssetSourceId> for $ident {
                fn from(value: AssetSourceId) -> Self {
                    Self(value)
                }
            }
        };
    }

    impl_from_source_id!(MissingAssetSource);
    impl_from_source_id!(MissingAssetWriter);
    impl_from_source_id!(MissingProcessedAssetReader);
    impl_from_source_id!(MissingProcessedAssetWriter);

    macro_rules! impl_from_string {
        ($ident:ident) => {
            impl From<String> for $ident {
                #[cold]
                #[inline]
                fn from(value: String) -> Self {
                    Self(value)
                }
            }
        };
    }

    impl_from_string!(MissingAssetLoader);
    impl_from_string!(MissingAssetTransformer);
    impl_from_string!(MissingAssetSaver);
    impl_from_string!(MissingAssetProcessor);

    /// Builds the message of a "missing" error: every identifier that was looked for, in the order
    /// the fields were set.
    ///
    /// The four `Missing*` registry errors are a single `String` each, so a builder keeps the call
    /// sites from formatting (and allocating) by hand:
    /// `MissingAssetLoader::from(MissingBuilder::new()…)`.
    ///
    /// Every field is optional, and [`finish`](Self::finish) spells out the ones that were set — the
    /// more a caller knows, the more the report says. The `may_with_*` setters are the counterparts
    /// of the `with_*` ones for the `Option` a call site already has (a type id nothing requested, a
    /// name an erased load does not know).
    #[derive(Default)]
    pub struct MissingBuilder {
        type_path: Option<&'static str>,
        type_name: Option<String>,
        extension: Option<String>,
        asset_path: Option<String>,
        asset_type: Option<&'static str>,
        asset_type_id: Option<TypeId>,
    }

    impl MissingBuilder {
        /// Starts an empty builder; every field is unset.
        #[inline]
        pub const fn new() -> Self {
            Self {
                type_path: None,
                type_name: None,
                extension: None,
                asset_path: None,
                asset_type: None,
                asset_type_id: None,
            }
        }

        /// Sets the registered type's fully-qualified path (`my_crate::MyLoader`).
        ///
        /// This is the strict form, and it is always a compile-time constant:
        /// a string read out of a `.meta` file is a type *name* and goes through
        /// [`with_type_name`](Self::with_type_name) instead.
        #[inline]
        pub fn with_type_path(self, s: &'static str) -> Self {
            Self {
                type_path: Some(s),
                ..self
            }
        }

        /// Sets the short type name (`MyLoader`).
        #[inline]
        pub fn with_type_name(self, s: impl Into<String>) -> Self {
            Self {
                type_name: Some(s.into()),
                ..self
            }
        }

        /// Sets the file extension that was looked up.
        #[inline]
        pub fn with_extension(self, s: impl Into<String>) -> Self {
            Self {
                extension: Some(s.into()),
                ..self
            }
        }

        /// Sets the asset path that was looked up.
        #[inline]
        pub fn with_asset_path(self, s: impl Into<String>) -> Self {
            Self {
                asset_path: Some(s.into()),
                ..self
            }
        }

        /// Sets the name of the *asset* that was looked up, as [`core::any::type_name`] spells it.
        #[inline]
        pub fn with_asset_type(self, s: &'static str) -> Self {
            Self {
                asset_type: Some(s),
                ..self
            }
        }

        /// Sets the id of the *asset* that was looked up.
        #[inline]
        pub fn with_asset_type_id(self, id: TypeId) -> Self {
            Self {
                asset_type_id: Some(id),
                ..self
            }
        }

        /// [`with_type_path`](Self::with_type_path) for an optional value.
        #[inline]
        pub fn may_with_type_path(self, s: Option<&'static str>) -> Self {
            Self {
                type_path: s,
                ..self
            }
        }

        /// [`with_type_name`](Self::with_type_name) for an optional value.
        #[inline]
        pub fn may_with_type_name(self, s: Option<impl Into<String>>) -> Self {
            Self {
                type_name: s.map(Into::into),
                ..self
            }
        }

        /// [`with_extension`](Self::with_extension) for an optional value.
        #[inline]
        pub fn may_with_extension(self, s: Option<impl Into<String>>) -> Self {
            Self {
                extension: s.map(Into::into),
                ..self
            }
        }

        /// [`with_asset_path`](Self::with_asset_path) for an optional value.
        #[inline]
        pub fn may_with_asset_path(self, s: Option<impl Into<String>>) -> Self {
            Self {
                asset_path: s.map(Into::into),
                ..self
            }
        }

        /// [`with_asset_type`](Self::with_asset_type) for an optional value.
        #[inline]
        pub fn may_with_asset_type(self, s: Option<&'static str>) -> Self {
            Self {
                asset_type: s,
                ..self
            }
        }

        /// [`with_asset_type_id`](Self::with_asset_type_id) for an optional value.
        #[inline]
        pub fn may_with_asset_type_id(self, id: Option<TypeId>) -> Self {
            Self {
                asset_type_id: id,
                ..self
            }
        }

        /// Spells out the fields that were set, as the message of a `Missing*` error.
        ///
        /// Nothing set is `_unknown_`: a call site with no identifying information at all.
        pub fn finish(self) -> String {
            let mut buffer = String::new();
            if let Some(type_path) = self.type_path {
                buffer.push_str("type_path `");
                buffer.push_str(type_path);
                buffer.push_str("`;`");
            }
            if let Some(type_name) = self.type_name {
                buffer.push_str("name `");
                buffer.push_str(&type_name);
                buffer.push_str("`;`");
            }
            if let Some(extension) = self.extension {
                buffer.push_str("extension `");
                buffer.push_str(&extension);
                buffer.push_str("`;`");
            }
            if let Some(asset_path) = self.asset_path {
                buffer.push_str("asset `");
                buffer.push_str(&asset_path);
                buffer.push_str("`;`");
            }
            if let Some(asset_type) = self.asset_type {
                buffer.push_str("asset `");
                buffer.push_str(asset_type);
                buffer.push_str("`;`");
            }
            if let Some(asset_type_id) = self.asset_type_id {
                use core::fmt::Write;
                buffer.push_str("asset `");
                write!(buffer, "{asset_type_id:?}").unwrap();
                buffer.push_str("`;`");
            }
            if buffer.is_empty() {
                buffer.push_str("_unknown_");
            }
            buffer
        }
    }

    macro_rules! impl_missing_builder {
        ($ident:ident) => {
            impl From<MissingBuilder> for $ident {
                fn from(value: MissingBuilder) -> Self {
                    Self(value.finish())
                }
            }
        };
    }

    impl_missing_builder!(MissingAssetLoader);
    impl_missing_builder!(MissingAssetTransformer);
    impl_missing_builder!(MissingAssetSaver);
    impl_missing_builder!(MissingAssetProcessor);

    /// An error returned when a handle is requested for an asset type that has no registered
    /// handle provider, identified by its [`TypeId`].
    #[derive(Error, Debug, Clone, PartialEq, Eq)]
    #[error("Cannot allocate a handle because no handle provider exists for asset type {_0:?}")]
    pub struct MissingHandleProvider(pub TypeId);

    impl_into_asset_error!(MissingHandleProvider);

    /// An error returned when the labeled sub-asset of a loaded asset is requested, but the loaded
    /// asset does not contain that label.
    #[derive(Error, Debug, Clone)]
    #[error(
        "The file at '{path}' does not contain the labeled asset \
        '{label}'; it contains the following assets: {all_labels:?}"
    )]
    pub struct MissingLabeledAsset {
        /// The path of the loaded asset that does not contain the label.
        pub path: AssetPath<'static>,
        /// The label that was requested.
        pub label: String,
        /// The labels the loaded asset does contain.
        pub all_labels: Vec<String>,
    }

    impl_into_asset_error!(MissingLabeledAsset);
}

pub use missing::*;

// -----------------------------------------------------------------------------
// Path

mod path {
    use super::AssetError;
    use crate::path::AssetPath;
    use zlim_core::derive::Error;

    /// An error returned when an asset is read or written with an empty path.
    #[derive(Error, Debug, Clone)]
    #[error("Attempted to read or write an asset with an empty path '{_0}'.")]
    pub struct EmptyPathError(pub AssetPath<'static>);

    /// An error returned when an asset path escapes the source root and the approval mode rejects
    /// it.
    #[derive(Error, Debug, Clone)]
    #[error("Asset path '{_0}' is unapproved (escapes the source root). See UnapprovedPathMode.")]
    pub struct UnapprovedPath(pub AssetPath<'static>);

    /// An error that occurs when an asset without an extension is processed.
    #[derive(Error, Debug, Clone)]
    #[error("Assets without extensions are not supported.")]
    pub struct ExtensionRequired;

    impl_into_asset_error!(EmptyPathError);
    impl_into_asset_error!(UnapprovedPath);
    impl_into_asset_error!(ExtensionRequired);
}

pub use path::*;

// -----------------------------------------------------------------------------
// Invalid Loading

mod invalid_load {
    use super::AssetError;
    use crate::path::AssetPath;
    use zlim_core::derive::Error;

    /// Error returned when an asset configured to be ignored is loaded directly.
    #[derive(Error, Debug, Clone)]
    #[error("Asset '{_0}' is configured to be ignored. It cannot be loaded.")]
    pub struct CannotLoadIgnoredAsset(pub AssetPath<'static>);

    /// Error returned when an asset configured to be processed is loaded directly.
    #[derive(Error, Debug, Clone)]
    #[error("Asset '{_0}' is configured to be processed. It cannot be loaded directly.")]
    pub struct CannotLoadProcessedAsset(pub AssetPath<'static>);

    impl_into_asset_error!(CannotLoadIgnoredAsset);
    impl_into_asset_error!(CannotLoadProcessedAsset);
}

pub use invalid_load::*;

// -----------------------------------------------------------------------------
// Settings

mod settings {
    use super::AssetError;
    use zlim_core::derive::Error;

    /// An error that occurs when settings of another type are passed into a loader or a processor.
    ///
    /// It can only come from a registry bug: a `.meta` is deserialized by the very loader or processor
    /// it names, so its settings always have that type.
    ///
    /// Which type was passed in is not recoverable — `dyn Settings` erases it — so the error names the
    /// type that was *expected* instead: that is what tells the reader which of the two sides is wrong.
    #[derive(Error, Debug, Clone)]
    #[error(
        "The wrong settings type was passed into a loader / saver / .. : `{source}` \
        was expected. This is probably an internal implementation error."
    )]
    pub struct MismatchedSettingsType {
        /// The settings type that was expected, as [`core::any::type_name`] spells it.
        pub source: &'static str,
    }

    impl_into_asset_error!(MismatchedSettingsType);

    impl MismatchedSettingsType {
        /// Builds the error for the settings type that was expected.
        ///
        /// The type is a parameter (and may be unsized) so that the caller names the type it has —
        /// usually its own `T::Settings` — and [`core::any::type_name`] spells it out.
        #[inline]
        pub fn from_source<T: ?Sized>() -> Self {
            Self {
                source: ::core::any::type_name::<T>(),
            }
        }
    }
}

pub use settings::*;

// -----------------------------------------------------------------------------
// Meta Io Extensions

mod meta_io {
    use super::AssetError;
    use super::{AssetReaderError, AssetWriterError};
    use super::{MissingAssetLoader, MissingAssetSource, MissingAssetWriter};
    use crate::path::AssetPath;
    use zlim_core::derive::Error;

    /// An error that occurs when the metadata of an asset cannot be read.
    #[derive(Error, Debug, Clone)]
    #[error("Failed to read asset metadata for '{path}': {error}")]
    pub struct AssetMetaReadError {
        /// The asset whose metadata could not be read.
        pub path: AssetPath<'static>,
        /// The error the reader reported.
        pub error: AssetReaderError,
    }

    /// An error returned when an asset's `.meta` file could not be written.
    #[derive(Error, Debug, Clone)]
    pub enum AssetMetaWriteError {
        /// A `.meta` file already exists and overwriting it was not requested.
        #[error("asset meta file already exists, so avoiding overwrite")]
        MetaAlreadyExists,
        /// The `.meta` bytes could not be built.
        #[error("failed to build meta: {_0}")]
        BuildError(String),
        /// Writing the `.meta` file failed.
        #[error("failed to write default asset meta file: {_0}")]
        WriteError(AssetWriterError),
        /// Checking whether a `.meta` file already exists failed.
        #[error("failed to check existing asset meta file: {_0}")]
        CheckError(AssetReaderError),
        /// The asset source is not registered.
        #[error(transparent)]
        MissingAssetSource(MissingAssetSource),
        /// The asset source has no writer.
        #[error(transparent)]
        MissingAssetWriter(MissingAssetWriter),
        /// No loader is registered for the asset, so no default `.meta` could be named.
        #[error(transparent)]
        MissingAssetLoader(MissingAssetLoader),
    }

    impl From<MissingAssetLoader> for AssetMetaWriteError {
        #[cold]
        fn from(value: MissingAssetLoader) -> Self {
            Self::MissingAssetLoader(value)
        }
    }

    impl From<MissingAssetSource> for AssetMetaWriteError {
        #[cold]
        fn from(value: MissingAssetSource) -> Self {
            Self::MissingAssetSource(value)
        }
    }

    impl From<MissingAssetWriter> for AssetMetaWriteError {
        #[cold]
        fn from(value: MissingAssetWriter) -> Self {
            Self::MissingAssetWriter(value)
        }
    }

    impl From<AssetWriterError> for AssetMetaWriteError {
        #[cold]
        fn from(value: AssetWriterError) -> Self {
            Self::WriteError(value)
        }
    }

    impl From<AssetReaderError> for AssetMetaWriteError {
        #[cold]
        fn from(value: AssetReaderError) -> Self {
            Self::CheckError(value)
        }
    }

    impl_into_asset_error!(AssetMetaReadError);
    impl_into_asset_error!(AssetMetaWriteError);
}

pub use meta_io::*;

// -----------------------------------------------------------------------------
// Wait

mod wait {
    use super::AssetError;
    use std::sync::Arc;
    use zlim_core::derive::Error;

    /// An error when attempting to wait asynchronously for an [`Asset`] to load.
    ///
    /// [`Asset`]: crate::asset::Asset
    #[derive(Error, Debug, Clone)]
    pub enum WaitForAssetError {
        /// The id is a UUID: UUID assets are never loaded, so there is nothing to wait for.
        #[error("tried to wait for an uuid asset that is unsupported")]
        Uuid,
        /// The asset is not being loaded; waiting for it is meaningless.
        #[error("tried to wait for an asset that is not being loaded")]
        NotLoaded,
        /// The asset failed to load.
        #[error("failed to wait for an asset: {_0}")]
        Failed(Arc<AssetError>),
        /// A dependency of the asset failed to load.
        #[error("failed to wait for asset dependency: {_0}")]
        DependencyFailed(Arc<AssetError>),
    }

    impl_into_asset_error!(WaitForAssetError);
}

pub use wait::*;

// -----------------------------------------------------------------------------
// ReadAssetBytes

mod read_asset_bytes {
    use super::{AssetError, AssetMetaParseError, MissingAssetSource};
    use super::{AssetReaderError, MissingProcessedAssetReader};
    use crate::path::AssetPath;
    use zlim_core::derive::Error;

    /// An error produced when calling [`LoadContext::read_asset_bytes`].
    ///
    /// [`LoadContext::read_asset_bytes`]: crate::loader::LoadContext::read_asset_bytes
    #[derive(Error, Debug)]
    pub enum ReadAssetBytesError {
        /// The asset path was empty.
        #[error("Attempted to load an asset with an empty path `{_0}`")]
        EmptyPath(AssetPath<'static>),

        /// The asset source could not be read.
        #[error(transparent)]
        AssetReaderError(AssetReaderError),

        /// The asset's `.meta` file could not be parsed.
        #[error(transparent)]
        AssetMetaParseError(AssetMetaParseError),

        /// No asset source with that id is registered.
        #[error(transparent)]
        MissingAssetSource(MissingAssetSource),

        /// The source has no processed reader, but the server is in processed mode.
        #[error(transparent)]
        MissingProcessedAssetReader(MissingProcessedAssetReader),

        /// A hash was required (`populate_hashes`) but this asset's `.meta` file has none.
        #[error("LoadContext requires asset hash for '{_0}', but none was provided")]
        MissingAssetHash(AssetPath<'static>),
    }

    impl_into_asset_error!(ReadAssetBytesError);

    impl From<AssetReaderError> for ReadAssetBytesError {
        #[cold]
        #[inline]
        fn from(value: AssetReaderError) -> Self {
            Self::AssetReaderError(value)
        }
    }

    impl From<AssetMetaParseError> for ReadAssetBytesError {
        #[cold]
        #[inline]
        fn from(value: AssetMetaParseError) -> Self {
            Self::AssetMetaParseError(value)
        }
    }

    impl From<MissingAssetSource> for ReadAssetBytesError {
        #[cold]
        #[inline]
        fn from(value: MissingAssetSource) -> Self {
            Self::MissingAssetSource(value)
        }
    }

    impl From<MissingProcessedAssetReader> for ReadAssetBytesError {
        #[cold]
        #[inline]
        fn from(value: MissingProcessedAssetReader) -> Self {
            Self::MissingProcessedAssetReader(value)
        }
    }
}

pub use read_asset_bytes::ReadAssetBytesError;

// -----------------------------------------------------------------------------
// LoadDirectError

mod load_direct {
    use super::{AssetError, AssetLoadError};
    use crate::path::AssetPath;
    use zlim_core::derive::Error;

    /// An error produced when loading a nested asset through
    /// [`NestedLoadBuilder`].
    ///
    /// [`NestedLoadBuilder`]: crate::loader::NestedLoadBuilder
    #[derive(Error, Debug)]
    pub enum LoadDirectError {
        /// The asset path was empty.
        #[error("Attempted to load an asset with an empty path `{_0}`")]
        EmptyPath(AssetPath<'static>),

        /// The asset path escapes its source root and the approval mode rejected it.
        #[error("Attempted to read an unapproved asset path `{_0}`")]
        UnapprovedPath(AssetPath<'static>),

        /// A labeled sub-asset was requested: sub-assets cannot be loaded directly.
        ///
        /// An asset's labeled sub-assets are produced by the loader of its base asset, so an immediate
        /// nested load cannot read one on its own. Loading the base and reading the label from it (or
        /// requesting the labeled path through a *deferred* load, which resolves against the base) is
        /// the way to get one.
        #[error(
            "Requested to load the sub-asset of `{_0}`, but loading a sub-asset \
            directly is not supported; load the base asset and read the labeled \
            asset from its `LoadContext`"
        )]
        RequestedSubAsset(AssetPath<'static>),

        /// The loaded asset does not have the type the caller asked for.
        #[error("Asset type mismatched, expect `{expect}`, but actual (loaded) is `{actual}`.")]
        AssetTypeMismatch {
            /// The path that was loaded.
            path: AssetPath<'static>,
            /// The type name the caller asked for (the generic parameter of the load).
            expect: &'static str,
            /// The type name that was actually loaded.
            actual: &'static str,
        },

        /// The asset (or dependency) itself failed to load.
        #[error("Failed to load the dependency '{asset}': {error}")]
        AssetLoadError {
            /// The dependency that failed to load.
            asset: AssetPath<'static>,
            /// Why the dependency failed.
            error: AssetLoadError,
        },
    }

    impl_into_asset_error!(LoadDirectError);
}

pub use load_direct::*;

// -----------------------------------------------------------------------------
// Handle

mod handle {
    use super::AssetError;
    use crate::path::AssetPath;
    use core::any::TypeId;
    use zlim_core::derive::Error;

    /// An error that occurs when the requested handle type doesn't match the actual loaded asset type.
    #[derive(Error, Debug, Clone)]
    #[error(
        "Requested handle of type {requested:?} for asset '{path}' does not match \
        actual asset type '{actual_asset_name}', which used loader '{loader_name}'"
    )]
    pub struct RequestedHandleTypeMismatch {
        /// The path of the asset.
        pub path: AssetPath<'static>,
        /// The requested type id of handle.
        pub requested: TypeId,
        /// The loader (debug) name used to load the asset.
        pub loader_name: &'static str,
        /// The actual loaded asset (debug) type name.
        pub actual_asset_name: &'static str,
    }

    impl_into_asset_error!(RequestedHandleTypeMismatch);
}

pub use handle::*;

// -----------------------------------------------------------------------------
