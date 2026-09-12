//! Asset source registry, builders, events and the errors they report.

mod error;
mod sources;

pub use error::{MissingAssetSource, MissingAssetWriter};
pub use error::{MissingProcessedAssetReader, MissingProcessedAssetWriter};
pub use sources::{AssetSource, AssetSourceBuilder};
pub use sources::{AssetSourceBuilders, AssetSources};
