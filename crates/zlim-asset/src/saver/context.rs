//! The context a save runs with: the path being written, the asset to save, and — when there is one —
//! the [`AssetServer`] and [`World`] the save can consult.

use zlim_core::world::World;

use crate::asset::Asset;
use crate::path::AssetPath;
use crate::saver::{ErasedSavedAsset, SavedAsset};
use crate::server::AssetServer;

// -----------------------------------------------------------------------------
// SaverContext
// -----------------------------------------------------------------------------

/// The context an [`AssetSaver`] runs with.
///
/// It carries what a save is about — the path being written and the asset itself — and, when there is
/// one, the ambient state a saver may consult: the [`AssetServer`] it was picked by and the [`World`]
/// the asset lives in.
///
/// # The two forms of a save, and why the ambient state is optional
///
/// A save happens in one of two places, and they have different things to offer:
///
/// - **Processing** turns a source asset into its processed form and writes that. It has everything
///   it needs already loaded, and the asset it saves is complete — value, labeled sub-assets and both
///   label indexes — so it has no reason to reach back into the server, and no world. It builds a
///   context with neither: [`server`](Self::server) and [`world`](Self::world) are both `None`.
/// - **The save queue** writes an asset that already exists in the world. It reads the value out of
///   the world to do so, so it has both, and hands them over.
///
/// So a saver that needs the server or the world is one that only makes sense for a queued save, and it
/// can say so by asking. A saver content with the path and the asset works in both places.
///
/// # The asset
///
/// The asset is held type-erased, because the two forms see different things: a processor has the
/// [`SavedAsset`] of the value it just produced, and the queue only has the value itself. A saver asks
/// for it as its own type with [`asset`](Self::asset).
///
/// [`AssetSaver`]: crate::saver::AssetSaver
/// [`AssetSaver::save`]: crate::saver::AssetSaver::save
pub struct SaverContext<'a> {
    /// The server this save is running on, when it is running on one.
    server: Option<&'a AssetServer>,

    /// The world the asset was read out of, when the save came through the queue.
    world: Option<&'a World>,

    /// The path being written, which is the *source* path of the asset.
    path: AssetPath<'static>,

    /// The asset being saved.
    asset: ErasedSavedAsset<'a>,

    /// Whether the asset is known to be missing parts its description has.
    is_incomplete: bool,
}

impl<'a> SaverContext<'a> {
    /// The context of a save that runs as part of processing.
    ///
    /// Such a save has no ambient state to offer — no server and no world. What it does have is a
    /// complete asset, one that came out of a load and a transform and so still carries its labeled
    /// sub-assets, and the path those bytes belong to.
    ///
    /// [`AssetProcessor::process`]: crate::processor::AssetProcessor::process
    #[inline]
    pub fn complete(path: AssetPath<'static>, asset: ErasedSavedAsset<'a>) -> Self {
        Self {
            server: None,
            world: None,
            path,
            asset,
            is_incomplete: false,
        }
    }

    /// The context of a save that came through the queue, with the [`World`] the asset was read out of.
    ///
    /// Such an asset is **incomplete**: the queue resolves a handle to the value in the world, and the
    /// value is all the world keeps. The labeled sub-assets a load would have produced are gone by
    /// then, so the saver is told — see the type documentation.
    #[inline]
    pub fn incomplete(
        world: &'a World,
        server: &'a AssetServer,
        path: AssetPath<'static>,
        asset: ErasedSavedAsset<'a>,
    ) -> Self {
        Self {
            server: Some(server),
            world: Some(world),
            path,
            asset,
            is_incomplete: true,
        }
    }

    /// Returns the [`World`] the asset was read out of, if this save has one.
    ///
    /// `None` for a save that runs as part of processing: that path has no world at all. A queued save
    /// has one, and a saver may look at what it needs there — a handle the asset points at, say.
    #[inline]
    pub fn world(&self) -> Option<&'a World> {
        self.world
    }

    /// Returns the [`AssetServer`] this save is running on, if it is running on one.
    ///
    /// `None` for a save that runs as part of processing: that path has the asset server's machinery
    /// behind it, but hands none of it to the saver. A queued save has the server, and it is how a saver
    /// reaches the rest of the asset system — the assets a value points at, the sources it may read
    /// from, the registry that named this saver.
    #[inline]
    pub fn server(&self) -> Option<&'a AssetServer> {
        self.server
    }

    /// Returns the path being written.
    ///
    /// This is the *source* side of the asset: the bytes a save writes are the runtime asset, not the
    /// processed form a loader produces from it.
    #[inline]
    pub fn path(&self) -> &AssetPath<'static> {
        &self.path
    }

    /// Returns the asset being saved, as the saver's own type.
    ///
    /// # Panics
    ///
    /// Panics if the asset is not an `A`. The registry pairs a saver with an asset type,
    /// so a mismatch is a registry bug rather than something a saver has to handle.
    #[inline]
    pub fn asset<A: Asset>(&self) -> SavedAsset<'_, A> {
        self.asset.with_type::<A>()
    }

    /// Returns the asset being saved, type-erased.
    ///
    /// For a saver that reaches further than its own asset type — one that walks labeled sub-assets
    /// without knowing what they are, say.
    #[inline]
    pub fn erased_asset(&self) -> &ErasedSavedAsset<'a> {
        &self.asset
    }

    /// Returns whether the asset is known to be missing parts its own description carries.
    ///
    /// A queued save resolves a [`Handle`](crate::handle::Handle) to the value in the world, and the
    /// value is all the world keeps: `SavedAsset` also carries the labeled sub-assets and their two
    /// indexes, which only a load produces. So a queued save is `incomplete`, and a save that runs as
    /// part of processing is not.
    ///
    /// What to do about it is the saver's business — a format that would have written the sub-assets
    /// into itself can refuse, or fall back to writing the value alone. This only reports what is
    /// known; it is not a promise that a `false` here means the asset is complete in every sense.
    #[inline]
    pub fn is_incomplete(&self) -> bool {
        self.is_incomplete
    }
}

impl core::fmt::Debug for SaverContext<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SaverContext")
            .field("path", &self.path)
            .field("asset_type", &self.asset.asset_type_id())
            .field("server", &self.server.is_some())
            .field("world", &self.world.is_some())
            .field("is_incomplete", &self.is_incomplete)
            .finish()
    }
}
