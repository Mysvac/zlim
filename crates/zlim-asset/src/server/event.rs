use std::borrow::Cow;

use crate::error::AssetLoadError;
use crate::handle::ErasedHandle;
use crate::ident::TypedAssetIndex;
use crate::loaded::ErasedLoadedAsset;
use crate::path::AssetPath;
use crate::server::builder::Guard;

/// What a load task reports back to the server.
///
/// The server drains these when it runs its event job, which is where the bookkeeping is
/// updated: `Failed` and `Loaded` settle one asset, and `FullyLoaded` — sent once the whole
/// dependency tree below an asset has settled — is what releases the tasks parked in
/// [`AssetServer::wait_for_asset_id`].
///
/// [`AssetServer::wait_for_asset_id`]: crate::server::AssetServer::wait_for_asset_id
#[non_exhaustive]
pub(crate) enum AssetServerEvent {
    /// A load failed: `error` is recorded for `index`, and for everything that waits on it.
    Failed {
        index: TypedAssetIndex,
        path: AssetPath<'static>,
        error: AssetLoadError,
    },
    /// A load succeeded: the server applies `loaded_asset` to the world, then settles the
    /// load states of `index` and of everything that waits on it.
    Loaded {
        index: TypedAssetIndex,
        loaded_asset: ErasedLoadedAsset,
    },
    /// The asset at `index` and every dependency below it are loaded: the server sends the typed
    /// asset event and wakes the tasks waiting for `index`.
    FullyLoaded { index: TypedAssetIndex },
}

/// One queued save request.
///
/// A [`SaveBuilder`](crate::server::SaveBuilder) fills it in and hands it to the server,
/// which applies it when it runs its save commands — not inside the builder call. `loader` and
/// `saver` are the names the caller forced, if any; `save_meta` decides whether the `.meta` that
/// carries the loader name is written at all.
pub(crate) struct SaveCommand {
    pub(crate) handle: ErasedHandle,
    pub(crate) path: AssetPath<'static>,
    pub(crate) loader: Option<Cow<'static, str>>,
    pub(crate) saver: Option<Cow<'static, str>>,
    pub(crate) save_meta: bool,
    pub(crate) override_unapproved: bool,
    pub(crate) guard: Option<Guard>,
}
