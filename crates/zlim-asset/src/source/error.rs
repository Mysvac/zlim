//! Errors reported when an asset source, writer or processed reader is missing.

use zlim_core::derive::Error;

use crate::ident::AssetSourceId;

// -----------------------------------------------------------------------------
// Errors

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
