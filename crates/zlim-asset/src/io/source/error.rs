use zlim_core::derive::Error;

use crate::ident::AssetSourceId;

// -----------------------------------------------------------------------------
// Errors

/// An error returned when an [`AssetSource`](crate::io::AssetSource) does not exist for a given id.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("Asset Source '{_0}' does not exist")]
pub struct MissingAssetSource(pub AssetSourceId<'static>);

/// An error returned when an [`AssetWriter`](crate::io::AssetWriter) does not exist for a given id.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("Asset Source '{_0}' does not have an AssetWriter.")]
pub struct MissingAssetWriter(pub AssetSourceId<'static>);

/// An error returned when a processed [`AssetReader`](crate::io::AssetReader) does not exist for a
/// given id.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("Asset Source '{_0}' does not have a processed AssetReader.")]
pub struct MissingProcessedAssetReader(pub AssetSourceId<'static>);

/// An error returned when a processed [`AssetWriter`](crate::io::AssetWriter) does not exist for a
/// given id.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("Asset Source '{_0}' does not have a processed AssetWriter.")]
pub struct MissingProcessedAssetWriter(pub AssetSourceId<'static>);
