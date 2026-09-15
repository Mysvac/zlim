//! The loader layer: the read half of the asset formats this engine knows.
//!
//! What lives here:
//!
//! - [`AssetLoader`] is a file format — it deserialises raw bytes into an asset — and
//!   [`ErasedAssetLoader`] is its type-erased mirror, for the code that drives a loader without
//!   knowing its type;
//! - [`LoadContext`] is what a load is recorded through: a loader reaches the rest of the asset
//!   system with it, and [`LoadContext::finish`] turns the value it produced into the result of the
//!   load, together with the sub-assets and the dependencies it registered on the way;
//! - [`NestedLoadBuilder`] is the nested-load API, which is how a loader loads another asset — or a
//!   file it reads itself — from inside its own load.
//!
//! The registry the server picks a loader from is `AssetLoaders`, which lives in this module and
//! stays crate-internal: it carries the reverse indexes (by loader type path, by type name, by
//! produced asset type and by file extension) that are consulted in that order, and it is reached
//! through the server rather than from here.
//!
//! [`LoadContext::finish`]: LoadContext::finish

mod context;
mod loader;
mod loaders;

pub use context::{LoadContext, NestedLoadBuilder};
pub use loader::{AssetLoader, ErasedAssetLoader};

pub(crate) use loaders::AssetLoaders;
